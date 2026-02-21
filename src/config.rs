use std::env;
use sqlx::{postgres::PgPoolOptions, PgPool};
use dotenvy::dotenv;

/// Application configuration
pub struct Config {
    pub database_url: String,
    pub port: u16,
    pub max_db_connections: u32,
}

impl Config {
    /// Load configuration from environment variables
    pub fn from_env() -> Self {
        dotenv().ok();

        Self {
            database_url: env::var("DATABASE_URL").expect("DATABASE_URL must be set"),
            port: env::var("PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(8000),
            max_db_connections: env::var("MAX_DB_CONNECTIONS")
                .ok()
                .and_then(|c| c.parse().ok())
                .unwrap_or(10),
        }
    }

    /// Create a database connection pool
    pub async fn create_pool(&self) -> PgPool {
        PgPoolOptions::new()
            .max_connections(self.max_db_connections)
            .connect(&self.database_url)
            .await
            .expect("Failed to connect to database")
    }
}
