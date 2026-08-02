use actix_web::{web, HttpRequest, HttpResponse};
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    config::Settings,
    error::ApiError,
    middleware::AuthUser,
    models::{Credential, Order},
    services::{fulfill_order, reserve_order, PurchaseExtras, ReservationError},
};

/// The React frontend sends `dns_domain_id` as a JS number; accept both
/// numbers and strings (Django used an integer field).
fn number_or_string<'de, D>(de: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumOrStr {
        Num(i64),
        Str(String),
    }
    let v: Option<NumOrStr> = Option::deserialize(de)?;
    Ok(v.map(|n| match n {
        NumOrStr::Num(i) => i.to_string(),
        NumOrStr::Str(s) => s,
    }))
}

#[derive(Debug, Deserialize)]
pub struct CreateOrderRequest {
    pub variant_id: Uuid,
    #[serde(default = "default_quantity")]
    pub quantity: i32,
    pub mac: Option<String>,
    pub note: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub template_id: Option<String>,
    #[serde(default, deserialize_with = "number_or_string")]
    pub dns_domain_id: Option<String>,
}

fn default_quantity() -> i32 {
    1
}

fn idempotency_key(req: &HttpRequest) -> Result<String, ApiError> {
    req.headers()
        .get("Idempotency-Key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .ok_or_else(|| ApiError::bad_request("Idempotency-Key header is required"))
}

pub async fn create_order(
    pool: web::Data<PgPool>,
    settings: web::Data<Settings>,
    redis_conn: web::Data<redis::aio::MultiplexedConnection>,
    req: HttpRequest,
    user: AuthUser,
    body: web::Json<CreateOrderRequest>,
) -> Result<HttpResponse, ApiError> {
    let key = idempotency_key(&req)?;

    // Replay detection: same reseller + key => return existing order.
    let existing: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT o.uuid, o.status FROM orders o \
         JOIN idempotency_keys ik ON ik.order_id = o.id \
         WHERE ik.reseller_id = $1 AND ik.key = $2",
    )
    .bind(user.0.id)
    .bind(&key)
    .fetch_optional(pool.get_ref())
    .await?;

    if let Some((order_uuid, status)) = existing {
        return Ok(HttpResponse::Conflict().json(serde_json::json!({
            "order_id": order_uuid,
            "status": status,
            "message": "duplicate request: idempotency key already used",
        })));
    }

    // Django parity: the provider drives device requirements.
    let adapter_key: String = sqlx::query_scalar(
        "SELECT COALESCE(p.adapter_key, 'mock') FROM product_variants pv \
         JOIN products pr ON pr.id = pv.product_id \
         LEFT JOIN providers p ON p.id = pr.provider_id \
         WHERE pv.id = $1",
    )
    .bind(body.variant_id)
    .fetch_optional(pool.get_ref())
    .await?
    .unwrap_or_else(|| "mock".to_string());

    if adapter_key == "hotplayer" {
        if body.quantity != 1 {
            return Err(ApiError::bad_request(
                "HotPlayer products support only quantity 1 per purchase.",
            ));
        }
        if body.mac.as_deref().map(str::trim).unwrap_or("").is_empty() {
            return Err(ApiError::bad_request(
                "MAC address is required for HotPlayer products.",
            ));
        }
    }

    // MAC validation via provider adapter when provided.
    if let Some(mac) = &body.mac {
        let provider = crate::providers::get_provider(&adapter_key, None, None, &settings)?;
        let check = provider.check_device(mac).await?;
        if !check.allowed {
            return Err(ApiError::bad_request(
                check.reason.unwrap_or_else(|| "device not allowed".to_string()),
            ));
        }
    }

    let extras = PurchaseExtras {
        mac: body.mac.clone(),
        note: body.note.clone(),
        username: body.username.clone(),
        password: body.password.clone(),
        template_id: body.template_id.clone(),
        dns_domain_id: body.dns_domain_id.clone(),
    };

    let reserved = match reserve_order(
        pool.get_ref(),
        user.0.id,
        body.variant_id,
        body.quantity,
        &key,
        &extras,
    )
    .await
    {
        Ok(r) => r,
        Err(ReservationError::InsufficientCredits { required, available }) => {
            return Err(ApiError::bad_request(format!(
                "Insufficient credits. Required: {required}, Available: {available}"
            )));
        }
        Err(ReservationError::ProductNotFound) => {
            return Err(ApiError::NotFound("product or variant".into()));
        }
        Err(ReservationError::VariantInactive) => {
            return Err(ApiError::bad_request("variant is not active"));
        }
        Err(ReservationError::InvalidQuantity) => {
            return Err(ApiError::bad_request("quantity must be between 1 and 50"));
        }
        Err(ReservationError::Database(e)) => return Err(ApiError::Database(e)),
    };

    let order_id = reserved.order.uuid;

    // Django parity: quantity <= ASYNC_THRESHOLD fulfills synchronously and
    // returns the credentials in the 201 response; larger quantities go to
    // the Redis Stream worker and return 202 PENDING.
    if reserved.order.quantity <= settings.async_threshold {
        fulfill_order(pool.get_ref(), settings.get_ref(), order_id).await?;

        let outcome: (String, Option<String>) = sqlx::query_as(
            "SELECT status, failure_reason FROM orders WHERE uuid = $1",
        )
        .bind(order_id)
        .fetch_one(pool.get_ref())
        .await?;

        let (status, failure_reason) = outcome;
        if status != "COMPLETED" {
            return Err(ApiError::bad_request(
                failure_reason.unwrap_or_else(|| "order fulfillment failed".to_string()),
            ));
        }

        let credentials = load_order_credentials(
            pool.get_ref(),
            settings.get_ref(),
            order_id,
            user.0.id,
        )
        .await?;

        return Ok(HttpResponse::Created().json(serde_json::json!({
            "order_id": order_id,
            "status": status,
            "credentials": credentials,
            "partial_failure": failure_reason.is_some(),
            "failure_reason": failure_reason,
            "total_credits": reserved.order.total_credits,
            "balance_after": reserved.balance_after,
        })));
    }

    // Enqueue background fulfillment on the Redis Stream (worker in main.rs).
    if let Err(e) = crate::queue::enqueue_order(redis_conn.get_ref(), order_id).await {
        tracing::error!(order_id = %order_id, error = %e, "failed to enqueue order on stream");
    }

    Ok(HttpResponse::Accepted().json(serde_json::json!({
        "status": "PENDING",
        "order_id": reserved.order.uuid,
        "total_credits": reserved.order.total_credits,
        "balance_after": reserved.balance_after,
    })))
}

pub async fn list_orders(
    pool: web::Data<PgPool>,
    user: AuthUser,
) -> Result<HttpResponse, ApiError> {
    let rows = sqlx::query_as::<_, Order>(
        "SELECT id, uuid, reseller_id, product_id, variant_id, quantity, unit_price_at_purchase, \
         product_name_at_purchase, total_credits, status, failure_reason, idempotency_key, created_at, expires_at, \
         mac, note, username, password, template_id, dns_domain_id \
         FROM orders WHERE reseller_id = $1 ORDER BY created_at DESC LIMIT 50",
    )
    .bind(user.0.id)
    .fetch_all(pool.get_ref())
    .await?;

    Ok(HttpResponse::Ok().json(rows))
}

pub async fn order_detail(
    pool: web::Data<PgPool>,
    user: AuthUser,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, ApiError> {
    let uuid = path.into_inner();
    let order = sqlx::query_as::<_, Order>(
        "SELECT id, uuid, reseller_id, product_id, variant_id, quantity, unit_price_at_purchase, \
         product_name_at_purchase, total_credits, status, failure_reason, idempotency_key, created_at, expires_at, \
         mac, note, username, password, template_id, dns_domain_id \
         FROM orders WHERE uuid = $1 AND reseller_id = $2",
    )
    .bind(uuid)
    .bind(user.0.id)
    .fetch_optional(pool.get_ref())
    .await?
    .ok_or_else(|| ApiError::NotFound("order".into()))?;

    Ok(HttpResponse::Ok().json(order))
}

pub async fn order_status(
    pool: web::Data<PgPool>,
    user: AuthUser,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, ApiError> {
    let uuid = path.into_inner();
    let row: Option<(Uuid, String, Option<String>)> = sqlx::query_as(
        "SELECT uuid, status, failure_reason FROM orders WHERE uuid = $1 AND reseller_id = $2",
    )
    .bind(uuid)
    .bind(user.0.id)
    .fetch_optional(pool.get_ref())
    .await?;

    let (order_id, status, failure_reason) = row.ok_or_else(|| ApiError::NotFound("order".into()))?;
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "order_id": order_id,
        "status": status,
        "failure_reason": failure_reason,
    })))
}

pub async fn order_credentials(
    pool: web::Data<PgPool>,
    settings: web::Data<Settings>,
    user: AuthUser,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, ApiError> {
    let uuid = path.into_inner();

    // Django parity: credentials are only revealed once the order is COMPLETED.
    let status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM orders WHERE uuid = $1 AND reseller_id = $2",
    )
    .bind(uuid)
    .bind(user.0.id)
    .fetch_optional(pool.get_ref())
    .await?;

    match status {
        None => return Err(ApiError::NotFound("order".into())),
        Some(s) if s != "COMPLETED" => {
            return Err(ApiError::bad_request("Order is not completed."));
        }
        Some(_) => {}
    }

    let out = load_order_credentials(pool.get_ref(), settings.get_ref(), uuid, user.0.id).await?;
    Ok(HttpResponse::Ok().json(out))
}

async fn load_order_credentials(
    pool: &PgPool,
    settings: &Settings,
    order_uuid: Uuid,
    reseller_id: Uuid,
) -> Result<Vec<crate::models::CredentialWithPassword>, ApiError> {
    let rows = sqlx::query_as::<_, Credential>(
        "SELECT cr.id, cr.order_id, cr.external_username, cr.streaming_username, cr.encrypted_password, \
         cr.dns_domain, cr.m3u_url, cr.data, cr.expires_at, cr.is_revoked, cr.created_at \
         FROM credentials cr JOIN orders o ON o.id = cr.order_id \
         WHERE o.uuid = $1 AND o.reseller_id = $2 AND cr.is_revoked = false",
    )
    .bind(order_uuid)
    .bind(reseller_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|c| c.with_password(settings.master_encryption_key.as_bytes()))
        .collect())
}

/// POST /api/check-device/ {mac} — Django CheckDeviceView parity.
#[derive(Debug, Deserialize)]
pub struct CheckDeviceRequest {
    pub mac: Option<String>,
}

pub async fn check_device(
    pool: web::Data<PgPool>,
    settings: web::Data<Settings>,
    _user: AuthUser,
    body: web::Json<CheckDeviceRequest>,
) -> Result<HttpResponse, ApiError> {
    let mac = body.mac.clone().unwrap_or_default();
    let mac = mac.trim().to_uppercase();
    if mac.is_empty() {
        return Err(ApiError::bad_request("MAC address is required."));
    }
    let valid_mac = mac.len() == 17
        && mac.split(':').count() == 6
        && mac.split(':').all(|octet| octet.len() == 2 && octet.chars().all(|c| c.is_ascii_hexdigit()));
    if !valid_mac {
        return Err(ApiError::bad_request(
            "Invalid MAC address. Use format XX:XX:XX:XX:XX:XX",
        ));
    }

    let provider_row = sqlx::query_as::<_, (String, Option<String>, Option<Vec<u8>>)>(
        "SELECT adapter_key, api_endpoint, api_token FROM providers \
         WHERE adapter_key = 'hotplayer' AND is_active = true \
         ORDER BY created_at ASC LIMIT 1",
    )
    .fetch_optional(pool.get_ref())
    .await?;

    let Some((adapter_key, endpoint, api_token)) = provider_row else {
        return Ok(HttpResponse::Ok().json(serde_json::json!({
            "found": false,
            "mac": mac,
            "status": "error",
            "message": "No active HotPlayer provider configured.",
        })));
    };

    let token = api_token
        .as_deref()
        .and_then(|t| String::from_utf8(t.to_vec()).ok());
    let adapter = crate::providers::get_provider(
        &adapter_key,
        endpoint.as_deref(),
        token.as_deref(),
        &settings,
    )?;

    match adapter.check_device(&mac).await {
        Ok(result) if result.allowed => Ok(HttpResponse::Ok().json(serde_json::json!({
            "found": true,
            "mac": mac,
            "plan": null,
            "expires_at": null,
            "days_remaining": null,
            "status": "active",
        }))),
        Ok(result) => Ok(HttpResponse::Ok().json(serde_json::json!({
            "found": false,
            "mac": mac,
            "status": "not_found",
            "message": result.reason,
        }))),
        Err(e) => Ok(HttpResponse::Ok().json(serde_json::json!({
            "found": false,
            "mac": mac,
            "status": "error",
            "message": format!("{mac}\n{e}"),
        }))),
    }
}

/// GET /api/credentials/?provider=<adapter_key> — Django CredentialListView
/// parity (own credentials from COMPLETED orders, password deliberately
/// excluded).
#[derive(Debug, Deserialize)]
pub struct CredentialListQuery {
    pub provider: Option<String>,
    pub page: Option<i64>,
    #[serde(rename = "page_size")]
    pub page_size: Option<i64>,
}

#[derive(Debug, serde::Serialize)]
struct CredentialListItem {
    id: Uuid,
    username: Option<String>,
    url: Option<String>,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    is_revoked: bool,
    provider_adapter_key: Option<String>,
    product_name: String,
    order_uuid: Uuid,
    order_created: chrono::DateTime<chrono::Utc>,
    credential_data: serde_json::Value,
    provider_config: Option<serde_json::Value>,
    created_at: chrono::DateTime<chrono::Utc>,
    product_id: Uuid,
}

fn m3u_host(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    match parsed.port() {
        Some(port) => Some(format!("{}://{}:{}", parsed.scheme(), parsed.host_str()?, port)),
        None => Some(format!("{}://{}", parsed.scheme(), parsed.host_str()?)),
    }
}

pub async fn credentials_list(
    pool: web::Data<PgPool>,
    user: AuthUser,
    query: web::Query<CredentialListQuery>,
) -> Result<HttpResponse, ApiError> {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(20).clamp(1, 100);

    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM credentials cr \
         JOIN orders o ON o.id = cr.order_id \
         LEFT JOIN products pr ON pr.id = o.product_id \
         LEFT JOIN providers p ON p.id = pr.provider_id \
         WHERE o.reseller_id = $1 AND o.status = 'COMPLETED' AND cr.is_revoked = false \
           AND ($2::text IS NULL OR p.adapter_key = $2)",
    )
    .bind(user.0.id)
    .bind(&query.provider)
    .fetch_one(pool.get_ref())
    .await?;

    let rows: Vec<(Uuid, Option<String>, Option<String>, Option<chrono::DateTime<chrono::Utc>>,
        bool, Option<String>, String, Uuid, chrono::DateTime<chrono::Utc>,
        serde_json::Value, Option<serde_json::Value>, chrono::DateTime<chrono::Utc>, Uuid)> =
        sqlx::query_as(
            "SELECT cr.id, cr.streaming_username, cr.m3u_url, cr.expires_at, cr.is_revoked, \
                    p.adapter_key, pr.name, o.uuid, o.created_at, cr.data, p.extra_config, cr.created_at, pr.id \
             FROM credentials cr \
             JOIN orders o ON o.id = cr.order_id \
             LEFT JOIN products pr ON pr.id = o.product_id \
             LEFT JOIN providers p ON p.id = pr.provider_id \
             WHERE o.reseller_id = $1 AND o.status = 'COMPLETED' AND cr.is_revoked = false \
               AND ($2::text IS NULL OR p.adapter_key = $2) \
             ORDER BY cr.created_at DESC LIMIT $3 OFFSET $4",
        )
        .bind(user.0.id)
        .bind(&query.provider)
        .bind(page_size)
        .bind((page - 1) * page_size)
        .fetch_all(pool.get_ref())
        .await?;

    let items: Vec<CredentialListItem> = rows
        .into_iter()
        .map(
            |(id, username, m3u_url, expires_at, is_revoked, adapter_key, product_name,
              order_uuid, order_created, data, extra_config, created_at, product_id)| {
                CredentialListItem {
                    id,
                    username,
                    url: m3u_url.as_deref().and_then(m3u_host),
                    expires_at,
                    is_revoked,
                    provider_adapter_key: adapter_key,
                    product_name,
                    order_uuid,
                    order_created,
                    credential_data: data,
                    provider_config: extra_config,
                    created_at,
                    product_id,
                }
            },
        )
        .collect();

    let total_pages = (count as i64 + page_size - 1) / page_size;
    let next: Value = if page < total_pages.max(1) {
        Value::String(format!("/api/credentials?page={}&page_size={}", page + 1, page_size))
    } else {
        Value::Null
    };
    let previous: Value = if page > 1 {
        Value::String(format!("/api/credentials?page={}&page_size={}", page - 1, page_size))
    } else {
        Value::Null
    };
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "count": count,
        "total_pages": total_pages,
        "next": next,
        "previous": previous,
        "results": items,
    })))
}
