use axum::{extract::{Path, Query, State}, Extension, Json};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::{AppError, Result};
use crate::models::{
    AvailabilityPoint, DashboardStats, LatencyRes, MonitorStats, MttrRes, TimeRangeParams,
    UptimeRes,
};

/// Get dashboard statistics for user
pub async fn get_dashboard_stats(
    State(pool): State<PgPool>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<DashboardStats>> {
    tracing::debug!(user_id = %user.user_id, "Fetching dashboard stats");

    let row = sqlx::query(
        r#"
        WITH monitor_counts AS (
            SELECT 
                COUNT(*) as total_monitors,
                SUM(CASE WHEN is_paused = FALSE THEN 1 ELSE 0 END) as active_monitors,
                SUM(CASE WHEN is_paused = TRUE THEN 1 ELSE 0 END) as paused_monitors
            FROM monitors
            WHERE user_id = $1
        ),
        status_counts AS (
            SELECT 
                COUNT(DISTINCT m.id) FILTER (WHERE c.is_up = TRUE) as monitors_up,
                COUNT(DISTINCT m.id) FILTER (WHERE c.is_up = FALSE) as monitors_down
            FROM monitors m
            LEFT JOIN LATERAL (
                SELECT is_up 
                FROM checks 
                WHERE monitor_id = m.id 
                ORDER BY checked_at DESC 
                LIMIT 1
            ) c ON TRUE
            WHERE m.user_id = $1 AND m.is_paused = FALSE
        ),
        incident_count AS (
            SELECT COUNT(*) as open_incidents
            FROM incidents i
            JOIN monitors m ON i.monitor_id = m.id
            WHERE m.user_id = $1 AND i.status = 'open'
        ),
        uptime_stats AS (
            SELECT 
                AVG(uptime_percent) as avg_uptime,
                SUM(check_count)::BIGINT as total_checks
            FROM (
                SELECT 
                    m.id,
                    (SUM(CASE WHEN is_up THEN 1 ELSE 0 END)::float / NULLIF(COUNT(*), 0)) * 100 as uptime_percent,
                    COUNT(*) as check_count
                FROM checks c
                JOIN monitors m ON c.monitor_id = m.id
                WHERE m.user_id = $1 
                AND c.checked_at > now() - interval '24 hours'
                GROUP BY m.id
            ) monitor_uptimes
        )
        SELECT 
            COALESCE(mc.total_monitors, 0) as total_monitors,
            COALESCE(mc.active_monitors, 0) as active_monitors,
            COALESCE(mc.paused_monitors, 0) as paused_monitors,
            COALESCE(sc.monitors_up, 0) as monitors_up,
            COALESCE(sc.monitors_down, 0) as monitors_down,
            COALESCE(ic.open_incidents, 0) as open_incidents,
            COALESCE(us.avg_uptime, 100.0) as avg_uptime_24h,
            COALESCE(us.total_checks, 0) as total_checks_24h
        FROM monitor_counts mc
        CROSS JOIN status_counts sc
        CROSS JOIN incident_count ic
        CROSS JOIN uptime_stats us
        "#,
    )
    .bind(user.user_id)
    .fetch_one(&pool)
    .await?;

    Ok(Json(DashboardStats {
        total_monitors: row.get("total_monitors"),
        active_monitors: row.get("active_monitors"),
        paused_monitors: row.get("paused_monitors"),
        monitors_up: row.get("monitors_up"),
        monitors_down: row.get("monitors_down"),
        open_incidents: row.get("open_incidents"),
        avg_uptime_24h: row.get("avg_uptime_24h"),
        total_checks_24h: row.get("total_checks_24h"),
    }))
}

/// Get comprehensive stats for a specific monitor
pub async fn get_monitor_stats(
    State(pool): State<PgPool>,
    Extension(user): Extension<AuthUser>,
    Path(monitor_id): Path<Uuid>,
) -> Result<Json<MonitorStats>> {
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

    let row = sqlx::query(
        r#"
        WITH uptime_24h AS (
            SELECT 
                (SUM(CASE WHEN is_up THEN 1 ELSE 0 END)::float / NULLIF(COUNT(*), 0)) * 100 as uptime
            FROM checks
            WHERE monitor_id = $1 AND checked_at > now() - interval '24 hours'
        ),
        uptime_7d AS (
            SELECT 
                (SUM(CASE WHEN is_up THEN 1 ELSE 0 END)::float / NULLIF(COUNT(*), 0)) * 100 as uptime
            FROM checks
            WHERE monitor_id = $1 AND checked_at > now() - interval '7 days'
        ),
        uptime_30d AS (
            SELECT 
                (SUM(CASE WHEN is_up THEN 1 ELSE 0 END)::float / NULLIF(COUNT(*), 0)) * 100 as uptime
            FROM checks
            WHERE monitor_id = $1 AND checked_at > now() - interval '30 days'
        ),
        response_time AS (
            SELECT AVG(response_time_ms)::float8 as avg_response
            FROM checks
            WHERE monitor_id = $1 
            AND checked_at > now() - interval '24 hours'
            AND response_time_ms IS NOT NULL
        ),
        streak AS (
            SELECT 
                EXTRACT(EPOCH FROM (now() - MIN(checked_at))) / 3600 as hours
            FROM (
                SELECT checked_at, is_up,
                    SUM(CASE WHEN is_up = FALSE THEN 1 ELSE 0 END) 
                        OVER (ORDER BY checked_at DESC) as fail_group
                FROM checks
                WHERE monitor_id = $1
                ORDER BY checked_at DESC
            ) sub
            WHERE fail_group = 0
        )
        SELECT 
            COALESCE(u24.uptime, 0) as uptime_24h,
            COALESCE(u7.uptime, 0) as uptime_7d,
            COALESCE(u30.uptime, 0) as uptime_30d,
            rt.avg_response as avg_response_time_24h,
            (SELECT COUNT(*) FROM checks WHERE monitor_id = $1) as total_checks,
            (SELECT COUNT(*) FROM incidents WHERE monitor_id = $1) as total_incidents,
            COALESCE(s.hours, 0)::bigint as current_streak_hours
        FROM uptime_24h u24
        CROSS JOIN uptime_7d u7
        CROSS JOIN uptime_30d u30
        CROSS JOIN response_time rt
        CROSS JOIN streak s
        "#,
    )
    .bind(monitor_id)
    .fetch_one(&pool)
    .await?;

    Ok(Json(MonitorStats {
        uptime_24h: row.get("uptime_24h"),
        uptime_7d: row.get("uptime_7d"),
        uptime_30d: row.get("uptime_30d"),
        avg_response_time_24h: row.get("avg_response_time_24h"),
        total_checks: row.get("total_checks"),
        total_incidents: row.get("total_incidents"),
        current_streak_hours: row.get("current_streak_hours"),
    }))
}

/// Get uptime percentage for a monitor
pub async fn get_uptime(
    State(pool): State<PgPool>,
    Extension(user): Extension<AuthUser>,
    Path(monitor_id): Path<Uuid>,
    Query(params): Query<TimeRangeParams>,
) -> Result<Json<UptimeRes>> {
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

    let row = sqlx::query(
        r#"
        SELECT
          (SUM(CASE WHEN is_up THEN 1 ELSE 0 END)::float / NULLIF(COUNT(*), 0)) * 100 AS uptime,
          COUNT(*) as total_checks,
          SUM(CASE WHEN is_up THEN 1 ELSE 0 END) as successful_checks
        FROM checks
        WHERE monitor_id = $1
          AND checked_at > now() - ($2 || ' days')::interval
        "#,
    )
    .bind(monitor_id)
    .bind(params.days)
    .fetch_one(&pool)
    .await?;

    Ok(Json(UptimeRes {
        uptime: row.get::<Option<f64>, _>("uptime").unwrap_or(0.0),
        total_checks: row.get("total_checks"),
        successful_checks: row.get("successful_checks"),
    }))
}

/// Get latency percentiles
pub async fn get_latency(
    State(pool): State<PgPool>,
    Extension(user): Extension<AuthUser>,
    Path(monitor_id): Path<Uuid>,
    Query(params): Query<TimeRangeParams>,
) -> Result<Json<LatencyRes>> {
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

    let row = sqlx::query(
        r#"
        SELECT
          percentile_cont(0.50) WITHIN GROUP (ORDER BY response_time_ms) AS p50,
          percentile_cont(0.95) WITHIN GROUP (ORDER BY response_time_ms) AS p95,
          percentile_cont(0.99) WITHIN GROUP (ORDER BY response_time_ms) AS p99,
          AVG(response_time_ms)::float8 as avg,
          MIN(response_time_ms)::float8 as min,
          MAX(response_time_ms)::float8 as max
        FROM checks
        WHERE monitor_id = $1
          AND checked_at > now() - ($2 || ' days')::interval
          AND response_time_ms IS NOT NULL
        "#,
    )
    .bind(monitor_id)
    .bind(params.days)
    .fetch_one(&pool)
    .await?;

    Ok(Json(LatencyRes {
        p50: row.get("p50"),
        p95: row.get("p95"),
        p99: row.get("p99"),
        avg: row.get("avg"),
        min: row.get("min"),
        max: row.get("max"),
    }))
}

/// Get MTTR (Mean Time To Recovery)
pub async fn get_mttr(
    State(pool): State<PgPool>,
    Extension(user): Extension<AuthUser>,
    Path(monitor_id): Path<Uuid>,
) -> Result<Json<MttrRes>> {
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

    let row = sqlx::query(
        r#"
        SELECT 
            AVG(EXTRACT(EPOCH FROM (ended_at - started_at))) AS mttr_seconds,
            COUNT(*) as total_incidents,
            COUNT(*) FILTER (WHERE ended_at IS NOT NULL) as resolved_incidents
        FROM incidents
        WHERE monitor_id = $1
        "#,
    )
    .bind(monitor_id)
    .fetch_one(&pool)
    .await?;

    Ok(Json(MttrRes {
        mttr_seconds: row.get("mttr_seconds"),
        total_incidents: row.get("total_incidents"),
        resolved_incidents: row.get("resolved_incidents"),
    }))
}

/// Get daily availability chart data
pub async fn get_availability(
    State(pool): State<PgPool>,
    Extension(user): Extension<AuthUser>,
    Path(monitor_id): Path<Uuid>,
    Query(params): Query<TimeRangeParams>,
) -> Result<Json<Vec<AvailabilityPoint>>> {
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

    let rows = sqlx::query(
        r#"
        SELECT
          date_trunc('day', checked_at) AS day,
          (SUM(CASE WHEN is_up THEN 1 ELSE 0 END)::float / COUNT(*)) * 100 AS uptime,
          COUNT(*) as total_checks
        FROM checks
        WHERE monitor_id = $1
          AND checked_at > now() - ($2 || ' days')::interval
        GROUP BY day
        ORDER BY day
        "#,
    )
    .bind(monitor_id)
    .bind(params.days)
    .fetch_all(&pool)
    .await?;

    let points = rows
        .iter()
        .map(|r| AvailabilityPoint {
            day: r.get("day"),
            uptime: r.get("uptime"),
            total_checks: r.get("total_checks"),
        })
        .collect();

    Ok(Json(points))
}
