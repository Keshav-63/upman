mod auth;
mod config;
mod error;
mod handlers;
mod models;
mod services;

use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tower_http::cors::{CorsLayer, Any};
use tracing::Level;

use config::Config;
use handlers::{analytics, monitor};
use services::checker::start_checker;

#[tokio::main]
async fn main() {
    // Initialize structured logging with file rotation (also outputs to console)
    let file_appender = tracing_appender::rolling::daily("./logs", "upman.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    
    // Log to both console AND file
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stdout))
        .with(tracing_subscriber::fmt::layer().with_writer(non_blocking).with_ansi(false))
        .init();

    tracing::info!("🚀 Starting UpMan Backend v3");

    // Load configuration
    let config = Config::from_env();
    tracing::info!("🔧 Configuration loaded");
    tracing::info!("📊 Database: {}", config.database_url);
    tracing::info!("🌐 Port: {}", config.port);

    // Create database pool
    let pool = config.create_pool().await;
    tracing::info!("✅ Connected to database (pool size: {})", config.max_db_connections);

    // Start background checker workers (multi-worker safe)
    start_checker(pool.clone()).await;
    start_checker(pool.clone()).await;
    tracing::info!("🔄 Started 2 checker workers");

    // CORS configuration for production
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Public routes (no auth)
    let public_routes = Router::new()
        .route("/health", get(monitor::health));

    // Protected routes (require auth)
    let protected_routes = Router::new()
        // Dashboard
        .route("/dashboard", get(analytics::get_dashboard_stats))
        
        // Monitors CRUD
        .route("/monitors", post(monitor::create_monitor).get(monitor::list_monitors))
        .route("/monitors/:id", 
            get(monitor::get_monitor)
            .put(monitor::update_monitor)
            .delete(monitor::delete_monitor)
        )
        
        // Monitor data
        .route("/monitors/:id/checks", get(monitor::get_recent_checks))
        .route("/monitors/:id/incidents", get(monitor::list_incidents))
        
        // Analytics
        .route("/monitors/:id/stats", get(analytics::get_monitor_stats))
        .route("/monitors/:id/uptime", get(analytics::get_uptime))
        .route("/monitors/:id/latency", get(analytics::get_latency))
        .route("/monitors/:id/mttr", get(analytics::get_mttr))
        .route("/monitors/:id/availability", get(analytics::get_availability))
        
        .layer(middleware::from_fn(auth::auth_middleware));

    // Combine routes
    let app = Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO))
        )
        .layer(cors)
        .with_state(pool);

    // Start server
    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    tracing::info!("🌐 Server running on http://{}", addr);
    tracing::info!("📡 Endpoints:");
    tracing::info!("   GET  /health (public)");
    tracing::info!("   GET  /dashboard (protected)");
    tracing::info!("   POST /monitors (protected)");
    tracing::info!("   GET  /monitors (protected)");
    tracing::info!("   ... and more (see docs)");

    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}