use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct UptimeRes {
    pub uptime: f64,
    pub total_checks: i64,
    pub successful_checks: i64,
}

#[derive(Serialize)]
pub struct LatencyRes {
    pub p50: Option<f64>,
    pub p95: Option<f64>,
    pub p99: Option<f64>,
    pub avg: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[derive(Serialize)]
pub struct MttrRes {
    pub mttr_seconds: Option<f64>,
    pub total_incidents: i64,
    pub resolved_incidents: i64,
}

#[derive(Serialize)]
pub struct AvailabilityPoint {
    pub day: chrono::DateTime<chrono::Utc>,
    pub uptime: f64,
    pub total_checks: i64,
}

#[derive(Serialize)]
pub struct DashboardStats {
    pub total_monitors: i64,
    pub active_monitors: i64,
    pub paused_monitors: i64,
    pub monitors_up: i64,
    pub monitors_down: i64,
    pub open_incidents: i64,
    pub avg_uptime_24h: f64,
    pub total_checks_24h: i64,
}

#[derive(Serialize)]
pub struct MonitorStats {
    pub uptime_24h: f64,
    pub uptime_7d: f64,
    pub uptime_30d: f64,
    pub avg_response_time_24h: Option<f64>,
    pub total_checks: i64,
    pub total_incidents: i64,
    pub current_streak_hours: i64,
}

#[derive(Deserialize)]
pub struct TimeRangeParams {
    #[serde(default = "default_days")]
    pub days: i32,
}

fn default_days() -> i32 {
    30
}
