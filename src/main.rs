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

// ---------- Data Structures ----------

#[derive(Deserialize)]
struct CreateMonitorReq {
    url: String,
    interval_seconds: i32,
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

// ---------- Main ----------

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL not set");
    println!("Connecting to DB at {}", database_url);

    let pool = PgPoolOptions::new()
        .max_connections(10) // scale better than 5
        .connect(&database_url)
        .await
        .expect("Failed to connect to DB");

    // Start background checker
    start_checker(pool.clone()).await;

    let app = Router::new()
        .route("/health", get(health))
        .route("/monitors", post(create_monitor).get(list_monitors))
        .route("/monitors/:id/uptime", get(get_uptime))
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
        "INSERT INTO monitors (url, interval_seconds) VALUES ($1, $2)"
    )
    .bind(&payload.url)
    .bind(payload.interval_seconds)
    .execute(&pool)
    .await
    .expect("Failed to insert monitor");

    Json(CreateMonitorRes { success: true })
}

// A) List monitors
async fn list_monitors(
    State(pool): State<PgPool>,
) -> Json<Vec<MonitorDto>> {
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

// C) Uptime analytics (99.9 / 99.99)
async fn get_uptime(
    State(pool): State<PgPool>,
    Path(monitor_id): Path<Uuid>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<UptimeRes> {
    let days: i32 = params
        .get("days")
        .and_then(|d| d.parse().ok())
        .unwrap_or(30);

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

// ---------- Background Worker ----------

async fn start_checker(pool: PgPool) {
    tokio::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10)) // avoid hanging checks
            .build()
            .unwrap();

        loop {
            // Fetch monitors (only what we need = low latency)
            let rows = sqlx::query("SELECT id, url FROM monitors")
                .fetch_all(&pool)
                .await
                .unwrap_or_default();

            let mut monitors = Vec::with_capacity(rows.len());
            for row in rows {
                let id: Uuid = row.get("id");
                let url: String = row.get("url");
                monitors.push(Monitor { id, url });
            }

            for m in monitors {
                let start = std::time::Instant::now();
                let res = client.get(&m.url).send().await;

                let (is_up, status_code, response_time_ms) = match res {
                    Ok(r) => {
                        let status = r.status().as_u16() as i32;
                        let ms = start.elapsed().as_millis() as i32;
                        (status < 500, Some(status), Some(ms))
                    }
                    Err(_) => (false, None, None),
                };

                // Insert result
                let _ = sqlx::query(
                    r#"
                    INSERT INTO checks (monitor_id, checked_at, status_code, response_time_ms, is_up)
                    VALUES ($1, $2, $3, $4, $5)
                    "#
                )
                .bind(m.id)
                .bind(Utc::now())
                .bind(status_code)
                .bind(response_time_ms)
                .bind(is_up)
                .execute(&pool)
                .await;
            }

            // Global tick (later we’ll do per-monitor scheduling)
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });
}