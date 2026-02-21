use axum::{extract::State, Extension, Json};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::{AppError, Result};
use crate::models::{
    CheckDto, CreateMonitorReq, CreateMonitorRes, IncidentDto, MonitorDetailDto, MonitorDto,
    PaginatedResponse, PaginationMeta, PaginationParams, UpdateMonitorReq,
};

pub async fn health() -> &'static str {
    "ok"
}

/// Create a new monitor (user-scoped)
pub async fn create_monitor(
    State(pool): State<PgPool>,
    Extension(user): Extension<AuthUser>,
    Json(payload): Json<CreateMonitorReq>,
) -> Result<Json<CreateMonitorRes>> {
    tracing::info!(
        user_id = %user.user_id,
        url = %payload.url,
        "Creating new monitor"
    );

    // Validate URL
    if !payload.url.starts_with("http://") && !payload.url.starts_with("https://") {
        return Err(AppError::Validation("URL must start with http:// or https://".to_string()));
    }

    // Validate interval
    if payload.interval_seconds < 30 {
        return Err(AppError::Validation("Interval must be at least 30 seconds".to_string()));
    }

    let monitor_id = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO monitors (
            user_id, url, interval_seconds, alert_email, alert_after_failures,
            next_run_at, failure_count, lease_until
        )
        VALUES ($1, $2, $3, $4, $5, now(), 0, NULL)
        RETURNING id
        "#,
    )
    .bind(&user.user_id)
    .bind(&payload.url)
    .bind(payload.interval_seconds)
    .bind(&payload.alert_email)
    .bind(payload.alert_after_failures.unwrap_or(1))
    .fetch_one(&pool)
    .await?;

    tracing::info!(
        monitor_id = %monitor_id,
        user_id = %user.user_id,
        "Monitor created successfully"
    );

    Ok(Json(CreateMonitorRes {
        success: true,
        monitor_id,
    }))
}

/// List user's monitors with pagination
pub async fn list_monitors(
    State(pool): State<PgPool>,
    Extension(user): Extension<AuthUser>,
    axum::extract::Query(params): axum::extract::Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<MonitorDto>>> {
    // Get total count
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM monitors WHERE user_id = $1"
    )
    .bind(&user.user_id)
    .fetch_one(&pool)
    .await?;

    // Fetch monitors with 24h uptime
    let rows = sqlx::query(
        r#"
        SELECT 
            m.id, 
            m.url, 
            m.interval_seconds, 
            m.alert_email,
            m.alert_after_failures,
            m.is_paused,
            m.created_at,
            (
                SELECT is_up 
                FROM checks 
                WHERE monitor_id = m.id 
                ORDER BY checked_at DESC 
                LIMIT 1
            ) as last_status,
            (
                SELECT 
                    (SUM(CASE WHEN is_up THEN 1 ELSE 0 END)::float / NULLIF(COUNT(*), 0)) * 100 
                FROM checks 
                WHERE monitor_id = m.id 
                AND checked_at > now() - interval '24 hours'
            ) as uptime_24h
        FROM monitors m
        WHERE m.user_id = $1
        ORDER BY m.created_at DESC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(&user.user_id)
    .bind(params.limit())
    .bind(params.offset())
    .fetch_all(&pool)
    .await?;

    let monitors = rows
        .iter()
        .map(|row| MonitorDto {
            id: row.get("id"),
            url: row.get("url"),
            interval_seconds: row.get("interval_seconds"),
            alert_email: row.get("alert_email"),
            alert_after_failures: row.get("alert_after_failures"),
            is_paused: row.get("is_paused"),
            created_at: row.get("created_at"),
            last_status: row.get::<Option<bool>, _>("last_status").map(|up| {
                if up { "up".to_string() } else { "down".to_string() }
            }),
            uptime_24h: row.get("uptime_24h"),
        })
        .collect();

    Ok(Json(PaginatedResponse {
        data: monitors,
        pagination: PaginationMeta {
            page: params.page,
            per_page: params.limit(),
            total,
            total_pages: (total + params.limit() - 1) / params.limit(),
        },
    }))
}

/// Get single monitor details
pub async fn get_monitor(
    State(pool): State<PgPool>,
    Extension(user): Extension<AuthUser>,
    axum::extract::Path(monitor_id): axum::extract::Path<Uuid>,
) -> Result<Json<MonitorDetailDto>> {
    let row = sqlx::query(
        r#"
        SELECT 
            m.*,
            (SELECT COUNT(*) FROM checks WHERE monitor_id = m.id) as total_checks,
            (
                SELECT 
                    (SUM(CASE WHEN is_up THEN 1 ELSE 0 END)::float / NULLIF(COUNT(*), 0)) * 100 
                FROM checks 
                WHERE monitor_id = m.id 
                AND checked_at > now() - interval '7 days'
            ) as uptime_7d,
            (
                SELECT 
                    (SUM(CASE WHEN is_up THEN 1 ELSE 0 END)::float / NULLIF(COUNT(*), 0)) * 100 
                FROM checks 
                WHERE monitor_id = m.id 
                AND checked_at > now() - interval '30 days'
            ) as uptime_30d,
            (
                SELECT AVG(response_time_ms) 
                FROM checks 
                WHERE monitor_id = m.id 
                AND response_time_ms IS NOT NULL
                AND checked_at > now() - interval '24 hours'
            ) as avg_response_time
        FROM monitors m
        WHERE m.id = $1 AND m.user_id = $2
        "#,
    )
    .bind(monitor_id)
    .bind(&user.user_id)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Monitor not found".to_string()))?;

    Ok(Json(MonitorDetailDto {
        id: row.get("id"),
        url: row.get("url"),
        interval_seconds: row.get("interval_seconds"),
        alert_email: row.get("alert_email"),
        alert_after_failures: row.get("alert_after_failures"),
        is_paused: row.get("is_paused"),
        created_at: row.get("created_at"),
        next_run_at: row.get("next_run_at"),
        failure_count: row.get("failure_count"),
        total_checks: row.get("total_checks"),
        uptime_7d: row.get("uptime_7d"),
        uptime_30d: row.get("uptime_30d"),
        avg_response_time: row.get("avg_response_time"),
    }))
}

/// Update monitor
pub async fn update_monitor(
    State(pool): State<PgPool>,
    Extension(user): Extension<AuthUser>,
    axum::extract::Path(monitor_id): axum::extract::Path<Uuid>,
    Json(payload): Json<UpdateMonitorReq>,
) -> Result<Json<serde_json::Value>> {
    tracing::info!(
        monitor_id = %monitor_id,
        user_id = %user.user_id,
        "Updating monitor"
    );

    // Build dynamic update query
    let mut updates = Vec::new();
    let mut bind_index = 3;

    if payload.url.is_some() {
        updates.push(format!("url = ${}", bind_index));
        bind_index += 1;
    }
    if payload.interval_seconds.is_some() {
        updates.push(format!("interval_seconds = ${}", bind_index));
        bind_index += 1;
    }
    if payload.alert_email.is_some() {
        updates.push(format!("alert_email = ${}", bind_index));
        bind_index += 1;
    }
    if payload.alert_after_failures.is_some() {
        updates.push(format!("alert_after_failures = ${}", bind_index));
        bind_index += 1;
    }
    if payload.is_paused.is_some() {
        updates.push(format!("is_paused = ${}", bind_index));
    }

    if updates.is_empty() {
        return Err(AppError::Validation("No fields to update".to_string()));
    }

    let query_str = format!(
        "UPDATE monitors SET {} WHERE id = $1 AND user_id = $2 RETURNING id",
        updates.join(", ")
    );

    let mut query = sqlx::query_scalar::<_, Uuid>(&query_str)
        .bind(monitor_id)
        .bind(&user.user_id);

    if let Some(url) = payload.url {
        query = query.bind(url);
    }
    if let Some(interval) = payload.interval_seconds {
        query = query.bind(interval);
    }
    if let Some(email) = payload.alert_email {
        query = query.bind(email);
    }
    if let Some(failures) = payload.alert_after_failures {
        query = query.bind(failures);
    }
    if let Some(paused) = payload.is_paused {
        query = query.bind(paused);
    }

    query
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Monitor not found".to_string()))?;

    Ok(Json(serde_json::json!({ "success": true })))
}

/// Delete monitor
pub async fn delete_monitor(
    State(pool): State<PgPool>,
    Extension(user): Extension<AuthUser>,
    axum::extract::Path(monitor_id): axum::extract::Path<Uuid>,
) -> Result<Json<serde_json::Value>> {
    tracing::info!(
        monitor_id = %monitor_id,
        user_id = %user.user_id,
        "Deleting monitor"
    );

    let result = sqlx::query("DELETE FROM monitors WHERE id = $1 AND user_id = $2 RETURNING id")
        .bind(monitor_id)
        .bind(&user.user_id)
        .fetch_optional(&pool)
        .await?;

    if result.is_none() {
        return Err(AppError::NotFound("Monitor not found".to_string()));
    }

    Ok(Json(serde_json::json!({ "success": true })))
}

/// List incidents for a monitor
pub async fn list_incidents(
    State(pool): State<PgPool>,
    Extension(user): Extension<AuthUser>,
    axum::extract::Path(monitor_id): axum::extract::Path<Uuid>,
    axum::extract::Query(params): axum::extract::Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<IncidentDto>>> {
    // Verify ownership
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM monitors WHERE id = $1 AND user_id = $2)"
    )
    .bind(monitor_id)
    .bind(&user.user_id)
    .fetch_one(&pool)
    .await?;

    if !exists {
        return Err(AppError::NotFound("Monitor not found".to_string()));
    }

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM incidents WHERE monitor_id = $1"
    )
    .bind(monitor_id)
    .fetch_one(&pool)
    .await?;

    let rows = sqlx::query(
        r#"
        SELECT 
            id, 
            started_at, 
            ended_at, 
            status, 
            reason,
            EXTRACT(EPOCH FROM (ended_at - started_at)) as duration_seconds
        FROM incidents
        WHERE monitor_id = $1
        ORDER BY started_at DESC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(monitor_id)
    .bind(params.limit())
    .bind(params.offset())
    .fetch_all(&pool)
    .await?;

    let incidents = rows
        .iter()
        .map(|r| IncidentDto {
            id: r.get("id"),
            started_at: r.get("started_at"),
            ended_at: r.get("ended_at"),
            status: r.get("status"),
            reason: r.get("reason"),
            duration_seconds: r.get::<Option<f64>, _>("duration_seconds").map(|d| d as i64),
        })
        .collect();

    Ok(Json(PaginatedResponse {
        data: incidents,
        pagination: PaginationMeta {
            page: params.page,
            per_page: params.limit(),
            total,
            total_pages: (total + params.limit() - 1) / params.limit(),
        },
    }))
}

/// Get recent checks for a monitor
pub async fn get_recent_checks(
    State(pool): State<PgPool>,
    Extension(user): Extension<AuthUser>,
    axum::extract::Path(monitor_id): axum::extract::Path<Uuid>,
    axum::extract::Query(params): axum::extract::Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<CheckDto>>> {
    // Verify ownership
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM monitors WHERE id = $1 AND user_id = $2)"
    )
    .bind(monitor_id)
    .bind(&user.user_id)
    .fetch_one(&pool)
    .await?;

    if !exists {
        return Err(AppError::NotFound("Monitor not found".to_string()));
    }

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM checks WHERE monitor_id = $1"
    )
    .bind(monitor_id)
    .fetch_one(&pool)
    .await?;

    let rows = sqlx::query(
        r#"
        SELECT id, checked_at, status_code, response_time_ms, is_up, error_message
        FROM checks
        WHERE monitor_id = $1
        ORDER BY checked_at DESC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(monitor_id)
    .bind(params.limit())
    .bind(params.offset())
    .fetch_all(&pool)
    .await?;

    let checks = rows
        .iter()
        .map(|r| CheckDto {
            id: r.get("id"),
            checked_at: r.get("checked_at"),
            status_code: r.get("status_code"),
            response_time_ms: r.get("response_time_ms"),
            is_up: r.get("is_up"),
            error_message: r.get("error_message"),
        })
        .collect();

    Ok(Json(PaginatedResponse {
        data: checks,
        pagination: PaginationMeta {
            page: params.page,
            per_page: params.limit(),
            total,
            total_pages: (total + params.limit() - 1) / params.limit(),
        },
    }))
}
