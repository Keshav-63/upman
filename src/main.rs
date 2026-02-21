use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, env, net::SocketAddr, time::Duration};
use tokio::net::TcpListener;
use tracing_subscriber;
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use dotenvy::dotenv;
use chrono::Utc;
use uuid::Uuid;

// Email
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use lettre::transport::smtp::authentication::Credentials;

// ---------- Data Structures ----------

#[derive(Deserialize)]
struct CreateMonitorReq {
    url: String,
    interval_seconds: i32,
    alert_email: Option<String>,
    alert_after_failures: Option<i32>,
}

#[derive(Serialize)]
struct CreateMonitorRes {
    success: bool,
}

struct Monitor {
    id: Uuid,
    url: String,
}

#[derive(Serialize)]
struct MonitorDto {
    id: Uuid,
    url: String,
    interval_seconds: i32,
}

#[derive(Serialize)]
struct UptimeRes {
    uptime: f64,
}

#[derive(Serialize)]
struct IncidentDto {
    id: Uuid,
    started_at: chrono::DateTime<chrono::Utc>,
    ended_at: Option<chrono::DateTime<chrono::Utc>>,
    status: String,
    reason: Option<String>,
}

#[derive(Serialize)]
struct LatencyRes {
    p50: Option<f64>,
    p95: Option<f64>,
    p99: Option<f64>,
}

#[derive(Serialize)]
struct MttrRes {
    mttr_seconds: Option<f64>,
}

#[derive(Serialize)]
struct AvailabilityPoint {
    day: chrono::DateTime<chrono::Utc>,
    uptime: f64,
}

// ---------- Main ----------

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL not set");
    println!("Connecting to DB at {}", database_url);

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await
        .expect("Failed to connect to DB");

    // Start background checker
    start_checker(pool.clone()).await;

    let app = Router::new()
        .route("/health", get(health))
        .route("/monitors", post(create_monitor).get(list_monitors))
        .route("/monitors/:id/uptime", get(get_uptime))
        .route("/monitors/:id/incidents", get(list_incidents))
        .route("/monitors/:id/latency", get(get_latency))
        .route("/monitors/:id/mttr", get(get_mttr))
        .route("/monitors/:id/availability", get(get_availability))
        .with_state(pool);

    let addr = SocketAddr::from(([0, 0, 0, 0], 8000));
    println!("Server running on {}", addr);

    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// ---------- Handlers ----------

async fn health() -> &'static str {
    "ok"
}

async fn create_monitor(
    State(pool): State<PgPool>,
    Json(payload): Json<CreateMonitorReq>,
) -> Json<CreateMonitorRes> {
    sqlx::query(
        "INSERT INTO monitors (url, interval_seconds, alert_email, alert_after_failures) VALUES ($1, $2, $3, $4)"
    )
    .bind(&payload.url)
    .bind(payload.interval_seconds)
    .bind(&payload.alert_email)
    .bind(payload.alert_after_failures.unwrap_or(1 )) // Default to 1 failure if not provided
    .execute(&pool)
    .await
    .expect("Failed to insert monitor");

    Json(CreateMonitorRes { success: true })
}

// A) List monitors
async fn list_monitors(State(pool): State<PgPool>) -> Json<Vec<MonitorDto>> {
    let rows = sqlx::query("SELECT id, url, interval_seconds FROM monitors")
        .fetch_all(&pool)
        .await
        .unwrap_or_default();

    let mut result = Vec::with_capacity(rows.len());

    for row in rows {
        let id: Uuid = row.get("id");
        let url: String = row.get("url");
        let interval_seconds: i32 = row.get("interval_seconds");

        result.push(MonitorDto {
            id,
            url,
            interval_seconds,
        });
    }

    Json(result)
}

// C) Uptime analytics
async fn get_uptime(
    State(pool): State<PgPool>,
    Path(monitor_id): Path<Uuid>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<UptimeRes> {
    let days: i32 = params.get("days").and_then(|d| d.parse().ok()).unwrap_or(30);

    let row = sqlx::query(
        r#"
        SELECT
          (SUM(CASE WHEN is_up THEN 1 ELSE 0 END)::float / NULLIF(COUNT(*), 0)) * 100 AS uptime
        FROM checks
        WHERE monitor_id = $1
          AND checked_at > now() - ($2 || ' days')::interval
        "#
    )
    .bind(monitor_id)
    .bind(days)
    .fetch_one(&pool)
    .await;

    let uptime = match row {
        Ok(r) => r.get::<Option<f64>, _>("uptime").unwrap_or(0.0),
        Err(_) => 0.0,
    };

    Json(UptimeRes { uptime })
}

// Incidents list
async fn list_incidents(
    State(pool): State<PgPool>,
    Path(monitor_id): Path<Uuid>,
) -> Json<Vec<IncidentDto>> {
    let rows = sqlx::query(
        r#"
        SELECT id, started_at, ended_at, status, reason
        FROM incidents
        WHERE monitor_id = $1
        ORDER BY started_at DESC
        "#
    )
    .bind(monitor_id)
    .fetch_all(&pool)
    .await
    .unwrap_or_default();

    let mut out = Vec::new();
    for r in rows {
        out.push(IncidentDto {
            id: r.get("id"),
            started_at: r.get("started_at"),
            ended_at: r.get("ended_at"),
            status: r.get("status"),
            reason: r.get("reason"),
        });
    }

    Json(out)
}

// Latency percentiles
async fn get_latency(
    State(pool): State<PgPool>,
    Path(monitor_id): Path<Uuid>,
) -> Json<LatencyRes> {
    let row = sqlx::query(
        r#"
        SELECT
          percentile_cont(0.50) WITHIN GROUP (ORDER BY response_time_ms) AS p50,
          percentile_cont(0.95) WITHIN GROUP (ORDER BY response_time_ms) AS p95,
          percentile_cont(0.99) WITHIN GROUP (ORDER BY response_time_ms) AS p99
        FROM checks
        WHERE monitor_id = $1
          AND checked_at > now() - interval '30 days'
          AND response_time_ms IS NOT NULL
        "#
    )
    .bind(monitor_id)
    .fetch_one(&pool)
    .await;

    if let Ok(r) = row {
        Json(LatencyRes {
            p50: r.get("p50"),
            p95: r.get("p95"),
            p99: r.get("p99"),
        })
    } else {
        Json(LatencyRes { p50: None, p95: None, p99: None })
    }
}

// MTTR
async fn get_mttr(
    State(pool): State<PgPool>,
    Path(monitor_id): Path<Uuid>,
) -> Json<MttrRes> {
    let row = sqlx::query(
        r#"
        SELECT AVG(EXTRACT(EPOCH FROM (ended_at - started_at))) AS mttr_seconds
        FROM incidents
        WHERE monitor_id = $1 AND ended_at IS NOT NULL
        "#
    )
    .bind(monitor_id)
    .fetch_one(&pool)
    .await;

    let mttr = match row {
        Ok(r) => r.get::<Option<f64>, _>("mttr_seconds"),
        Err(_) => None,
    };

    Json(MttrRes { mttr_seconds: mttr })
}

// Availability by day (for charts)
async fn get_availability(
    State(pool): State<PgPool>,
    Path(monitor_id): Path<Uuid>,
) -> Json<Vec<AvailabilityPoint>> {
    let rows = sqlx::query(
        r#"
        SELECT
          date_trunc('day', checked_at) AS day,
          (SUM(CASE WHEN is_up THEN 1 ELSE 0 END)::float / COUNT(*)) * 100 AS uptime
        FROM checks
        WHERE monitor_id = $1
        GROUP BY day
        ORDER BY day
        "#
    )
    .bind(monitor_id)
    .fetch_all(&pool)
    .await
    .unwrap_or_default();

    let mut out = Vec::new();
    for r in rows {
        out.push(AvailabilityPoint {
            day: r.get("day"),
            uptime: r.get("uptime"),
        });
    }

    Json(out)
}

// ---------- Email ----------

async fn send_email(to: &str, subject: &str, body: &str) {
    let smtp_host = match env::var("SMTP_HOST") {
        Ok(v) => v,
        Err(_) => {
            tracing::warn!("SMTP_HOST not set, skipping email");
            return;
        }
    };

    let smtp_port: u16 = match env::var("SMTP_PORT").ok().and_then(|v| v.parse().ok()) {
        Some(v) => v,
        None => {
            tracing::warn!("SMTP_PORT not set or invalid, skipping email");
            return;
        }
    };

    let smtp_user = match env::var("SMTP_USER") {
        Ok(v) => v,
        Err(_) => {
            tracing::warn!("SMTP_USER not set, skipping email");
            return;
        }
    };

    let smtp_pass = match env::var("SMTP_PASS") {
        Ok(v) => v,
        Err(_) => {
            tracing::warn!("SMTP_PASS not set, skipping email");
            return;
        }
    };

    let from = match env::var("ALERT_FROM") {
        Ok(v) => v,
        Err(_) => {
            tracing::warn!("ALERT_FROM not set, skipping email");
            return;
        }
    };

    let email = Message::builder()
        .from(from.parse().unwrap())
        .to(to.parse().unwrap())
        .subject(subject)
        .body(body.to_string())
        .unwrap();

    let creds = Credentials::new(smtp_user, smtp_pass);

    let mailer = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&smtp_host)

        .unwrap()
        .port(smtp_port)
        .credentials(creds)
        .build();

    match mailer.send(email).await {
        Ok(_) => {
            tracing::info!("📧 Email sent successfully to {} | subject: {}", to, subject);
        }
        Err(e) => {
            tracing::error!("❌ Failed to send email to {}: {:?}", to, e);
        }
    }
}

// ---------- Background Worker (Scheduler + Incident Engine) ----------

async fn start_checker(pool: PgPool) {
    tokio::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap();

        loop {
            // Fetch only due monitors (scalable)
            let rows = sqlx::query(
                r#"
                SELECT id, url, interval_seconds, last_checked_at, alert_after_failures, alert_email
                FROM monitors
                WHERE
                  is_paused = FALSE
                  AND (
                    last_checked_at IS NULL
                    OR now() - last_checked_at >= (interval_seconds || ' seconds')::interval
                  )
                LIMIT 100
                "#
            )
            .fetch_all(&pool)
            .await
            .unwrap_or_default();

            for row in rows {
                let id: Uuid = row.get("id");
                let url: String = row.get("url");
                let threshold: i32 = row.get("alert_after_failures");
                let alert_email: Option<String> = row.get("alert_email");

                let start = std::time::Instant::now();
                let res = client.get(&url).send().await;

                let (is_up, status_code, response_time_ms, error_message) = match res {
                    Ok(r) => {
                        let status = r.status().as_u16() as i32;
                        let ms = start.elapsed().as_millis() as i32;
                        (status < 500, Some(status), Some(ms), None)
                    }
                    Err(e) => (false, None, None, Some(e.to_string())),
                };

                // Insert check
                let _ = sqlx::query(
                    r#"
                    INSERT INTO checks (monitor_id, checked_at, status_code, response_time_ms, is_up, error_message)
                    VALUES ($1, $2, $3, $4, $5, $6)
                    "#
                )
                .bind(id)
                .bind(Utc::now())
                .bind(status_code)
                .bind(response_time_ms)
                .bind(is_up)
                .bind(error_message)
                .execute(&pool)
                .await;

                // Update last_checked_at
                let _ = sqlx::query(
                    "UPDATE monitors SET last_checked_at = now() WHERE id = $1"
                )
                .bind(id)
                .execute(&pool)
                .await;

                // Count last N failures
                let fail_row = sqlx::query(
                    r#"
                    SELECT COUNT(*) as fails
                    FROM (
                      SELECT is_up
                      FROM checks
                      WHERE monitor_id = $1
                      ORDER BY checked_at DESC
                      LIMIT $2
                    ) t
                    WHERE is_up = false
                    "#
                )
                .bind(id)
                .bind(threshold)
                .fetch_one(&pool)
                .await
                .ok();

                let fails: i64 = fail_row.and_then(|r| r.try_get("fails").ok()).unwrap_or(0);

                // Check open incident
                let open_incident = sqlx::query(
                    "SELECT id FROM incidents WHERE monitor_id = $1 AND status = 'open' LIMIT 1"
                )
                .bind(id)
                .fetch_optional(&pool)
                .await
                .ok()
                .flatten();

                // Open incident
                if fails >= threshold as i64 && open_incident.is_none() {
                    let _ = sqlx::query(
                        r#"
                        INSERT INTO incidents (monitor_id, started_at, status, reason)
                        VALUES ($1, now(), 'open', 'Consecutive failures exceeded threshold')
                        "#
                    )
                    .bind(id)
                    .execute(&pool)
                    .await;

                    if let Some(email) = alert_email.clone() {
                        let subject = format!("🚨 DOWN: {}", url);
                        let body = format!(
                            "Monitor is DOWN\n\nURL: {}\nTime: {}\nReason: {} consecutive failures\n",
                            url, Utc::now(), threshold
                        );
                        send_email(&email, &subject, &body).await;
                    }
                }

                // Resolve incident on recovery
                if is_up {
                    if let Some(inc) = open_incident {
                        let inc_id: Uuid = inc.get("id");

                        let _ = sqlx::query(
                            "UPDATE incidents SET status = 'resolved', ended_at = now() WHERE id = $1"
                        )
                        .bind(inc_id)
                        .execute(&pool)
                        .await;

                        if let Some(email) = alert_email {
                            let subject = format!("✅ RECOVERED: {}", url);
                            let body = format!(
                                "Monitor has RECOVERED\n\nURL: {}\nTime: {}\n",
                                url, Utc::now()
                            );
                            send_email(&email, &subject, &body).await;
                        }
                    }
                }
            }

            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}