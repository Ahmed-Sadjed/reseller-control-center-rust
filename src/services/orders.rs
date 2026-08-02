use rust_decimal::Decimal;
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    config::Settings,
    models::{Order, ProductVariant, User},
    providers,
    utils::crypto::encrypt,
};

#[derive(Debug, Error)]
pub enum ReservationError {
    #[error("insufficient credits. Required: {required}, Available: {available}")]
    InsufficientCredits { required: Decimal, available: Decimal },

    #[error("product or variant not found")]
    ProductNotFound,

    #[error("variant is not active")]
    VariantInactive,

    #[error("quantity must be between 1 and 50")]
    InvalidQuantity,

    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, Error)]
pub enum FulfillmentError {
    #[error("provider error: {0}")]
    Provider(#[from] providers::ProviderError),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("order not found: {0}")]
    OrderNotFound(Uuid),
}

pub struct ReservedOrder {
    pub order: Order,
    pub balance_after: Decimal,
}

pub struct PurchaseExtras {
    pub mac: Option<String>,
    pub note: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub template_id: Option<String>,
    pub dns_domain_id: Option<String>,
}

/// Mirrors Django `reserve_phase`: lock reseller, check credits, deduct,
/// create PENDING order + CreditTransaction + IdempotencyKey (one transaction).
pub async fn reserve_order(
    pool: &PgPool,
    reseller_id: Uuid,
    variant_id: Uuid,
    quantity: i32,
    idempotency_key: &str,
    extras: &PurchaseExtras,
) -> Result<ReservedOrder, ReservationError> {
    if !(1..=50).contains(&quantity) {
        return Err(ReservationError::InvalidQuantity);
    }

    let variant = sqlx::query_as::<_, ProductVariant>(
        "SELECT id, product_id, duration_months, is_lifetime, external_pack_id, price_in_credits, \
         is_active, created_at, updated_at FROM product_variants WHERE id = $1",
    )
    .bind(variant_id)
    .fetch_optional(pool)
    .await?
    .ok_or(ReservationError::ProductNotFound)?;

    if !variant.is_active {
        return Err(ReservationError::VariantInactive);
    }

    let product = sqlx::query_as::<_, crate::models::Product>(
        "SELECT id, name, category_id, provider_id, description, external_pack_id, duration_months, \
         price_in_credits, image, is_active, is_manual, credential_type, created_at, updated_at \
         FROM products WHERE id = $1 AND is_active = true",
    )
    .bind(variant.product_id)
    .fetch_optional(pool)
    .await?
    .ok_or(ReservationError::ProductNotFound)?;

    let total = variant.price_in_credits * Decimal::from(quantity);

    let mut tx = pool.begin().await?;

    let reseller = sqlx::query_as::<_, User>(
        "SELECT id, username, email, password_hash, role, credit_balance, is_active, uuid, date_joined \
         FROM users WHERE id = $1 FOR UPDATE",
    )
    .bind(reseller_id)
    .fetch_one(&mut *tx)
    .await?;

    if reseller.credit_balance < total {
        tx.rollback().await?;
        return Err(ReservationError::InsufficientCredits {
            required: total,
            available: reseller.credit_balance,
        });
    }

    let balance_after = reseller.credit_balance - total;
    sqlx::query("UPDATE users SET credit_balance = $1 WHERE id = $2")
        .bind(balance_after)
        .bind(reseller_id)
        .execute(&mut *tx)
        .await?;

    let product_name = format!(
        "{} - {}",
        product.name,
        if variant.is_lifetime {
            "Lifetime".to_string()
        } else {
            format!("{} Month(s)", variant.duration_months.unwrap_or(1))
        }
    );

    let order = sqlx::query_as::<_, Order>(
        "INSERT INTO orders (reseller_id, product_id, variant_id, quantity, unit_price_at_purchase, \
         product_name_at_purchase, total_credits, status, idempotency_key, mac, note, username, password, \
         template_id, dns_domain_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, 'PENDING', $8, $9, $10, $11, $12, $13, $14) \
         RETURNING id, uuid, reseller_id, product_id, variant_id, quantity, unit_price_at_purchase, \
         product_name_at_purchase, total_credits, status, failure_reason, idempotency_key, created_at, expires_at, \
         mac, note, username, password, template_id, dns_domain_id",
    )
    .bind(reseller_id)
    .bind(product.id)
    .bind(variant.id)
    .bind(quantity)
    .bind(variant.price_in_credits)
    .bind(&product_name)
    .bind(total)
    .bind(idempotency_key)
    .bind(&extras.mac)
    .bind(&extras.note)
    .bind(&extras.username)
    .bind(&extras.password)
    .bind(&extras.template_id)
    .bind(&extras.dns_domain_id)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO credit_transactions (reseller_id, delta, balance_after, actor, reason, reference_order_id) \
         VALUES ($1, $2, $3, 'RESELLER', $4, $5)",
    )
    .bind(reseller_id)
    .bind(-total)
    .bind(balance_after)
    .bind(format!("Purchase #{}", order.uuid))
    .bind(order.id)
    .execute(&mut *tx)
    .await?;

    sqlx::query("INSERT INTO idempotency_keys (reseller_id, key, order_id) VALUES ($1, $2, $3)")
        .bind(reseller_id)
        .bind(idempotency_key)
        .bind(order.id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(ReservedOrder {
        order,
        balance_after,
    })
}

/// Mirrors Django `fulfill_sync`: call the provider adapter once per quantity,
/// persist a Credential per success, refund unprocessed items on partial failure.
pub async fn fulfill_order(
    pool: &PgPool,
    settings: &Settings,
    order_id: Uuid,
) -> Result<(), FulfillmentError> {
    // Atomic claim: only the first caller wins a PENDING order; concurrent
    // fulfillment (background worker vs WhatsApp admin flow) is prevented.
    let claimed = sqlx::query("UPDATE orders SET status = 'PROCESSING' WHERE uuid = $1 AND status = 'PENDING'")
        .bind(order_id)
        .execute(pool)
        .await?;
    if claimed.rows_affected() == 0 {
        tracing::warn!(order_id = %order_id, "skipping fulfillment: order not pending (already claimed)");
        return Ok(());
    }

    let order = sqlx::query_as::<_, Order>(
        "SELECT id, uuid, reseller_id, product_id, variant_id, quantity, unit_price_at_purchase, \
         product_name_at_purchase, total_credits, status, failure_reason, idempotency_key, created_at, expires_at, \
         mac, note, username, password, template_id, dns_domain_id \
         FROM orders WHERE uuid = $1",
    )
    .bind(order_id)
    .fetch_optional(pool)
    .await?
    .ok_or(FulfillmentError::OrderNotFound(order_id))?;

    tracing::info!(order_id = %order.uuid, "fulfilling claimed order");

    let provider_row = sqlx::query_as::<_, (Uuid, String, Option<String>, Option<Vec<u8>>)>(
        "SELECT p.id, p.adapter_key, p.api_endpoint, p.api_token FROM providers p \
         JOIN products pr ON pr.provider_id = p.id WHERE pr.id = $1",
    )
    .bind(order.product_id)
    .fetch_optional(pool)
    .await?;

    let (_, adapter_key, endpoint, api_token) = match provider_row {
        Some(row) => row,
        None => {
            let reason = "no provider configured for product";
            mark_failed(pool, &order, reason).await?;
            return Ok(());
        }
    };

    // Variant + customer info needed to build the provision context.
    let variant = sqlx::query_as::<_, ProductVariant>(
        "SELECT id, product_id, duration_months, is_lifetime, external_pack_id, price_in_credits, \
         is_active, created_at, updated_at FROM product_variants WHERE id = $1",
    )
    .bind(order.variant_id)
    .fetch_optional(pool)
    .await?
    .ok_or(FulfillmentError::OrderNotFound(order_id))?;

    let customer_username = sqlx::query_scalar::<_, String>("SELECT username FROM users WHERE id = $1")
        .bind(order.reseller_id)
        .fetch_optional(pool)
        .await?
        .unwrap_or_else(|| "reseller".to_string());

    let adapter = providers::get_provider(
        &adapter_key,
        endpoint.as_deref(),
        api_token.as_deref().and_then(|t| String::from_utf8(t.to_vec()).ok()).as_deref(),
        settings,
    )?;

    // One provider call per purchased unit; each success persists a Credential.
    let ctx = providers::ProvisionContext {
        product_name: order.product_name_at_purchase.clone(),
        duration_months: variant.duration_months,
        external_pack_id: variant.external_pack_id,
        order_id: order.uuid,
        customer_username,
        mac: order.mac.clone(),
        preferred_username: order.username.clone(),
        preferred_password: order.password.clone(),
        template_id: order.template_id.clone(),
        dns_domain_id: order.dns_domain_id.clone(),
        extra: serde_json::json!({}),
    };

    let mut credentials_created: i32 = 0;
    let mut failure_reason: Option<String> = None;

    for _ in 0..order.quantity {
        match adapter.provision(&ctx).await {
            Ok(result) => {
                let encrypted_password = encrypt(
                    result.password.as_bytes(),
                    settings.master_encryption_key.as_bytes(),
                )
                .unwrap_or_default();

                sqlx::query(
                    "INSERT INTO credentials (order_id, external_username, streaming_username, \
                     encrypted_password, dns_domain, m3u_url, data, expires_at, is_revoked) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, false)",
                )
                .bind(order.id)
                .bind(&result.username)
                .bind(&result.username)
                .bind(&encrypted_password)
                .bind(&result.dns)
                .bind(&result.m3u_url)
                .bind(&result.extra)
                .bind(&result.expires_at)
                .execute(pool)
                .await?;
                credentials_created += 1;
            }
            Err(e) => {
                failure_reason = Some(e.to_string());
                break;
            }
        }
    }

    if failure_reason.is_none() {
        sqlx::query("UPDATE orders SET status = 'COMPLETED' WHERE id = $1")
            .bind(order.id)
            .execute(pool)
            .await?;
        tracing::info!(
            order_id = %order.uuid,
            credentials = credentials_created,
            "order fulfilled"
        );
        return Ok(());
    }

    // Partial failure: refund unprocessed items, keep the created credentials
    // (Django parity: the order stays COMPLETED with the reduced quantity and
    // a failure_reason; only a TOTAL failure marks the order FAILED).
    let unprocessed = order.quantity - credentials_created;
    if unprocessed > 0 {
        let refund = order.unit_price_at_purchase * Decimal::from(unprocessed);
        let mut tx = pool.begin().await?;
        let reseller = sqlx::query_as::<_, User>(
            "SELECT id, username, email, password_hash, role, credit_balance, is_active, uuid, date_joined \
             FROM users WHERE id = $1 FOR UPDATE",
        )
        .bind(order.reseller_id)
        .fetch_one(&mut *tx)
        .await?;
        let balance_after = reseller.credit_balance + refund;
        sqlx::query("UPDATE users SET credit_balance = $1 WHERE id = $2")
            .bind(balance_after)
            .bind(order.reseller_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO credit_transactions (reseller_id, delta, balance_after, actor, reason, reference_order_id) \
             VALUES ($1, $2, $3, 'SYSTEM', $4, $5)",
        )
        .bind(order.reseller_id)
        .bind(refund)
        .bind(balance_after)
        .bind(format!(
            "Partial refund for {unprocessed} unprocessed item(s) in order #{}",
            order.uuid
        ))
        .bind(order.id)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE orders SET quantity = $1 WHERE id = $2")
            .bind(credentials_created)
            .bind(order.id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
    }

    if credentials_created > 0 {
        sqlx::query("UPDATE orders SET status = 'COMPLETED', failure_reason = $1 WHERE id = $2")
            .bind(failure_reason.as_deref().unwrap_or("unknown error"))
            .bind(order.id)
            .execute(pool)
            .await?;
        tracing::warn!(
            order_id = %order.uuid,
            credentials = credentials_created,
            reason = failure_reason.as_deref().unwrap_or("unknown error"),
            "order partially fulfilled"
        );
        return Ok(());
    }

    mark_failed(pool, &order, failure_reason.as_deref().unwrap_or("unknown error")).await?;
    Ok(())
}

async fn mark_failed(pool: &PgPool, order: &Order, reason: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE orders SET status = 'FAILED', failure_reason = $1 WHERE id = $2")
        .bind(reason)
        .bind(order.id)
        .execute(pool)
        .await?;
    tracing::warn!(order_id = %order.uuid, reason, "order failed");
    Ok(())
}
