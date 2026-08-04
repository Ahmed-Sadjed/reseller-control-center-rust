use actix_web::http::header::CONTENT_TYPE;
use actix_web::{web, FromRequest, HttpRequest, HttpResponse};
use futures_util::StreamExt;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Row};
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

use crate::{
    config::Settings, error::ApiError, middleware::AuthUser, models::User,
    utils::crypto::hash_password,
};

fn require_admin(user: &User) -> Result<(), ApiError> {
    if user.role != "ADMIN" {
        return Err(ApiError::Forbidden);
    }
    Ok(())
}

fn page_bounds(page: Option<i64>, page_size: Option<i64>) -> (i64, i64) {
    let page = page.unwrap_or(1).max(1);
    let page_size = page_size.unwrap_or(20).clamp(1, 100);
    (page, page_size)
}

fn page_url(base: &str, page: i64, page_size: i64) -> serde_json::Value {
    if page >= 1 {
        serde_json::Value::String(format!("{base}?page={}&page_size={}", page + 1, page_size))
    } else {
        serde_json::Value::Null
    }
}

#[derive(Debug, Serialize, FromRow)]
pub struct ResellerListItem {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub role: String,
    pub credit_balance: Decimal,
    pub is_active: bool,
    pub uuid: Uuid,
    pub date_joined: chrono::DateTime<chrono::Utc>,
    pub last_login: Option<chrono::DateTime<chrono::Utc>>,
    pub order_count: i64,
    pub total_revenue: Decimal,
}

const RESELLER_LIST_SELECT: &str = "SELECT u.id, u.username, u.email, u.role, u.credit_balance, \
     u.is_active, u.uuid, u.date_joined, u.last_login, \
     (SELECT count(*) FROM orders o WHERE o.reseller_id = u.id AND o.status = 'COMPLETED') AS order_count, \
     (SELECT COALESCE(sum(o.total_credits), 0) FROM orders o \
      WHERE o.reseller_id = u.id AND o.status = 'COMPLETED') AS total_revenue \
     FROM users u";

#[derive(Debug, Deserialize)]
pub struct ResellerQuery {
    pub search: Option<String>,
    pub status: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

pub async fn resellers_list(
    pool: web::Data<PgPool>,
    user: AuthUser,
    query: web::Query<ResellerQuery>,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let (page, page_size) = page_bounds(query.page, query.page_size);

    let mut where_qb: sqlx::QueryBuilder<sqlx::Postgres> =
        sqlx::QueryBuilder::new("WHERE u.role = 'RESELLER'");
    if let Some(search) = &query.search {
        where_qb
            .push(" AND u.username ILIKE ")
            .push_bind(format!("%{search}%"));
    }
    if let Some(status) = &query.status {
        if status == "active" {
            where_qb.push(" AND u.is_active = true");
        } else if status == "inactive" {
            where_qb.push(" AND u.is_active = false");
        }
    }
    let where_sql = where_qb.sql().to_string();

    let count: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM users u {where_sql}"))
        .fetch_one(pool.get_ref())
        .await?;

    let rows: Vec<ResellerListItem> = sqlx::query_as(&format!(
        "{RESELLER_LIST_SELECT} {where_sql} ORDER BY u.date_joined DESC LIMIT $1 OFFSET $2"
    ))
    .bind(page_size)
    .bind((page - 1) * page_size)
    .fetch_all(pool.get_ref())
    .await?;

    let total_pages = (count + page_size - 1) / page_size;
    let next = if page < total_pages {
        page_url("/api/dashboard/resellers", page, page_size)
    } else {
        serde_json::Value::Null
    };
    let previous = if page > 1 {
        page_url("/api/dashboard/resellers", page - 2, page_size)
    } else {
        serde_json::Value::Null
    };

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "count": count,
        "total_pages": total_pages,
        "next": next,
        "previous": previous,
        "results": rows,
    })))
}

#[derive(Debug, Deserialize)]
pub struct CreateResellerRequest {
    pub username: String,
    pub password: String,
    pub password_confirm: String,
    pub initial_credits: Option<Decimal>,
}

/// Auto-generate an unused {username}@reseller.local email, deduped with a
/// numeric suffix when the base is taken (Django parity).
async fn auto_email(pool: &PgPool, username: &str) -> Result<String, ApiError> {
    for i in 0..100 {
        let candidate = if i == 0 {
            format!("{username}@reseller.local")
        } else {
            format!("{username}{i}@reseller.local")
        };
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users WHERE email = $1)")
                .bind(&candidate)
                .fetch_one(pool)
                .await?;
        if !exists {
            return Ok(candidate);
        }
    }
    Err(ApiError::Internal(
        "could not allocate reseller email".into(),
    ))
}

pub async fn resellers_create(
    pool: web::Data<PgPool>,
    user: AuthUser,
    body: web::Json<CreateResellerRequest>,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;

    if body.username.len() > 150 || body.username.is_empty() {
        return Err(ApiError::bad_request(
            "Username is required and must be at most 150 characters.",
        ));
    }
    if body.password.len() < 6 {
        return Err(ApiError::bad_request(
            "Password must be at least 6 characters.",
        ));
    }
    if body.password != body.password_confirm {
        return Err(ApiError::bad_request("Passwords do not match."));
    }
    let taken: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users WHERE username = $1)")
        .bind(&body.username)
        .fetch_one(pool.get_ref())
        .await?;
    if taken {
        return Err(ApiError::bad_request(format!(
            "A user with username '{}' already exists.",
            body.username
        )));
    }

    let email = auto_email(pool.get_ref(), &body.username).await?;
    let hash = hash_password(&body.password)
        .map_err(|e| ApiError::Internal(format!("hash password: {e}")))?;
    let initial = body.initial_credits.unwrap_or_default();

    let mut tx = pool.begin().await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (username, email, password_hash, role, credit_balance, is_active, is_staff, is_superuser, created_by) \
         VALUES ($1, $2, $3, 'RESELLER', $4, true, false, false, $5) RETURNING id",
    )
    .bind(&body.username)
    .bind(&email)
    .bind(hash)
    .bind(initial)
    .bind(user.0.id)
    .fetch_one(&mut *tx)
    .await?;

    if initial > Decimal::ZERO {
        sqlx::query(
            "INSERT INTO credit_transactions (reseller_id, delta, balance_after, actor, reason) \
             VALUES ($1, $2, $2, 'ADMIN', 'Initial credits on account creation')",
        )
        .bind(id)
        .bind(initial)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    let row: ResellerListItem = sqlx::query_as(&format!("{RESELLER_LIST_SELECT} WHERE u.id = $1"))
        .bind(id)
        .fetch_one(pool.get_ref())
        .await?;

    Ok(HttpResponse::Created().json(row))
}

#[derive(Debug, Serialize, FromRow)]
pub struct ResellerDetail {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub role: String,
    pub credit_balance: Decimal,
    pub is_active: bool,
    pub uuid: Uuid,
    pub date_joined: chrono::DateTime<chrono::Utc>,
    pub last_login: Option<chrono::DateTime<chrono::Utc>>,
    pub created_by: Option<Uuid>,
    pub created_by_username: Option<String>,
    pub order_count: i64,
    pub total_revenue: Decimal,
}

const RESELLER_DETAIL_SELECT: &str = "SELECT u.id, u.username, u.email, u.role, u.credit_balance, \
     u.is_active, u.uuid, u.date_joined, u.last_login, u.created_by, cb.username AS created_by_username, \
     (SELECT count(*) FROM orders o WHERE o.reseller_id = u.id) AS order_count, \
     (SELECT COALESCE(sum(o.total_credits), 0) FROM orders o \
      WHERE o.reseller_id = u.id AND o.status = 'COMPLETED') AS total_revenue \
     FROM users u LEFT JOIN users cb ON cb.id = u.created_by";

async fn load_reseller_detail(pool: &PgPool, id: Uuid) -> Result<ResellerDetail, ApiError> {
    sqlx::query_as::<_, ResellerDetail>(&format!("{RESELLER_DETAIL_SELECT} WHERE u.id = $1"))
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(ApiError::NotFound("reseller not found".into()))
}

pub async fn reseller_detail(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    user: AuthUser,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let row = load_reseller_detail(pool.get_ref(), path.into_inner()).await?;
    Ok(HttpResponse::Ok().json(row))
}

#[derive(Debug, Deserialize)]
pub struct UpdateResellerRequest {
    pub username: Option<String>,
    pub password: Option<String>,
    pub is_active: Option<bool>,
}

pub async fn reseller_update(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    user: AuthUser,
    body: web::Json<UpdateResellerRequest>,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let id = path.into_inner();
    load_reseller_detail(pool.get_ref(), id).await?; // 404 if missing

    if let Some(username) = &body.username {
        if username.is_empty() || username.len() > 150 {
            return Err(ApiError::bad_request(
                "Username must be at most 150 characters.",
            ));
        }
        let taken: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM users WHERE username = $1 AND id <> $2)",
        )
        .bind(username)
        .bind(id)
        .fetch_one(pool.get_ref())
        .await?;
        if taken {
            return Err(ApiError::bad_request(format!(
                "A user with username '{username}' already exists."
            )));
        }
        sqlx::query("UPDATE users SET username = $1 WHERE id = $2")
            .bind(username)
            .bind(id)
            .execute(pool.get_ref())
            .await?;
    }
    if let Some(password) = &body.password {
        if password.len() < 6 {
            return Err(ApiError::bad_request(
                "Password must be at least 6 characters.",
            ));
        }
        let hash = hash_password(password)
            .map_err(|e| ApiError::Internal(format!("hash password: {e}")))?;
        sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
            .bind(hash)
            .bind(id)
            .execute(pool.get_ref())
            .await?;
    }
    if let Some(is_active) = body.is_active {
        sqlx::query("UPDATE users SET is_active = $1 WHERE id = $2")
            .bind(is_active)
            .bind(id)
            .execute(pool.get_ref())
            .await?;
    }

    let row = load_reseller_detail(pool.get_ref(), id).await?;
    Ok(HttpResponse::Ok().json(row))
}

/// Soft delete (Django parity): is_active = false, 200 with a message.
pub async fn reseller_delete(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    user: AuthUser,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let id = path.into_inner();
    load_reseller_detail(pool.get_ref(), id).await?; // 404 if missing
    sqlx::query("UPDATE users SET is_active = false WHERE id = $1")
        .bind(id)
        .execute(pool.get_ref())
        .await?;
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "detail": "Reseller deactivated."
    })))
}

#[derive(Debug, Deserialize)]
pub struct CreditAdjustRequest {
    pub amount: Decimal,
    pub reason: Option<String>,
}

pub async fn reseller_credits(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    user: AuthUser,
    body: web::Json<CreditAdjustRequest>,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let id = path.into_inner();
    if body.amount == Decimal::ZERO {
        return Err(ApiError::bad_request("Amount must not be zero."));
    }

    let mut tx = pool.begin().await?;
    let current: Option<Decimal> =
        sqlx::query_scalar("SELECT credit_balance FROM users WHERE id = $1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    let current = current.ok_or(ApiError::NotFound("reseller not found".into()))?;

    let new_balance = current + body.amount;
    if new_balance < Decimal::ZERO {
        return Err(ApiError::bad_request(format!(
            "Cannot deduct {} credits. Reseller only has {} credits.",
            body.amount.abs(),
            current
        )));
    }

    let reason = body
        .reason
        .clone()
        .unwrap_or_else(|| "Admin adjustment".into());
    sqlx::query("UPDATE users SET credit_balance = $1 WHERE id = $2")
        .bind(new_balance)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO credit_transactions (reseller_id, delta, balance_after, actor, reason) \
         VALUES ($1, $2, $3, 'ADMIN', $4)",
    )
    .bind(id)
    .bind(body.amount)
    .bind(new_balance)
    .bind(&reason)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "credit_balance": new_balance,
        "detail": "Credits updated successfully.",
    })))
}

#[derive(Debug, Serialize, FromRow)]
pub struct CreditTransactionItem {
    pub id: Uuid,
    pub delta: Decimal,
    pub balance_after: Decimal,
    pub actor: String,
    pub reason: String,
    pub reference_order: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn reseller_transactions(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    user: AuthUser,
    query: web::Query<ResellerQuery>,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let id = path.into_inner();
    let (page, page_size) = page_bounds(query.page, query.page_size);

    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM credit_transactions WHERE reseller_id = $1")
            .bind(id)
            .fetch_one(pool.get_ref())
            .await?;

    let rows: Vec<CreditTransactionItem> = sqlx::query_as(
        "SELECT id, delta, balance_after, actor, reason, reference_order_id AS reference_order, created_at \
         FROM credit_transactions WHERE reseller_id = $1 \
         ORDER BY created_at DESC LIMIT $2 OFFSET $3",
    )
    .bind(id)
    .bind(page_size)
    .bind((page - 1) * page_size)
    .fetch_all(pool.get_ref())
    .await?;

    let total_pages = (count + page_size - 1) / page_size;
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "count": count,
        "total_pages": total_pages,
        "next": page_url("/api/dashboard/resellers", page, page_size),
        "previous": if page > 1 { page_url("/api/dashboard/resellers", page - 2, page_size) } else { serde_json::Value::Null },
        "results": rows,
    })))
}

#[derive(Debug, Serialize, FromRow)]
pub struct ResellerOrderItem {
    pub id: Uuid,
    pub uuid: Uuid,
    pub reseller: Uuid,
    pub reseller_username: String,
    pub product_name_at_purchase: String,
    pub quantity: i32,
    pub total_credits: Decimal,
    pub status: String,
    pub failure_reason: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn reseller_orders(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    user: AuthUser,
    query: web::Query<ResellerQuery>,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let id = path.into_inner();
    let (page, page_size) = page_bounds(query.page, query.page_size);

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM orders WHERE reseller_id = $1")
        .bind(id)
        .fetch_one(pool.get_ref())
        .await?;

    let rows: Vec<ResellerOrderItem> = sqlx::query_as(
        "SELECT o.id, o.uuid, o.reseller_id AS reseller, u.username AS reseller_username, \
                o.product_name_at_purchase, o.quantity, o.total_credits, o.status, \
                o.failure_reason, o.created_at \
         FROM orders o JOIN users u ON u.id = o.reseller_id \
         WHERE o.reseller_id = $1 ORDER BY o.created_at DESC LIMIT $2 OFFSET $3",
    )
    .bind(id)
    .bind(page_size)
    .bind((page - 1) * page_size)
    .fetch_all(pool.get_ref())
    .await?;

    let total_pages = (count + page_size - 1) / page_size;
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "count": count,
        "total_pages": total_pages,
        "next": page_url("/api/dashboard/resellers", page, page_size),
        "previous": if page > 1 { page_url("/api/dashboard/resellers", page - 2, page_size) } else { serde_json::Value::Null },
        "results": rows,
    })))
}

#[derive(Debug, Deserialize)]
pub struct ToggleRequest {
    pub is_active: bool,
}

pub async fn reseller_toggle(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    user: AuthUser,
    body: web::Json<ToggleRequest>,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let id = path.into_inner();

    let result = sqlx::query("UPDATE users SET is_active = $1 WHERE id = $2 AND role = 'RESELLER'")
        .bind(body.is_active)
        .bind(id)
        .execute(pool.get_ref())
        .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("reseller not found".into()));
    }

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "is_active": body.is_active,
        "detail": if body.is_active { "Reseller activated." } else { "Reseller deactivated." },
    })))
}

#[derive(Debug, Serialize)]
pub struct SettingsResponse {
    pub whatsapp_phone: String,
    pub detail: Option<String>,
}

pub async fn settings_get(
    pool: web::Data<PgPool>,
    user: AuthUser,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let whatsapp_phone: String =
        sqlx::query_scalar("SELECT whatsapp_phone FROM users WHERE id = $1")
            .bind(user.0.id)
            .fetch_one(pool.get_ref())
            .await?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "whatsapp_phone": whatsapp_phone })))
}

#[derive(Debug, Deserialize)]
pub struct SettingsUpdateRequest {
    pub whatsapp_phone: Option<String>,
}

pub async fn settings_put(
    pool: web::Data<PgPool>,
    user: AuthUser,
    body: web::Json<SettingsUpdateRequest>,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let whatsapp_phone = body.whatsapp_phone.clone().unwrap_or_default();
    sqlx::query("UPDATE users SET whatsapp_phone = $1 WHERE id = $2")
        .bind(&whatsapp_phone)
        .bind(user.0.id)
        .execute(pool.get_ref())
        .await?;
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "whatsapp_phone": whatsapp_phone,
        "detail": "WhatsApp phone updated.",
    })))
}

// ============================================================================
// Dashboard parity part 2: admin products, variants, categories, providers
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct AdminProductQuery {
    pub search: Option<String>,
    pub category: Option<Uuid>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub status: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct AdminProductItem {
    pub id: Uuid,
    pub name: String,
    pub category: Option<Uuid>,
    pub category_name: Option<String>,
    pub provider: Option<Uuid>,
    pub provider_name: Option<String>,
    pub provider_key: Option<String>,
    pub description: Option<String>,
    pub is_active: bool,
    pub is_manual: bool,
    pub credential_type: Option<String>,
    pub price_in_credits: Option<Decimal>,
    pub duration_months: Option<i32>,
    pub external_pack_id: Option<i32>,
    pub image_url: Option<String>,
    pub variant_count: i64,
    pub total_credentials: i64,
    pub available_credentials: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

const ADMIN_PRODUCT_SELECT: &str = "SELECT p.id, p.name, p.category_id AS category, c.name AS category_name, \
     p.provider_id AS provider, pr.name AS provider_name, pr.adapter_key AS provider_key, \
     p.description, p.is_active, p.is_manual, p.credential_type, p.price_in_credits, \
     p.duration_months, p.external_pack_id, p.image AS image_url, \
     (SELECT count(*) FROM product_variants pv WHERE pv.product_id = p.id)::bigint AS variant_count, \
     (SELECT count(*) FROM manual_credentials mc WHERE mc.product_id = p.id)::bigint AS total_credentials, \
     (SELECT count(*) FROM manual_credentials mc WHERE mc.product_id = p.id AND mc.status = 'available')::bigint AS available_credentials, \
     p.created_at, p.updated_at \
     FROM products p LEFT JOIN categories c ON c.id = p.category_id LEFT JOIN providers pr ON pr.id = p.provider_id";

pub async fn admin_products_list(
    pool: web::Data<PgPool>,
    user: AuthUser,
    query: web::Query<AdminProductQuery>,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let (page, page_size) = page_bounds(query.page, query.page_size);
    let search = query.search.clone();
    let category = query.category;

    let mut static_where = String::new();
    match query.kind.as_deref() {
        Some("manual") => static_where.push_str(" AND p.is_manual = true"),
        Some("whatsapp") => static_where.push_str(" AND pr.adapter_key = 'whatsapp'"),
        Some("api") => static_where
            .push_str(" AND p.is_manual = false AND COALESCE(pr.adapter_key, '') != 'whatsapp'"),
        _ => {}
    }
    match query.status.as_deref() {
        Some("active") => static_where.push_str(" AND p.is_active = true"),
        Some("inactive") => static_where.push_str(" AND p.is_active = false"),
        _ => {}
    }
    // Parameterized search/category: bound unconditionally as NULL-able so the
    // count and rows queries share the same SQL shape.
    static_where.push_str(
        " AND ($1::text IS NULL OR p.name ILIKE '%'||$1||'%') \
         AND ($2::uuid IS NULL OR p.category_id = $2)",
    );

    let count: i64 = sqlx::query_scalar(&format!(
        "SELECT count(*) FROM products p LEFT JOIN categories c ON c.id = p.category_id \
         LEFT JOIN providers pr ON pr.id = p.provider_id WHERE 1=1{static_where}"
    ))
    .bind(&search)
    .bind(category)
    .fetch_one(pool.get_ref())
    .await?;

    let mut rows: Vec<AdminProductItem> = sqlx::query_as(&format!(
        "{ADMIN_PRODUCT_SELECT} WHERE 1=1{static_where} ORDER BY p.created_at DESC \
         LIMIT $3 OFFSET $4"
    ))
    .bind(&search)
    .bind(category)
    .bind(page_size)
    .bind((page - 1) * page_size)
    .fetch_all(pool.get_ref())
    .await?;

    for r in &mut rows {
        r.image_url = media_url(r.image_url.take());
        if !r.is_manual {
            r.total_credentials = 0;
            r.available_credentials = 0;
        }
    }

    let total_pages = (count + page_size - 1) / page_size;
    let base = "/api/dashboard/products";
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "count": count,
        "total_pages": total_pages,
        "next": page_url(base, page, page_size),
        "previous": if page > 1 { page_url(base, page - 2, page_size) } else { serde_json::Value::Null },
        "results": rows,
    })))
}

#[derive(Debug, Deserialize)]
pub struct AdminProductCreateRequest {
    pub name: String,
    pub category: Option<Uuid>,
    pub provider: Option<Uuid>,
    pub description: Option<String>,
    pub image: Option<String>,
    pub is_manual: Option<bool>,
    pub credential_type: Option<String>,
    pub price_in_credits: Option<Decimal>,
    pub duration_months: Option<i32>,
    pub external_pack_id: Option<i32>,
    pub is_active: Option<bool>,
}

/// POST /api/dashboard/products/create/ â€” Django field names (category/provider,
/// not category_id/provider_id). is_manual requires credential_type; non-manual
/// requires provider. Accepts both JSON and multipart/form-data (the admin
/// frontend submits a FormData with an optional image upload).
pub async fn admin_products_create(
    pool: web::Data<PgPool>,
    settings: web::Data<Settings>,
    user: AuthUser,
    req: HttpRequest,
    payload: web::Payload,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let parsed = parse_product_payload(&req, payload, &settings.media_dir).await?;
    let body = parsed.request;

    if body.name.trim().is_empty() {
        return Err(ApiError::BadRequest("name is required.".into()));
    }
    let is_manual = body.is_manual.unwrap_or(false);
    if is_manual && body.credential_type.as_deref().unwrap_or("").is_empty() {
        return Err(ApiError::BadRequest(
            "credential_type is required for manual products.".into(),
        ));
    }
    if !is_manual && body.provider.is_none() {
        return Err(ApiError::BadRequest(
            "provider is required for API products.".into(),
        ));
    }

    let image = parsed.image_path.or_else(|| image_path_from_payload(&body));
    let product: crate::models::Product = sqlx::query_as(
        "INSERT INTO products (name, category_id, provider_id, description, image, is_manual, \
         credential_type, price_in_credits, duration_months, external_pack_id, is_active) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) RETURNING id, name, category_id, \
         provider_id, description, external_pack_id, duration_months, price_in_credits, image, \
         is_active, is_manual, credential_type, created_at, updated_at",
    )
    .bind(&body.name)
    .bind(body.category)
    .bind(body.provider)
    .bind(body.description.clone().unwrap_or_default())
    .bind(&image)
    .bind(is_manual)
    .bind(&body.credential_type)
    .bind(body.price_in_credits)
    .bind(body.duration_months)
    .bind(body.external_pack_id)
    .bind(body.is_active.unwrap_or(true))
    .fetch_one(pool.get_ref())
    .await?;

    let item = load_admin_product(pool.get_ref(), product.id).await?;
    Ok(HttpResponse::Created().json(item))
}

pub async fn admin_products_update(
    pool: web::Data<PgPool>,
    settings: web::Data<Settings>,
    path: web::Path<Uuid>,
    user: AuthUser,
    req: HttpRequest,
    payload: web::Payload,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let product_id = path.into_inner();
    let parsed = parse_product_payload(&req, payload, &settings.media_dir).await?;
    let body = parsed.request;

    if body.name.trim().is_empty() {
        return Err(ApiError::BadRequest("name is required.".into()));
    }

    let mut qb: sqlx::QueryBuilder<sqlx::Postgres> =
        sqlx::QueryBuilder::new("UPDATE products SET updated_at = now()");
    qb.push(", name = ").push_bind(&body.name);
    if let Some(category) = body.category {
        qb.push(", category_id = ").push_bind(category);
    }
    if let Some(provider) = body.provider {
        qb.push(", provider_id = ").push_bind(provider);
    }
    if let Some(description) = &body.description {
        qb.push(", description = ").push_bind(description);
    }
    if let Some(image) = parsed.image_path.or_else(|| image_path_from_payload(&body)) {
        qb.push(", image = ").push_bind(image);
    }
    if let Some(is_manual) = body.is_manual {
        qb.push(", is_manual = ").push_bind(is_manual);
    }
    if let Some(credential_type) = &body.credential_type {
        qb.push(", credential_type = ").push_bind(credential_type);
    }
    if let Some(price) = body.price_in_credits {
        qb.push(", price_in_credits = ").push_bind(price);
    }
    if let Some(duration) = body.duration_months {
        qb.push(", duration_months = ").push_bind(duration);
    }
    if let Some(pack) = body.external_pack_id {
        qb.push(", external_pack_id = ").push_bind(pack);
    }
    if let Some(is_active) = body.is_active {
        qb.push(", is_active = ").push_bind(is_active);
    }
    qb.push(" WHERE id = ").push_bind(product_id);
    qb.build().execute(pool.get_ref()).await?;

    let item = load_admin_product(pool.get_ref(), product_id).await?;
    Ok(HttpResponse::Ok().json(item))
}

// ---------------------------------------------------------------------------
// Product payload parsing: the admin frontend submits products as
// multipart/form-data (FormData + optional image file); API clients send
// application/json. Both are accepted. Uploaded images are persisted under
// <media_dir>/products/ and the products.image column stores the relative
// path ("products/<file>"), exposed to the API as "/media/products/<file>".
// ---------------------------------------------------------------------------

const PRODUCT_IMAGE_LIMIT: usize = 10 * 1024 * 1024;
const PRODUCT_FIELD_LIMIT: usize = 1024 * 1024;

struct ProductPayload {
    request: AdminProductCreateRequest,
    image_path: Option<String>,
}

fn image_path_from_payload(body: &AdminProductCreateRequest) -> Option<String> {
    body.image
        .as_ref()
        .filter(|s| !s.trim().is_empty())
        .cloned()
}

fn parse_opt_uuid(fields: &HashMap<String, String>, name: &str) -> Result<Option<Uuid>, ApiError> {
    match fields.get(name) {
        None => Ok(None),
        Some(s) if s.trim().is_empty() => Ok(None),
        Some(s) => s
            .trim()
            .parse::<Uuid>()
            .map(Some)
            .map_err(|_| ApiError::BadRequest(format!("Invalid value for '{name}'."))),
    }
}

fn parse_opt_bool(fields: &HashMap<String, String>, name: &str) -> Result<Option<bool>, ApiError> {
    match fields.get(name) {
        None => Ok(None),
        Some(s) if s.trim().is_empty() => Ok(None),
        Some(s) => s
            .trim()
            .parse::<bool>()
            .map(Some)
            .map_err(|_| ApiError::BadRequest(format!("Invalid value for '{name}'."))),
    }
}

fn parse_opt_i32(fields: &HashMap<String, String>, name: &str) -> Result<Option<i32>, ApiError> {
    match fields.get(name) {
        None => Ok(None),
        Some(s) if s.trim().is_empty() => Ok(None),
        Some(s) => s
            .trim()
            .parse::<i32>()
            .map(Some)
            .map_err(|_| ApiError::BadRequest(format!("Invalid value for '{name}'."))),
    }
}

fn parse_opt_decimal(
    fields: &HashMap<String, String>,
    name: &str,
) -> Result<Option<Decimal>, ApiError> {
    match fields.get(name) {
        None => Ok(None),
        Some(s) if s.trim().is_empty() => Ok(None),
        Some(s) => s
            .trim()
            .parse::<Decimal>()
            .map(Some)
            .map_err(|_| ApiError::BadRequest(format!("Invalid value for '{name}'."))),
    }
}

fn image_extension(mime_subtype: Option<&str>) -> &'static str {
    match mime_subtype {
        Some("png") => "png",
        Some("jpeg") | Some("jpg") => "jpg",
        Some("webp") => "webp",
        Some("gif") => "gif",
        _ => "img",
    }
}

fn save_uploaded_image(
    media_dir: &str,
    subdir: &str,
    extension: &str,
    data: &[u8],
) -> Result<String, ApiError> {
    let dir = Path::new(media_dir).join(subdir);
    std::fs::create_dir_all(&dir)
        .map_err(|e| ApiError::Internal(format!("create media directory: {e}")))?;
    let filename = format!("{}.{}", Uuid::new_v4().simple(), extension);
    std::fs::write(dir.join(&filename), data)
        .map_err(|e| ApiError::Internal(format!("write uploaded file: {e}")))?;
    Ok(format!("{subdir}/{filename}"))
}

async fn read_body_bytes(mut payload: web::Payload) -> Result<web::BytesMut, ApiError> {
    let mut bytes = actix_web::web::BytesMut::new();
    while let Some(chunk) = payload.next().await {
        let chunk = chunk.map_err(|e| ApiError::Internal(format!("read request body: {e}")))?;
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn is_multipart(req: &HttpRequest) -> bool {
    req.headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase()
        .starts_with("multipart/form-data")
}

/// Reads a whole multipart field into memory, enforcing a byte limit.
async fn read_field(field: &mut actix_multipart::Field, limit: usize) -> Result<Vec<u8>, ApiError> {
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = field.next().await {
        let chunk =
            chunk.map_err(|e| ApiError::BadRequest(format!("failed reading field: {e}")))?;
        buf.extend_from_slice(&chunk);
        if buf.len() > limit {
            return Err(ApiError::BadRequest(format!(
                "Field exceeds the {} MiB limit.",
                limit / (1024 * 1024)
            )));
        }
    }
    Ok(buf)
}

/// Parses a multipart/form-data payload into (text fields, saved image path).
/// The "image" file field is persisted under <media_dir>/<subdir>/ and the
/// returned path is the relative "subdir/<file>" stored in the DB.
async fn parse_multipart_form(
    req: &HttpRequest,
    payload: &mut actix_web::dev::Payload,
    media_dir: &str,
    subdir: &str,
) -> Result<(HashMap<String, String>, Option<String>), ApiError> {
    let mut multipart = actix_multipart::Multipart::from_request(req, payload)
        .await
        .map_err(|e| ApiError::BadRequest(format!("invalid multipart payload: {e}")))?;

    let mut fields: HashMap<String, String> = HashMap::new();
    let mut image: Option<(String, Vec<u8>)> = None;

    while let Some(item) = multipart.next().await {
        let mut field =
            item.map_err(|e| ApiError::BadRequest(format!("invalid multipart field: {e}")))?;
        let name = field.name().unwrap_or("").to_string();
        if name == "image" {
            let data = read_field(&mut field, PRODUCT_IMAGE_LIMIT).await?;
            let subtype = field.content_type().map(|m| m.subtype().as_str());
            image = Some((image_extension(subtype).to_string(), data));
        } else {
            let data = read_field(&mut field, PRODUCT_FIELD_LIMIT).await?;
            fields.insert(name, String::from_utf8_lossy(&data).into_owned());
        }
    }

    let image_path = match image {
        Some((extension, data)) => Some(save_uploaded_image(media_dir, subdir, &extension, &data)?),
        None => None,
    };
    Ok((fields, image_path))
}

async fn parse_product_payload(
    req: &HttpRequest,
    payload: web::Payload,
    media_dir: &str,
) -> Result<ProductPayload, ApiError> {
    if is_multipart(req) {
        let (mut fields, image_path) =
            parse_multipart_form(req, &mut payload.into_inner(), media_dir, "products").await?;
        let request = AdminProductCreateRequest {
            name: fields.remove("name").unwrap_or_default(),
            category: parse_opt_uuid(&fields, "category")?,
            provider: parse_opt_uuid(&fields, "provider")?,
            description: fields
                .remove("description")
                .filter(|s| !s.trim().is_empty()),
            image: None,
            is_manual: parse_opt_bool(&fields, "is_manual")?,
            credential_type: fields
                .remove("credential_type")
                .filter(|s| !s.trim().is_empty()),
            price_in_credits: parse_opt_decimal(&fields, "price_in_credits")?,
            duration_months: parse_opt_i32(&fields, "duration_months")?,
            external_pack_id: parse_opt_i32(&fields, "external_pack_id")?,
            is_active: parse_opt_bool(&fields, "is_active")?,
        };
        Ok(ProductPayload {
            request,
            image_path,
        })
    } else {
        let bytes = read_body_bytes(payload).await?;
        let request: AdminProductCreateRequest = serde_json::from_slice(&bytes)
            .map_err(|e| ApiError::BadRequest(format!("Invalid JSON body: {e}")))?;
        Ok(ProductPayload {
            request,
            image_path: None,
        })
    }
}

/// Django parity: products with existing orders are soft-deactivated (FK
/// RESTRICT would block a hard delete); otherwise hard delete (variants
/// cascade) with a 204 + body.
pub async fn admin_products_delete(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    user: AuthUser,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let product_id = path.into_inner();

    let has_orders: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM orders WHERE product_id = $1)")
            .bind(product_id)
            .fetch_one(pool.get_ref())
            .await?;

    if has_orders {
        let result =
            sqlx::query("UPDATE products SET is_active = false, updated_at = now() WHERE id = $1")
                .bind(product_id)
                .execute(pool.get_ref())
                .await?;
        if result.rows_affected() == 0 {
            return Err(ApiError::NotFound("product not found".into()));
        }
        return Ok(HttpResponse::Ok().json(serde_json::json!({
            "detail": "Product has existing orders. It has been deactivated instead of deleted."
        })));
    }

    let result = sqlx::query("DELETE FROM products WHERE id = $1")
        .bind(product_id)
        .execute(pool.get_ref())
        .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("product not found".into()));
    }
    Ok(HttpResponse::build(actix_web::http::StatusCode::NO_CONTENT)
        .json(serde_json::json!({ "detail": "Product deleted successfully." })))
}

async fn load_admin_product(pool: &PgPool, product_id: Uuid) -> Result<AdminProductItem, ApiError> {
    let mut item: AdminProductItem =
        sqlx::query_as::<_, AdminProductItem>(&format!("{ADMIN_PRODUCT_SELECT} WHERE p.id = $1"))
            .bind(product_id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| ApiError::NotFound("product not found".into()))?;
    item.image_url = media_url(item.image_url.take());
    Ok(item)
}

/// Exposes stored image paths ("products/<file>") as absolute /media/ URLs,
/// matching Django's MEDIA_URL semantics.
fn media_url(image: Option<String>) -> Option<String> {
    image
        .filter(|s| !s.trim().is_empty())
        .map(|s| format!("/media/{}", s.trim()))
}

// --- variants ---------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct AdminVariantRequest {
    pub duration_months: Option<i32>,
    pub is_lifetime: Option<bool>,
    pub external_pack_id: Option<i32>,
    pub price_in_credits: Option<Decimal>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct AdminVariantItem {
    pub id: Uuid,
    pub product: Uuid,
    pub duration_months: Option<i32>,
    pub is_lifetime: bool,
    pub external_pack_id: Option<i32>,
    pub price_in_credits: Option<Decimal>,
    pub is_active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn variants_list(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    user: AuthUser,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let product_id = path.into_inner();
    let mut rows: Vec<AdminVariantItem> = sqlx::query_as(
        "SELECT id, product_id AS product, duration_months, is_lifetime, external_pack_id, \
         price_in_credits, is_active, created_at FROM product_variants \
         WHERE product_id = $1 ORDER BY duration_months NULLS FIRST",
    )
    .bind(product_id)
    .fetch_all(pool.get_ref())
    .await?;

    let items: Vec<serde_json::Value> = rows
        .drain(..)
        .map(|v| {
            let display = crate::models::duration_display(v.duration_months, v.is_lifetime);
            serde_json::json!({
                "id": v.id,
                "product": v.product,
                "duration_months": v.duration_months,
                "is_lifetime": v.is_lifetime,
                "external_pack_id": v.external_pack_id,
                "price_in_credits": v.price_in_credits.map(|d| d.to_string()),
                "is_active": v.is_active,
                "duration_display": display,
                "created_at": v.created_at,
            })
        })
        .collect();
    Ok(HttpResponse::Ok().json(items))
}

pub async fn variant_create(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    user: AuthUser,
    body: web::Json<AdminVariantRequest>,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let product_id = path.into_inner();

    let (is_manual, adapter_key): (bool, Option<String>) = sqlx::query_as(
        "SELECT p.is_manual, pr.adapter_key FROM products p LEFT JOIN providers pr ON pr.id = p.provider_id WHERE p.id = $1",
    )
    .bind(product_id)
    .fetch_optional(pool.get_ref())
    .await?
    .ok_or_else(|| ApiError::NotFound("product not found".into()))?;

    if !is_manual && adapter_key.as_deref() != Some("whatsapp") && body.external_pack_id.is_none() {
        return Err(ApiError::BadRequest(
            "external_pack_id is required for API products.".into(),
        ));
    }

    let row: (Uuid,) = sqlx::query_as(
        "INSERT INTO product_variants (product_id, duration_months, is_lifetime, external_pack_id, \
         price_in_credits, is_active) VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(product_id)
    .bind(body.duration_months)
    .bind(body.is_lifetime.unwrap_or(false))
    .bind(body.external_pack_id)
    .bind(body.price_in_credits.unwrap_or(Decimal::ZERO))
    .bind(body.is_active.unwrap_or(true))
    .fetch_one(pool.get_ref())
    .await?;

    Ok(HttpResponse::Created().json(serde_json::json!({ "id": row.0 })))
}

pub async fn variant_update(
    pool: web::Data<PgPool>,
    path: web::Path<(Uuid, Uuid)>,
    user: AuthUser,
    body: web::Json<AdminVariantRequest>,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let (product_id, variant_id) = path.into_inner();

    let mut qb: sqlx::QueryBuilder<sqlx::Postgres> =
        sqlx::QueryBuilder::new("UPDATE product_variants SET updated_at = now()");
    if let Some(duration) = body.duration_months {
        qb.push(", duration_months = ").push_bind(duration);
    }
    if let Some(lifetime) = body.is_lifetime {
        qb.push(", is_lifetime = ").push_bind(lifetime);
    }
    if let Some(pack) = body.external_pack_id {
        qb.push(", external_pack_id = ").push_bind(pack);
    }
    if let Some(price) = body.price_in_credits {
        qb.push(", price_in_credits = ").push_bind(price);
    }
    if let Some(is_active) = body.is_active {
        qb.push(", is_active = ").push_bind(is_active);
    }
    qb.push(" WHERE id = ")
        .push_bind(variant_id)
        .push(" AND product_id = ")
        .push_bind(product_id);
    let result = qb.build().execute(pool.get_ref()).await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("variant not found".into()));
    }
    Ok(HttpResponse::Ok().json(serde_json::json!({ "detail": "variant updated" })))
}

pub async fn variant_delete(
    pool: web::Data<PgPool>,
    path: web::Path<(Uuid, Uuid)>,
    user: AuthUser,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let (product_id, variant_id) = path.into_inner();
    let result = sqlx::query("DELETE FROM product_variants WHERE id = $1 AND product_id = $2")
        .bind(variant_id)
        .bind(product_id)
        .execute(pool.get_ref())
        .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("variant not found".into()));
    }
    Ok(HttpResponse::NoContent().finish())
}

// --- categories -------------------------------------------------------------

#[derive(Debug, Serialize, FromRow)]
pub struct AdminCategoryItem {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub image: Option<String>,
    pub is_active: bool,
    pub sort_order: i32,
    pub product_count: i64,
}

pub async fn admin_categories_list(
    pool: web::Data<PgPool>,
    user: AuthUser,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let mut rows: Vec<AdminCategoryItem> = sqlx::query_as(
        "SELECT c.id, c.name, c.slug, c.description, c.image, c.is_active, c.sort_order, \
         (SELECT count(*) FROM products p WHERE p.category_id = c.id)::bigint AS product_count \
         FROM categories c ORDER BY c.sort_order, c.name",
    )
    .fetch_all(pool.get_ref())
    .await?;
    for r in &mut rows {
        r.image = media_url(r.image.take());
    }
    Ok(HttpResponse::Ok().json(rows))
}

#[derive(Debug, Deserialize)]
pub struct AdminCategoryRequest {
    pub name: String,
    pub slug: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
    pub is_active: Option<bool>,
    pub sort_order: Option<i32>,
}

pub fn slugify(name: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in name.to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    slug.trim_matches('-').to_string()
}

struct CategoryPayload {
    request: AdminCategoryRequest,
    image_path: Option<String>,
}

async fn parse_category_payload(
    req: &HttpRequest,
    payload: web::Payload,
    media_dir: &str,
) -> Result<CategoryPayload, ApiError> {
    if is_multipart(req) {
        let (mut fields, image_path) =
            parse_multipart_form(req, &mut payload.into_inner(), media_dir, "categories").await?;
        let request = AdminCategoryRequest {
            name: fields.remove("name").unwrap_or_default(),
            slug: fields.remove("slug").filter(|s| !s.trim().is_empty()),
            description: fields
                .remove("description")
                .filter(|s| !s.trim().is_empty()),
            image: None,
            is_active: parse_opt_bool(&fields, "is_active")?,
            sort_order: parse_opt_i32(&fields, "sort_order")?,
        };
        Ok(CategoryPayload {
            request,
            image_path,
        })
    } else {
        let bytes = read_body_bytes(payload).await?;
        let request: AdminCategoryRequest = serde_json::from_slice(&bytes)
            .map_err(|e| ApiError::BadRequest(format!("Invalid JSON body: {e}")))?;
        Ok(CategoryPayload {
            request,
            image_path: None,
        })
    }
}

pub async fn admin_category_create(
    pool: web::Data<PgPool>,
    settings: web::Data<Settings>,
    user: AuthUser,
    req: HttpRequest,
    payload: web::Payload,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let body = parse_category_payload(&req, payload, &settings.media_dir).await?;
    let slug = body
        .request
        .slug
        .clone()
        .unwrap_or_else(|| slugify(&body.request.name));
    let image = body.image_path.or_else(|| body.request.image.clone());
    let row: AdminCategoryItem = sqlx::query_as(
        "INSERT INTO categories (name, slug, description, image, is_active, sort_order) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         RETURNING id, name, slug, description, image, is_active, sort_order, 0::bigint AS product_count",
    )
    .bind(&body.request.name)
    .bind(&slug)
    .bind(body.request.description.clone().unwrap_or_default())
    .bind(&image)
    .bind(body.request.is_active.unwrap_or(true))
    .bind(body.request.sort_order.unwrap_or(0))
    .fetch_one(pool.get_ref())
    .await?;
    let mut row = row;
    row.image = media_url(row.image.take());
    Ok(HttpResponse::Created().json(row))
}

pub async fn admin_category_update(
    pool: web::Data<PgPool>,
    settings: web::Data<Settings>,
    path: web::Path<Uuid>,
    user: AuthUser,
    req: HttpRequest,
    payload: web::Payload,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let category_id = path.into_inner();
    let body = parse_category_payload(&req, payload, &settings.media_dir).await?;
    let slug = body
        .request
        .slug
        .clone()
        .unwrap_or_else(|| slugify(&body.request.name));
    let image = body.image_path.or_else(|| body.request.image.clone());
    let result = sqlx::query(
        "UPDATE categories SET name = $1, slug = $2, description = $3, image = $4, \
         is_active = $5, sort_order = $6 WHERE id = $7",
    )
    .bind(&body.request.name)
    .bind(&slug)
    .bind(body.request.description.clone().unwrap_or_default())
    .bind(&image)
    .bind(body.request.is_active.unwrap_or(true))
    .bind(body.request.sort_order.unwrap_or(0))
    .bind(category_id)
    .execute(pool.get_ref())
    .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("category not found".into()));
    }
    Ok(HttpResponse::Ok().json(serde_json::json!({ "detail": "category updated" })))
}

pub async fn admin_category_delete(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    user: AuthUser,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let category_id = path.into_inner();
    let product_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM products WHERE category_id = $1")
            .bind(category_id)
            .fetch_one(pool.get_ref())
            .await?;
    if product_count > 0 {
        return Err(ApiError::BadRequest(format!(
            "Cannot delete category. It has {product_count} product(s) assigned."
        )));
    }
    let result = sqlx::query("DELETE FROM categories WHERE id = $1")
        .bind(category_id)
        .execute(pool.get_ref())
        .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("category not found".into()));
    }
    Ok(HttpResponse::NoContent().finish())
}

// --- providers --------------------------------------------------------------

#[derive(Debug, Serialize, FromRow)]
pub struct AdminProviderItem {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub adapter_key: String,
    pub is_active: bool,
}

pub async fn admin_providers_list(
    pool: web::Data<PgPool>,
    user: AuthUser,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let rows: Vec<AdminProviderItem> = sqlx::query_as(
        "SELECT id, name, slug, adapter_key, is_active FROM providers WHERE is_active = true ORDER BY name",
    )
    .fetch_all(pool.get_ref())
    .await?;
    Ok(HttpResponse::Ok().json(rows))
}

// ===========================================================================
// Dashboard parity part 3: manual products, credentials, whatsapp orders
// ===========================================================================

#[derive(Debug, Serialize, FromRow)]
pub struct ManualProductItem {
    pub id: Uuid,
    pub name: String,
    pub category: Option<Uuid>,
    pub category_name: Option<String>,
    pub credential_type: Option<String>,
    pub is_active: bool,
    pub image: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub total_credentials: i64,
    pub available_credentials: i64,
    pub used_credentials: i64,
}

const MANUAL_PRODUCT_SELECT: &str = "SELECT p.id, p.name, p.category_id AS category, \
     c.name AS category_name, p.credential_type, p.is_active, p.image, p.created_at, \
     (SELECT COUNT(*) FROM manual_credentials mc WHERE mc.product_id = p.id) AS total_credentials, \
     (SELECT COUNT(*) FROM manual_credentials mc WHERE mc.product_id = p.id \
      AND mc.status = 'available') AS available_credentials, \
     (SELECT COUNT(*) FROM manual_credentials mc WHERE mc.product_id = p.id \
      AND mc.status = 'used') AS used_credentials \
     FROM products p LEFT JOIN categories c ON c.id = p.category_id";

pub async fn manual_products_list(
    pool: web::Data<PgPool>,
    user: AuthUser,
    query: web::Query<ResellerQuery>,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let (page, page_size) = page_bounds(query.page, query.page_size);
    let search = query.search.clone().unwrap_or_default();

    let mut where_qb: sqlx::QueryBuilder<sqlx::Postgres> =
        sqlx::QueryBuilder::new("WHERE p.is_manual = TRUE");
    if !search.is_empty() {
        where_qb
            .push(" AND p.name ILIKE ")
            .push_bind(format!("%{search}%"));
    }
    let where_sql = where_qb.sql().to_string();

    let count: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM products p {where_sql}"))
        .fetch_one(pool.get_ref())
        .await?;

    let rows: Vec<ManualProductItem> = sqlx::query_as(&format!(
        "{MANUAL_PRODUCT_SELECT} {where_sql} ORDER BY p.name LIMIT $1 OFFSET $2"
    ))
    .bind(page_size)
    .bind((page - 1) * page_size)
    .fetch_all(pool.get_ref())
    .await?;

    let total_pages = (count + page_size - 1) / page_size;
    let next = if page < total_pages {
        page_url("/api/dashboard/manual-products", page, page_size)
    } else {
        serde_json::Value::Null
    };
    let previous = if page > 1 {
        page_url("/api/dashboard/manual-products", page - 2, page_size)
    } else {
        serde_json::Value::Null
    };

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "count": count,
        "total_pages": total_pages,
        "next": next,
        "previous": previous,
        "results": rows,
    })))
}

/// Load a full credential row with joined product name, assigned-to username
/// and variant duration (Django CredentialSerializer parity).
async fn load_manual_credential(
    pool: &PgPool,
    credential_id: Uuid,
) -> Result<serde_json::Value, ApiError> {
    let row = sqlx::query(
        "SELECT mc.id, mc.uuid, mc.product_id, p.name AS product_name, mc.credential_type, \
                mc.variant_id, pv.duration_months, pv.is_lifetime, \
                mc.username, mc.password, mc.code, mc.notes, mc.status, mc.assigned_to, \
                u.username AS assigned_to_username, mc.assigned_at, mc.used_at, mc.expires_at, \
                mc.created_at, mc.updated_at \
         FROM manual_credentials mc \
         JOIN products p ON p.id = mc.product_id \
         LEFT JOIN product_variants pv ON pv.id = mc.variant_id \
         LEFT JOIN users u ON u.id = mc.assigned_to \
         WHERE mc.id = $1",
    )
    .bind(credential_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ApiError::NotFound("credential not found".into()))?;

    Ok(credential_to_json(&row))
}

/// Django CredentialSerializer output for a manual_credentials row joined
/// with products (p), product_variants (pv) and users (u).
fn credential_to_json(r: &sqlx::postgres::PgRow) -> serde_json::Value {
    let product_name: String = r.try_get("product_name").unwrap_or_default();
    let months: Option<i32> = r.try_get("duration_months").unwrap_or(None);
    let lifetime: bool = r.try_get("is_lifetime").unwrap_or(false);
    let variant_id: Option<Uuid> = r.try_get("variant_id").unwrap_or(None);
    let variant_display = if variant_id.is_some() {
        Some(format!(
            "{product_name} - {}",
            crate::models::duration_display(months, lifetime)
        ))
    } else {
        None
    };
    serde_json::json!({
        "id": r.try_get::<Uuid, _>("id").unwrap_or(Uuid::nil()),
        "uuid": r.try_get::<Uuid, _>("uuid").unwrap_or(Uuid::nil()),
        "product": r.try_get::<Uuid, _>("product_id").unwrap_or(Uuid::nil()),
        "product_name": product_name,
        "credential_type": r.try_get::<String, _>("credential_type").unwrap_or_default(),
        "variant": variant_id,
        "variant_display": variant_display,
        "username": r.try_get::<String, _>("username").unwrap_or_default(),
        "password": r.try_get::<String, _>("password").unwrap_or_default(),
        "code": r.try_get::<String, _>("code").unwrap_or_default(),
        "notes": r.try_get::<String, _>("notes").unwrap_or_default(),
        "status": r.try_get::<String, _>("status").unwrap_or_default(),
        "assigned_to": r.try_get::<Option<Uuid>, _>("assigned_to").unwrap_or(None),
        "assigned_to_username": r.try_get::<Option<String>, _>("assigned_to_username").unwrap_or(None),
        "assigned_at": r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("assigned_at").unwrap_or(None),
        "used_at": r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("used_at").unwrap_or(None),
        "expires_at": r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("expires_at").unwrap_or(None),
        "created_at": r.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").unwrap_or(chrono::DateTime::UNIX_EPOCH),
        "updated_at": r.try_get::<chrono::DateTime<chrono::Utc>, _>("updated_at").unwrap_or(chrono::DateTime::UNIX_EPOCH),
    })
}

/// Appends optional status/search predicates to a query builder that
/// references `mc` (manual_credentials) and `u` (users, left-joined).
fn push_cred_filter<'a>(
    qb: &mut sqlx::QueryBuilder<'a, sqlx::Postgres>,
    status: &'a str,
    search: &'a str,
) {
    if !status.is_empty() {
        qb.push(" AND mc.status = ").push_bind(status);
    }
    if !search.is_empty() {
        qb.push(" AND (mc.username ILIKE ")
            .push_bind(format!("%{search}%"));
        qb.push(" OR mc.code ILIKE ")
            .push_bind(format!("%{search}%"));
        qb.push(" OR u.username ILIKE ")
            .push_bind(format!("%{search}%"))
            .push(")");
    }
}

pub async fn manual_product_detail(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    user: AuthUser,
    query: web::Query<ResellerQuery>,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let product_id = path.into_inner();

    let product: Option<(Uuid, String, Option<String>, bool)> = sqlx::query_as(
        "SELECT id, name, credential_type, is_active FROM products WHERE id = $1 AND is_manual = TRUE",
    )
    .bind(product_id)
    .fetch_optional(pool.get_ref())
    .await?;
    let (pid, pname, ptype, pactive) =
        product.ok_or_else(|| ApiError::NotFound("manual product not found".into()))?;

    let (page, page_size) = page_bounds(query.page, query.page_size);
    let status = query.status.clone().unwrap_or_default();
    let search = query.search.clone().unwrap_or_default();

    let mut count_qb: sqlx::QueryBuilder<sqlx::Postgres> = sqlx::QueryBuilder::new(
        "SELECT count(*) FROM manual_credentials mc LEFT JOIN users u ON u.id = mc.assigned_to \
         WHERE mc.product_id = ",
    );
    count_qb.push_bind(pid);
    push_cred_filter(&mut count_qb, &status, &search);
    let total: i64 = count_qb
        .build_query_scalar()
        .fetch_one(pool.get_ref())
        .await?;

    let available: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM manual_credentials WHERE product_id = $1 AND status = 'available'",
    )
    .bind(pid)
    .fetch_one(pool.get_ref())
    .await?;
    let used: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM manual_credentials WHERE product_id = $1 AND status = 'used'",
    )
    .bind(pid)
    .fetch_one(pool.get_ref())
    .await?;

    let variants: Vec<serde_json::Value> = sqlx::query(
        "SELECT id, duration_months, is_lifetime, price_in_credits FROM product_variants \
         WHERE product_id = $1 ORDER BY duration_months NULLS FIRST",
    )
    .bind(pid)
    .fetch_all(pool.get_ref())
    .await?
    .into_iter()
    .map(|r| {
        let months: Option<i32> = r.try_get("duration_months").unwrap_or(None);
        let lifetime: bool = r.try_get("is_lifetime").unwrap_or(false);
        let price: Option<Decimal> = r.try_get("price_in_credits").unwrap_or(None);
        serde_json::json!({
            "id": r.try_get::<Uuid, _>("id").unwrap_or(Uuid::nil()),
            "duration_months": months,
            "is_lifetime": lifetime,
            "price_in_credits": price.map(|d| d.to_string()),
        })
    })
    .collect();

    let mut creds_qb: sqlx::QueryBuilder<sqlx::Postgres> = sqlx::QueryBuilder::new(
        "SELECT mc.id, mc.uuid, mc.product_id, p.name AS product_name, mc.credential_type, \
                mc.variant_id, pv.duration_months, pv.is_lifetime, \
                mc.username, mc.password, mc.code, mc.notes, mc.status, mc.assigned_to, \
                u.username AS assigned_to_username, mc.assigned_at, mc.used_at, mc.expires_at, \
                mc.created_at, mc.updated_at \
         FROM manual_credentials mc \
         JOIN products p ON p.id = mc.product_id \
         LEFT JOIN product_variants pv ON pv.id = mc.variant_id \
         LEFT JOIN users u ON u.id = mc.assigned_to \
         WHERE mc.product_id = ",
    );
    creds_qb.push_bind(pid);
    push_cred_filter(&mut creds_qb, &status, &search);
    creds_qb
        .push(" ORDER BY mc.created_at DESC LIMIT ")
        .push_bind(page_size)
        .push(" OFFSET ")
        .push_bind((page - 1) * page_size);
    let cred_rows = creds_qb.build().fetch_all(pool.get_ref()).await?;

    let results: Vec<serde_json::Value> = cred_rows.iter().map(credential_to_json).collect();

    let total_pages = (total + page_size - 1) / page_size;
    let next = if page < total_pages {
        page_url("/api/dashboard/manual-products", page, page_size)
    } else {
        serde_json::Value::Null
    };
    let previous = if page > 1 {
        page_url("/api/dashboard/manual-products", page - 2, page_size)
    } else {
        serde_json::Value::Null
    };

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "count": total,
        "total_pages": total_pages,
        "next": next,
        "previous": previous,
        "results": results,
        "product": {
            "id": pid,
            "name": pname,
            "credential_type": ptype,
            "is_active": pactive,
            "variants": variants,
        },
        "stats": { "total": total, "available": available, "used": used },
    })))
}

#[derive(Debug, Deserialize)]
pub struct CredentialCreateRequest {
    pub username: Option<String>,
    pub password: Option<String>,
    pub code: Option<String>,
    pub notes: Option<String>,
    pub expires_at: Option<Option<chrono::DateTime<chrono::Utc>>>,
    pub variant_id: Option<Option<Uuid>>,
}

/// POST /api/dashboard/manual-products/{pk}/credentials/ (Django
/// CredentialCreateView): create a single credential for a manual product.
pub async fn credential_create(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    user: AuthUser,
    body: web::Json<CredentialCreateRequest>,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let product_id = path.into_inner();

    let product: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT id, credential_type FROM products WHERE id = $1 AND is_manual = TRUE",
    )
    .bind(product_id)
    .fetch_optional(pool.get_ref())
    .await?;
    let (pid, credential_type) =
        product.ok_or_else(|| ApiError::NotFound("manual product not found".into()))?;

    validate_credential_fields(&credential_type, &body)?;

    let variant_id: Option<Uuid> = body.variant_id.unwrap_or(None);
    if let Some(vid) = variant_id {
        let belongs: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM product_variants WHERE id = $1 AND product_id = $2)",
        )
        .bind(vid)
        .bind(pid)
        .fetch_one(pool.get_ref())
        .await?;
        if !belongs {
            return Err(ApiError::BadRequest(
                "Variant does not belong to this product.".into(),
            ));
        }
    }

    let username = body.username.clone().unwrap_or_default();
    let password = body.password.clone().unwrap_or_default();
    let code = body.code.clone().unwrap_or_default();
    let notes = body.notes.clone().unwrap_or_default();
    let expires_at: Option<chrono::DateTime<chrono::Utc>> = body.expires_at.unwrap_or(None);

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO manual_credentials \
         (product_id, variant_id, credential_type, username, password, code, notes, \
          status, expires_at, created_by) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, 'available', $8, $9) \
         RETURNING id",
    )
    .bind(pid)
    .bind(variant_id)
    .bind(&credential_type)
    .bind(&username)
    .bind(&password)
    .bind(&code)
    .bind(&notes)
    .bind(expires_at)
    .bind(user.0.id)
    .fetch_one(pool.get_ref())
    .await?;

    let item = load_manual_credential(pool.get_ref(), id).await?;
    Ok(HttpResponse::Created().json(item))
}

fn validate_credential_fields(
    credential_type: &str,
    body: &CredentialCreateRequest,
) -> Result<(), ApiError> {
    if credential_type == "username_password" {
        let username = body.username.clone().unwrap_or_default();
        let password = body.password.clone().unwrap_or_default();
        if username.is_empty() || password.is_empty() {
            return Err(ApiError::BadRequest(
                "Username and password are required for this product.".into(),
            ));
        }
    } else if credential_type == "single_code" {
        let code = body.code.clone().unwrap_or_default();
        if code.is_empty() {
            return Err(ApiError::BadRequest(
                "Activation code is required for this product.".into(),
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct CredentialBulkRequest {
    pub credentials: Vec<CredentialCreateRequest>,
}

/// POST /api/dashboard/manual-products/{pk}/credentials/bulk/ (Django
/// CredentialBulkCreateView): 1-100 credentials, all-or-nothing validation.
pub async fn credential_bulk_create(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    user: AuthUser,
    body: web::Json<CredentialBulkRequest>,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let product_id = path.into_inner();

    if body.credentials.is_empty() {
        return Err(ApiError::BadRequest(
            "At least one credential is required.".into(),
        ));
    }
    if body.credentials.len() > 100 {
        return Err(ApiError::BadRequest(
            "Maximum 100 credentials per bulk operation.".into(),
        ));
    }

    let product: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT id, credential_type FROM products WHERE id = $1 AND is_manual = TRUE",
    )
    .bind(product_id)
    .fetch_optional(pool.get_ref())
    .await?;
    let (pid, credential_type) =
        product.ok_or_else(|| ApiError::NotFound("manual product not found".into()))?;

    for item in &body.credentials {
        if let Err(e) = validate_credential_fields(&credential_type, item) {
            let detail = match e {
                ApiError::BadRequest(v) => v,
                _ => "invalid credential data".to_string(),
            };
            return Err(ApiError::BadRequest(format!(
                "Invalid credential data: {detail}"
            )));
        }
    }

    let mut created: Vec<serde_json::Value> = Vec::with_capacity(body.credentials.len());
    for item in &body.credentials {
        let variant_id: Option<Uuid> = item.variant_id.unwrap_or(None);
        let username = item.username.clone().unwrap_or_default();
        let password = item.password.clone().unwrap_or_default();
        let code = item.code.clone().unwrap_or_default();
        let notes = item.notes.clone().unwrap_or_default();
        let expires_at: Option<chrono::DateTime<chrono::Utc>> = item.expires_at.unwrap_or(None);

        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO manual_credentials \
             (product_id, variant_id, credential_type, username, password, code, notes, \
              status, expires_at, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, 'available', $8, $9) \
             RETURNING id",
        )
        .bind(pid)
        .bind(variant_id)
        .bind(&credential_type)
        .bind(&username)
        .bind(&password)
        .bind(&code)
        .bind(&notes)
        .bind(expires_at)
        .bind(user.0.id)
        .fetch_one(pool.get_ref())
        .await?;

        created.push(load_manual_credential(pool.get_ref(), id).await?);
    }

    Ok(HttpResponse::Created().json(serde_json::json!({
        "count": created.len(),
        "credentials": created,
    })))
}

#[derive(Debug, Deserialize)]
pub struct CredentialUpdateRequest {
    pub username: Option<String>,
    pub password: Option<String>,
    pub code: Option<String>,
    pub notes: Option<String>,
    pub status: Option<String>,
    pub expires_at: Option<Option<chrono::DateTime<chrono::Utc>>>,
    pub variant_id: Option<Option<Uuid>>,
}

/// PUT /api/dashboard/credentials/{pk}/ (Django CredentialDetailView):
/// partial update, keys present in the body are applied.
pub async fn credential_update(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    user: AuthUser,
    body: web::Json<CredentialUpdateRequest>,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let credential_id = path.into_inner();

    let mut qb: sqlx::QueryBuilder<sqlx::Postgres> =
        sqlx::QueryBuilder::new("UPDATE manual_credentials SET updated_at = now()");
    if let Some(username) = &body.username {
        qb.push(", username = ").push_bind(username);
    }
    if let Some(password) = &body.password {
        qb.push(", password = ").push_bind(password);
    }
    if let Some(code) = &body.code {
        qb.push(", code = ").push_bind(code);
    }
    if let Some(notes) = &body.notes {
        qb.push(", notes = ").push_bind(notes);
    }
    if let Some(status) = &body.status {
        qb.push(", status = ").push_bind(status);
    }
    if let Some(expires_at) = body.expires_at {
        qb.push(", expires_at = ").push_bind(expires_at);
    }
    if let Some(variant_id) = body.variant_id {
        match variant_id {
            Some(vid) => {
                let belongs: bool = sqlx::query_scalar(
                    "SELECT EXISTS ( \
                        SELECT 1 FROM manual_credentials mc \
                        JOIN product_variants pv ON pv.id = $1 AND pv.product_id = mc.product_id \
                        WHERE mc.id = $2 \
                     )",
                )
                .bind(vid)
                .bind(credential_id)
                .fetch_one(pool.get_ref())
                .await?;
                if !belongs {
                    return Err(ApiError::NotFound("variant not found".into()));
                }
                qb.push(", variant_id = ").push_bind(vid);
            }
            None => {
                qb.push(", variant_id = NULL");
            }
        }
    }
    qb.push(" WHERE id = ").push_bind(credential_id);
    qb.build().execute(pool.get_ref()).await?;

    let item = load_manual_credential(pool.get_ref(), credential_id).await?;
    Ok(HttpResponse::Ok().json(item))
}

/// DELETE /api/dashboard/credentials/{pk}/ â†’ 204.
pub async fn credential_delete(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    user: AuthUser,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let credential_id = path.into_inner();

    let deleted = sqlx::query("DELETE FROM manual_credentials WHERE id = $1")
        .bind(credential_id)
        .execute(pool.get_ref())
        .await?;
    if deleted.rows_affected() == 0 {
        return Err(ApiError::NotFound("credential not found".into()));
    }
    Ok(HttpResponse::NoContent().finish())
}

#[derive(Debug, Serialize, FromRow)]
pub struct WhatsAppOrderItem {
    pub id: Uuid,
    pub uuid: Uuid,
    pub reseller: Uuid,
    pub reseller_username: String,
    pub product: Uuid,
    pub product_name: String,
    pub duration_months: Option<i32>,
    pub is_lifetime: Option<bool>,
    pub quantity: i32,
    pub total_credits: Decimal,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub wa_link: Option<String>,
    pub message_text: Option<String>,
}

/// GET /api/dashboard/whatsapp-orders/ (Django WhatsAppOrdersView): pending
/// orders fulfilled through the whatsapp adapter.
pub async fn whatsapp_orders_list(
    pool: web::Data<PgPool>,
    user: AuthUser,
    query: web::Query<ResellerQuery>,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let (page, page_size) = page_bounds(query.page, query.page_size);

    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM orders o \
         JOIN products p ON p.id = o.product_id \
         JOIN providers pr ON pr.id = p.provider_id \
         WHERE pr.adapter_key = 'whatsapp' AND o.status = 'PENDING'",
    )
    .fetch_one(pool.get_ref())
    .await?;

    let rows: Vec<WhatsAppOrderItem> = sqlx::query_as(
        "SELECT o.id, o.uuid, o.reseller_id AS reseller, u.username AS reseller_username, \
                o.product_id AS product, p.name AS product_name, \
                o.quantity, o.total_credits, o.status, o.created_at, \
                pv.duration_months, pv.is_lifetime, \
                (SELECT cr.data->>'wa_link' FROM credentials cr \
                 WHERE cr.order_id = o.id ORDER BY cr.created_at LIMIT 1) AS wa_link, \
                (SELECT cr.data->>'message' FROM credentials cr \
                 WHERE cr.order_id = o.id ORDER BY cr.created_at LIMIT 1) AS message_text \
         FROM orders o \
         JOIN users u ON u.id = o.reseller_id \
         JOIN products p ON p.id = o.product_id \
         JOIN providers pr ON pr.id = p.provider_id \
         LEFT JOIN product_variants pv ON pv.id = o.variant_id \
         WHERE pr.adapter_key = 'whatsapp' AND o.status = 'PENDING' \
         ORDER BY o.created_at DESC LIMIT $1 OFFSET $2",
    )
    .bind(page_size)
    .bind((page - 1) * page_size)
    .fetch_all(pool.get_ref())
    .await?;

    let results: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|o| {
            let duration_display = o
                .duration_months
                .map(|m| crate::models::duration_display(Some(m), o.is_lifetime.unwrap_or(false)))
                .unwrap_or_else(|| "â€”".to_string());
            serde_json::json!({
                "id": o.id,
                "uuid": o.uuid,
                "reseller": o.reseller,
                "reseller_username": o.reseller_username,
                "product": o.product,
                "product_name": o.product_name,
                "duration_display": duration_display,
                "quantity": o.quantity,
                "total_credits": o.total_credits.to_string(),
                "status": o.status,
                "created_at": o.created_at,
                "wa_link": o.wa_link,
                "message_text": o.message_text,
            })
        })
        .collect();

    let total_pages = (count + page_size - 1) / page_size;
    let next = if page < total_pages {
        page_url("/api/dashboard/whatsapp-orders", page, page_size)
    } else {
        serde_json::Value::Null
    };
    let previous = if page > 1 {
        page_url("/api/dashboard/whatsapp-orders", page - 2, page_size)
    } else {
        serde_json::Value::Null
    };

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "count": count,
        "total_pages": total_pages,
        "next": next,
        "previous": previous,
        "results": results,
    })))
}

/// POST /api/dashboard/whatsapp-orders/{uuid}/complete/ (Django
/// CompleteWhatsAppOrderView): mark a pending whatsapp order completed.
pub async fn whatsapp_order_complete(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    user: AuthUser,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let order_uuid = path.into_inner();

    let updated = sqlx::query(
        "UPDATE orders o SET status = 'COMPLETED' \
         FROM products p JOIN providers pr ON pr.id = p.provider_id \
         WHERE o.product_id = p.id AND pr.adapter_key = 'whatsapp' \
           AND o.status = 'PENDING' AND o.uuid = $1",
    )
    .bind(order_uuid)
    .execute(pool.get_ref())
    .await?;
    if updated.rows_affected() == 0 {
        return Err(ApiError::NotFound("order not found".into()));
    }
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "detail": "Order marked as completed."
    })))
}

/// GET /api/dashboard/stats/ (Django DashboardStatsView): aggregate overview
/// numbers. Flat object, no envelope.
pub async fn dashboard_stats(
    pool: web::Data<PgPool>,
    user: AuthUser,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let pool = pool.get_ref();

    let resellers: (i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE role = 'RESELLER') AS total, \
                count(*) FILTER (WHERE role = 'RESELLER' AND is_active) AS active \
         FROM users",
    )
    .fetch_one(pool)
    .await?;

    let (order_total, order_completed, revenue): (i64, i64, Option<Decimal>) = sqlx::query_as(
        "SELECT count(*) AS total, \
                count(*) FILTER (WHERE status = 'COMPLETED') AS completed, \
                SUM(total_credits) FILTER (WHERE status = 'COMPLETED') AS revenue \
         FROM orders",
    )
    .fetch_one(pool)
    .await?;

    let (cred_total, cred_available): (i64, i64) = sqlx::query_as(
        "SELECT count(*) AS total, \
                count(*) FILTER (WHERE status = 'available') AS available \
         FROM manual_credentials",
    )
    .fetch_one(pool)
    .await?;

    let revenue_str = format!("{:.2}", revenue.unwrap_or(Decimal::ZERO));

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "total_resellers": resellers.0,
        "active_resellers": resellers.1,
        "total_orders": order_total,
        "completed_orders": order_completed,
        "total_revenue": revenue_str,
        "total_credentials": cred_total,
        "available_credentials": cred_available,
    })))
}

#[derive(Debug, Deserialize)]
pub struct LimitQuery {
    pub limit: Option<i64>,
}

/// GET /api/dashboard/top-resellers/?limit= (Django TopResellersView):
/// bare array of resellers ranked by completed-order revenue.
pub async fn top_resellers(
    pool: web::Data<PgPool>,
    query: web::Query<LimitQuery>,
    user: AuthUser,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let limit = query.limit.unwrap_or(10);

    #[derive(FromRow)]
    struct TopResellerRow {
        id: Uuid,
        username: String,
        credit_balance: Option<Decimal>,
        is_active: bool,
        order_count: i64,
        total_revenue: Option<Decimal>,
    }

    let rows: Vec<TopResellerRow> = sqlx::query_as(
        "SELECT u.id, u.username, u.credit_balance, u.is_active, \
                count(o.id) FILTER (WHERE o.status = 'COMPLETED') AS order_count, \
                SUM(o.total_credits) FILTER (WHERE o.status = 'COMPLETED') AS total_revenue \
         FROM users u LEFT JOIN orders o ON o.reseller_id = u.id \
         WHERE u.role = 'RESELLER' \
         GROUP BY u.id, u.username, u.credit_balance, u.is_active \
         ORDER BY total_revenue DESC, u.username \
         LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool.get_ref())
    .await?;

    let items: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "username": r.username,
                "credit_balance": format!("{:.2}", r.credit_balance.unwrap_or(Decimal::ZERO)),
                "is_active": r.is_active,
                "order_count": r.order_count,
                "total_revenue": format!("{:.2}", r.total_revenue.unwrap_or(Decimal::ZERO)),
            })
        })
        .collect();
    Ok(HttpResponse::Ok().json(items))
}

/// GET /api/dashboard/recent-activity/?limit= (Django RecentActivityView):
/// merged, globally date-descending list of order + admin credit events.
pub async fn recent_activity(
    pool: web::Data<PgPool>,
    query: web::Query<LimitQuery>,
    user: AuthUser,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let limit = query.limit.unwrap_or(20);
    let pool = pool.get_ref();

    #[derive(FromRow)]
    struct OrderEvent {
        uuid: Uuid,
        product_name_at_purchase: String,
        total_credits: Decimal,
        status: String,
        created_at: chrono::DateTime<chrono::Utc>,
        username: String,
    }

    #[derive(FromRow)]
    struct CreditEvent {
        delta: Decimal,
        reason: String,
        created_at: chrono::DateTime<chrono::Utc>,
        username: String,
    }

    let order_events: Vec<OrderEvent> = sqlx::query_as(
        "SELECT o.uuid, o.product_name_at_purchase, o.total_credits, o.status, \
                o.created_at, u.username \
         FROM orders o JOIN users u ON u.id = o.reseller_id \
         ORDER BY o.created_at DESC LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    let credit_events: Vec<CreditEvent> = sqlx::query_as(
        "SELECT ct.delta, ct.reason, ct.created_at, u.username \
         FROM credit_transactions ct JOIN users u ON u.id = ct.reseller_id \
         WHERE ct.actor = 'ADMIN' \
         ORDER BY ct.created_at DESC LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    let mut activities: Vec<(chrono::DateTime<chrono::Utc>, serde_json::Value)> =
        Vec::with_capacity(order_events.len() + credit_events.len());
    for o in order_events {
        let uuid_short = o.uuid.to_string();
        activities.push((
            o.created_at,
            serde_json::json!({
                "type": "order",
                "description": format!(
                    "Order #{} \u{2014} {}",
                    &uuid_short[..8],
                    o.product_name_at_purchase
                ),
                "amount": format!("{:.2}", o.total_credits),
                "user": o.username,
                "status": o.status,
                "created_at": o.created_at,
            }),
        ));
    }
    for c in credit_events {
        activities.push((
            c.created_at,
            serde_json::json!({
                "type": "credit",
                "description": c.reason,
                "amount": format!("{:.2}", c.delta),
                "user": c.username,
                "status": serde_json::Value::Null,
                "created_at": c.created_at,
            }),
        ));
    }

    activities.sort_by_key(|a| std::cmp::Reverse(a.0));
    let items: Vec<serde_json::Value> = activities
        .into_iter()
        .take(limit as usize)
        .map(|(_, v)| v)
        .collect();
    Ok(HttpResponse::Ok().json(items))
}

/// GET /api/dashboard/provider-health/ (Django ProviderHealthView):
/// bare array with 24h order counts and error rates per active provider.
pub async fn provider_health(
    pool: web::Data<PgPool>,
    user: AuthUser,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;

    #[derive(FromRow)]
    struct ProviderHealthRow {
        id: Uuid,
        name: String,
        adapter_key: String,
        is_active: bool,
        orders_24h: i64,
        failed_24h: i64,
    }

    let rows: Vec<ProviderHealthRow> = sqlx::query_as(
        "SELECT pr.id, pr.name, pr.adapter_key, pr.is_active, \
                count(DISTINCT o.id) FILTER \
                    (WHERE o.created_at >= now() - interval '24 hours') AS orders_24h, \
                count(DISTINCT o.id) FILTER \
                    (WHERE o.status = 'FAILED' AND o.created_at >= now() - interval '24 hours') \
                    AS failed_24h \
         FROM providers pr \
         LEFT JOIN products p ON p.provider_id = pr.id \
         LEFT JOIN orders o ON o.product_id = p.id \
         WHERE pr.is_active \
         GROUP BY pr.id, pr.name, pr.adapter_key, pr.is_active \
         ORDER BY pr.name",
    )
    .fetch_all(pool.get_ref())
    .await?;

    let items: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            let error_rate = if r.orders_24h > 0 {
                (r.failed_24h as f64 / r.orders_24h as f64 * 100.0 * 10.0).round() / 10.0
            } else {
                0.0
            };
            serde_json::json!({
                "id": r.id,
                "name": r.name,
                "adapter_key": r.adapter_key,
                "is_active": r.is_active,
                "orders_24h": r.orders_24h,
                "failed_24h": r.failed_24h,
                "error_rate": error_rate,
            })
        })
        .collect();
    Ok(HttpResponse::Ok().json(items))
}
