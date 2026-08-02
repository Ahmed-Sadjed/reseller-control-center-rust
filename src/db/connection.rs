use std::time::Duration;

use sqlx::postgres::{PgPool, PgPoolOptions};

pub async fn create_pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(50)
        .idle_timeout(Duration::from_secs(600))
        .acquire_timeout(Duration::from_secs(15))
        .connect(database_url)
        .await
}
