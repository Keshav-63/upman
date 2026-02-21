use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct CreateMonitorReq {
    pub url: String,
    pub interval_seconds: i32,
    pub alert_email: Option<String>,
    pub alert_after_failures: Option<i32>,
}

#[derive(Serialize)]
pub struct CreateMonitorRes {
    pub success: bool,
    pub monitor_id: Uuid,
}

#[derive(Serialize)]
pub struct MonitorDto {
    pub id: Uuid,
    pub url: String,
    pub interval_seconds: i32,
    pub alert_email: Option<String>,
    pub alert_after_failures: i32,
    pub is_paused: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_status: Option<String>,
    pub uptime_24h: Option<f64>,
}

#[derive(Serialize)]
pub struct MonitorDetailDto {
    pub id: Uuid,
    pub url: String,
    pub interval_seconds: i32,
    pub alert_email: Option<String>,
    pub alert_after_failures: i32,
    pub is_paused: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub next_run_at: Option<chrono::DateTime<chrono::Utc>>,
    pub failure_count: i32,
    pub total_checks: i64,
    pub uptime_7d: Option<f64>,
    pub uptime_30d: Option<f64>,
    pub avg_response_time: Option<f64>,
}

#[derive(Serialize)]
pub struct IncidentDto {
    pub id: Uuid,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub ended_at: Option<chrono::DateTime<chrono::Utc>>,
    pub status: String,
    pub reason: Option<String>,
    pub duration_seconds: Option<i64>,
}

#[derive(Deserialize)]
pub struct PaginationParams {
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_per_page")]
    pub per_page: i64,
}

fn default_page() -> i64 {
    1
}

fn default_per_page() -> i64 {
    20
}

impl PaginationParams {
    pub fn offset(&self) -> i64 {
        (self.page - 1) * self.per_page
    }

    pub fn limit(&self) -> i64 {
        self.per_page.min(100) // Cap at 100
    }
}

#[derive(Serialize)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub pagination: PaginationMeta,
}

#[derive(Serialize)]
pub struct PaginationMeta {
    pub page: i64,
    pub per_page: i64,
    pub total: i64,
    pub total_pages: i64,
}

#[derive(Deserialize)]
pub struct UpdateMonitorReq {
    pub url: Option<String>,
    pub interval_seconds: Option<i32>,
    pub alert_email: Option<String>,
    pub alert_after_failures: Option<i32>,
    pub is_paused: Option<bool>,
}

#[derive(Serialize)]
pub struct CheckDto {
    pub id: Uuid,
    pub checked_at: chrono::DateTime<chrono::Utc>,
    pub status_code: Option<i32>,
    pub response_time_ms: Option<i32>,
    pub is_up: bool,
    pub error_message: Option<String>,
}
