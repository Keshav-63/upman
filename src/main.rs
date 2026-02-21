use axum::{
    routing::{get, post},
    Router, Json,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing_subscriber;
use sqlx::{PgPool, postgres::PgPoolOptions};
use dotenvy::dotenv;
use std::env;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL not set");
    println!("Connecting to DB at {}", database_url);

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("Failed to connect to DB");

    let app = Router::new()
        .route("/health", get(health))
        .route("/monitors", post(create_monitor))
        .with_state(pool);

    let addr = SocketAddr::from(([0, 0, 0, 0], 8000));
    println!("Server running on {}", addr);

    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health() -> &'static str {
    "ok"
}

#[derive(Deserialize)]
struct CreateMonitorReq {
    url: String,
    interval_seconds: i32,
}

#[derive(Serialize)]
struct CreateMonitorRes {
    success: bool,
}

async fn create_monitor(
    axum::extract::State(pool): axum::extract::State<PgPool>,
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