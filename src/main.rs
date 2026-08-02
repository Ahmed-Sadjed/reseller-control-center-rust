use actix_web::HttpServer;
use reseller_control_center_rust::{app::build_app, config::Settings, db, queue};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let settings = Settings::from_env();

    tracing_subscriber::fmt()
        .with_env_filter(&settings.rust_log)
        .init();

    let pool = db::create_pool(&settings.database_url)
        .await
        .expect("failed to create database pool");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("failed to run database migrations");

    let redis_client = redis::Client::open(settings.redis_url.clone())
        .expect("failed to create redis client");
    let redis_conn = redis_client
        .get_multiplexed_async_connection()
        .await
        .expect("failed to connect to redis");

    // Redis Stream fulfillment worker (Phase 6).
    {
        let pool = pool.clone();
        let settings = settings.clone();
        let client = redis_client.clone();
        tokio::spawn(async move {
            queue::run_worker(pool, settings, client).await;
        });
    }

    let bind_addr = format!("0.0.0.0:8080");
    let workers = settings.workers;
    tracing::info!(bind_addr = %bind_addr, workers = workers, "starting actix server");

    HttpServer::new(move || build_app(pool.clone(), settings.clone(), redis_conn.clone()))
        .workers(workers)
        .keep_alive(std::time::Duration::from_secs(60))
        .bind(&bind_addr)?
        .run()
        .await
}
