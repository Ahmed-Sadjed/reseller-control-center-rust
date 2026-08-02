use redis::AsyncCommands;
use sqlx::PgPool;
use std::time::Duration;
use uuid::Uuid;

use crate::{config::Settings, services::orders::fulfill_order};

pub const STREAM_KEY: &str = "orders:fulfill";
pub const GROUP_NAME: &str = "workers";

/// Push an order uuid onto the fulfillment stream (replaces tokio::spawn).
pub async fn enqueue_order(
    conn: &redis::aio::MultiplexedConnection,
    stream_key: &str,
    order_id: Uuid,
) -> redis::RedisResult<String> {
    let mut conn = conn.clone();
    conn.xadd::<_, _, _, _, String>(stream_key, "*", &[("order_id", order_id.to_string())])
        .await
}

async fn ensure_group(conn: &mut redis::aio::MultiplexedConnection, stream_key: &str) {
    // MKSTREAM creates the stream if missing; BUSYGROUP errors are ignored.
    let _: redis::RedisResult<()> = conn
        .xgroup_create_mkstream::<_, _, _, ()>(stream_key, GROUP_NAME, "$")
        .await;
}

/// Read up to 1 message, fulfill it, XACK it. Returns true if work was done.
async fn worker_once(
    pool: &PgPool,
    settings: &Settings,
    client: &redis::Client,
) -> Result<bool, anyhow::Error> {
    let mut conn = client.get_multiplexed_async_connection().await?;
    ensure_group(&mut conn, &settings.redis_stream_key).await;

    let consumer = format!("worker-{}", std::process::id());
    let opts = redis::streams::StreamReadOptions::default()
        .group(GROUP_NAME, &consumer)
        .count(1)
        .block(1000);
    let reply: redis::streams::StreamReadReply =
        conn.xread_options(&[&settings.redis_stream_key], &[">"], &opts).await?;

    let mut done = false;
    for stream in &reply.keys {
        for entry in &stream.ids {
            done = true;
            let order_id = entry
                .get::<String>("order_id")
                .and_then(|s| s.parse::<Uuid>().ok());

            if let Some(order_id) = order_id {
                match fulfill_order(pool, settings, order_id).await {
                    Ok(()) => tracing::info!(order_id = %order_id, "worker fulfilled order"),
                    Err(e) => tracing::error!(order_id = %order_id, error = %e, "worker fulfillment failed"),
                }
            }

            // Ack regardless: failures are persisted on the order itself.
            let _: redis::RedisResult<usize> =
                conn.xack(&settings.redis_stream_key, GROUP_NAME, &[&entry.id]).await;
        }
    }

    Ok(done)
}

/// Long-running worker: XREAD BLOCK 1000 in a loop (spawned from main).
pub async fn run_worker(pool: PgPool, settings: Settings, client: redis::Client) {
    tracing::info!("order fulfillment worker started");
    loop {
        match worker_once(&pool, &settings, &client).await {
            Ok(_) => tokio::time::sleep(Duration::from_millis(100)).await,
            Err(e) => {
                tracing::error!(error = %e, "worker error, retrying in 1s");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}
