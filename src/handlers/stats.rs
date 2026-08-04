use actix_web::{web, HttpResponse};
use redis::AsyncCommands;
use sqlx::PgPool;

use crate::{error::ApiError, middleware::AuthUser};

const STATS_CACHE_TTL: u64 = 600;

pub async fn stats(
    pool: web::Data<PgPool>,
    redis_conn: web::Data<redis::aio::MultiplexedConnection>,
    user: AuthUser,
) -> Result<HttpResponse, ApiError> {
    let cache_key = "stats:global";
    let mut conn = redis_conn.get_ref().clone();

    // Global counts cached 600s (mirrors Django) + live credit_balance injected.
    let cached: Option<String> = conn.get(cache_key).await?;
    let mut data = match cached {
        Some(body) => serde_json::from_str::<serde_json::Value>(&body)
            .unwrap_or_else(|_| serde_json::json!({})),
        None => {
            let (total_products, total_categories): (i64, i64) = sqlx::query_as(
                "SELECT (SELECT count(*) FROM products WHERE is_active = true), \
                        (SELECT count(*) FROM categories WHERE is_active = true)",
            )
            .fetch_one(pool.get_ref())
            .await?;

            let value = serde_json::json!({
                "total_products": total_products,
                "total_categories": total_categories,
            });
            conn.set_ex::<_, _, ()>(cache_key, value.to_string(), STATS_CACHE_TTL)
                .await?;
            value
        }
    };

    data["credit_balance"] = serde_json::json!(live_balance(pool.get_ref(), user.0.id).await?);
    Ok(HttpResponse::Ok().json(data))
}

/// Live balance is deliberately NOT cached: it must reflect the last
/// transaction (purchases/refunds) within seconds.
async fn live_balance(
    pool: &PgPool,
    user_id: uuid::Uuid,
) -> Result<rust_decimal::Decimal, ApiError> {
    Ok(sqlx::query_scalar::<_, rust_decimal::Decimal>(
        "SELECT credit_balance FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?)
}
