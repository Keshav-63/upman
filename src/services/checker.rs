use sqlx::{PgPool, Row};
use std::time::Duration;
use uuid::Uuid;
use chrono::Utc;

use crate::services::email::send_email;

/// Compute exponential backoff with a cap
fn compute_backoff_seconds(failure_count: i32) -> i64 {
    let base: i64 = 5; // base delay in seconds
    let max: i64 = 300; // cap at 5 minutes
    let exp = 2_i64.pow(failure_count.min(10) as u32);
    (base * exp).min(max)
}

/// Add random jitter to prevent thundering herd
fn jitter_seconds() -> i64 {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    rng.gen_range(0..5) // 0–5 seconds
}

/// Atomically claim jobs ready for processing (multi-worker safe)
async fn claim_jobs(pool: &PgPool, limit: i64) -> Vec<sqlx::postgres::PgRow> {
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(_) => return vec![],
    };

    let rows = sqlx::query(
        r#"
        WITH cte AS (
            SELECT id
            FROM monitors
            WHERE
              is_paused = FALSE
              AND next_run_at <= now()
              AND (lease_until IS NULL OR lease_until < now())
            ORDER BY next_run_at
            LIMIT $1
            FOR UPDATE SKIP LOCKED
        )
        UPDATE monitors
        SET lease_until = now() + interval '60 seconds'
        FROM cte
        WHERE monitors.id = cte.id
        RETURNING monitors.*;
        "#,
    )
    .bind(limit)
    .fetch_all(&mut *tx)
    .await
    .unwrap_or_default();

    let _ = tx.commit().await;
    rows
}

/// Start the uptime checker background worker
pub async fn start_checker(pool: PgPool) {
    tokio::spawn(async move {
        let worker_id = uuid::Uuid::new_v4();
        tracing::info!(worker_id = %worker_id, "🔄 Checker worker started");

        let client = reqwest::Client::builder()
            .build()
            .unwrap();

        let mut check_count = 0u64;
        let mut iteration = 0u64;

        loop {
            iteration += 1;
            
            // 1) Atomically claim jobs (multi-worker safe)
            let rows = claim_jobs(&pool, 50).await;

            if rows.is_empty() {
                // Only log idle state every 100 iterations (5 minutes)
                if iteration % 100 == 0 {
                    tracing::info!(
                        worker_id = %worker_id,
                        checks_performed = check_count,
                        "Worker idle - no jobs available"
                    );
                }
                tokio::time::sleep(Duration::from_secs(3)).await;
                continue;
            }

            // Only log batch processing if jobs > 0
            if rows.len() > 1 {
                tracing::info!(
                    worker_id = %worker_id,
                    jobs_claimed = rows.len(),
                    "Processing batch"
                );
            }

            for row in rows {
                let id: Uuid = row.get("id");
                let url: String = row.get("url");
                let interval_seconds: i32 = row.get("interval_seconds");
                let threshold: i32 = row.get("alert_after_failures");
                let alert_email: Option<String> = row.get("alert_email");
                let mut failure_count: i32 = row.get("failure_count");

                // 2) Run HTTP check with timeout
                let start = std::time::Instant::now();
                let res = tokio::time::timeout(
                    Duration::from_secs(10),
                    client.get(&url).send()
                ).await;

                let (is_up, status_code, response_time_ms, error_message) = match res {
                    Ok(Ok(r)) => {
                        let status = r.status().as_u16() as i32;
                        let ms = start.elapsed().as_millis() as i32;
                        let is_success = status >= 200 && status < 300;
                        
                        // Log non-2xx responses
                        if !is_success {
                            tracing::warn!(
                                monitor_id = %id,
                                url = %url,
                                status_code = status,
                                response_time_ms = ms,
                                is_success = is_success,
                                "Check completed with non-2xx status"
                            );
                        }
                        (is_success, Some(status), Some(ms), None)
                    }
                    Ok(Err(e)) => {
                        let ms = start.elapsed().as_millis() as i32;
                        tracing::warn!(
                            monitor_id = %id,
                            url = %url,
                            error = %e,
                            elapsed_ms = ms,
                            "Check failed - HTTP error"
                        );
                        (false, None, None, Some(e.to_string()))
                    }
                    Err(_) => {
                        tracing::warn!(
                            monitor_id = %id,
                            url = %url,
                            "Check failed - Timeout (10s)"
                        );
                        (false, None, None, Some("Request timeout after 10 seconds".to_string()))
                    }
                };

                check_count += 1;

                // 3) Insert check
                let check_result = sqlx::query(
                    r#"
                    INSERT INTO checks (monitor_id, checked_at, status_code, response_time_ms, is_up, error_message)
                    VALUES ($1, now(), $2, $3, $4, $5)
                    "#,
                )
                .bind(id)
                .bind(status_code)
                .bind(response_time_ms)
                .bind(is_up)
                .bind(&error_message)
                .execute(&pool)
                .await;

                if let Err(e) = check_result {
                    tracing::error!(
                        monitor_id = %id,
                        error = %e,
                        "Failed to insert check record"
                    );
                }

                // 4) Incident logic
                let open_incident = sqlx::query(
                    "SELECT id FROM incidents WHERE monitor_id = $1 AND status = 'open' LIMIT 1",
                )
                .bind(id)
                .fetch_optional(&pool)
                .await
                .ok()
                .flatten();

                if !is_up {
                    failure_count += 1;
                    tracing::warn!(
                        monitor_id = %id,
                        url = %url,
                        failure_count = failure_count,
                        threshold = threshold,
                        "Monitor is DOWN - incrementing failure count"
                    );
                } else {
                    // Only log recovery if there were previous failures
                    if failure_count > 0 {
                        tracing::info!(
                            monitor_id = %id,
                            url = %url,
                            previous_failures = failure_count,
                            "✅ Monitor recovered"
                        );
                    }
                    failure_count = 0;
                }

                if failure_count >= threshold && open_incident.is_none() {
                    tracing::warn!(
                        monitor_id = %id,
                        url = %url,
                        failure_count = failure_count,
                        threshold = threshold,
                        "🚨 Opening incident - threshold reached"
                    );

                    let incident_result = sqlx::query(
                        r#"
                        INSERT INTO incidents (monitor_id, started_at, status, reason)
                        VALUES ($1, now(), 'open', 'Consecutive failures exceeded threshold')
                        "#,
                    )
                    .bind(id)
                    .execute(&pool)
                    .await;

                    match incident_result {
                        Ok(_) => {
                            tracing::info!(
                                monitor_id = %id,
                                url = %url,
                                "✅ Incident created successfully"
                            );
                        }
                        Err(e) => {
                            tracing::error!(
                                monitor_id = %id,
                                error = %e,
                                "❌ Failed to create incident"
                            );
                        }
                    }

                    if let Some(email) = alert_email.clone() {
                        tracing::info!(
                            monitor_id = %id,
                            email = %email,
                            "📧 Sending alert email..."
                        );
                        let subject = format!("🚨 DOWN: {}", url);
                        let body = format!("Monitor is DOWN\n\nURL: {}\nTime: {}\nFailure Count: {}\n", url, Utc::now(), failure_count);
                        send_email(&email, &subject, &body).await;
                    } else {
                        tracing::warn!(
                            monitor_id = %id,
                            "No alert email configured - skipping email notification"
                        );
                    }
                }

                if is_up {
                    if let Some(inc) = open_incident {
                        let inc_id: Uuid = inc.get("id");

                        tracing::info!(
                            monitor_id = %id,
                            incident_id = %inc_id,
                            url = %url,
                            "✅ Resolving incident - monitor recovered"
                        );

                        let _ = sqlx::query(
                            "UPDATE incidents SET status = 'resolved', ended_at = now() WHERE id = $1",
                        )
                        .bind(inc_id)
                        .execute(&pool)
                        .await;

                        if let Some(email) = alert_email.clone() {
                            let subject = format!("✅ RECOVERED: {}", url);
                            let body = format!(
                                "Monitor has RECOVERED\n\nURL: {}\nTime: {}\n",
                                url,
                                Utc::now()
                            );
                            send_email(&email, &subject, &body).await;
                        }
                    }
                }

                // 5) Compute next_run_at (normal or backoff)
                let next_run_at_sql = if is_up {
                    format!("now() + interval '{} seconds'", interval_seconds)
                } else {
                    let backoff = compute_backoff_seconds(failure_count) + jitter_seconds();
                    format!("now() + interval '{} seconds'", backoff)
                };

                // 6) Update scheduler state + release lease
                let update_result = sqlx::query(&format!(
                    r#"
                    UPDATE monitors
                    SET
                      next_run_at = {},
                      lease_until = NULL,
                      failure_count = $2
                    WHERE id = $1
                    "#,
                    next_run_at_sql
                ))
                .bind(id)
                .bind(failure_count)
                .execute(&pool)
                .await;

                if let Err(e) = update_result {
                    tracing::error!(
                        monitor_id = %id,
                        error = %e,
                        failure_count = failure_count,
                        "Failed to update monitor state"
                    );
                }
                // Don't log successful updates - too verbose
            }

            // Log metrics every 10 iterations
            if iteration % 10 == 0 {
                tracing::info!(
                    worker_id = %worker_id,
                    total_checks = check_count,
                    iteration = iteration,
                    "Worker metrics"
                );
            }
        }
    });
}
