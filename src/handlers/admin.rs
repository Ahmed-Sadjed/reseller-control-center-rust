use actix_web::{web, HttpResponse};
use rust_decimal::Decimal;
use serde::Deserialize;
use sqlx::{PgPool, QueryBuilder};
use uuid::Uuid;

use crate::{config::Settings, error::ApiError, middleware::AuthUser, models::User, providers};

fn require_admin(user: &User) -> Result<(), ApiError> {
    if user.role != "ADMIN" {
        return Err(ApiError::Forbidden);
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct CreateProductRequest {
    pub name: String,
    pub category_id: Option<Uuid>,
    pub provider_id: Option<Uuid>,
    pub description: Option<String>,
    pub external_pack_id: Option<i32>,
    pub duration_months: Option<i32>,
    pub price_in_credits: Option<Decimal>,
    pub image: Option<String>,
    pub is_manual: Option<bool>,
    pub credential_type: Option<String>,
    pub variants: Option<Vec<NewVariantRequest>>,
}

#[derive(Debug, Deserialize)]
pub struct NewVariantRequest {
    pub duration_months: Option<i32>,
    pub is_lifetime: bool,
    pub external_pack_id: Option<i32>,
    pub price_in_credits: Decimal,
}

const PRODUCT_COLUMNS: &str = "id, name, category_id, provider_id, description, external_pack_id, \
     duration_months, price_in_credits, image, is_active, is_manual, credential_type, created_at, updated_at";

pub async fn create_product(
    pool: web::Data<PgPool>,
    user: AuthUser,
    body: web::Json<CreateProductRequest>,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;

    let mut tx = pool.begin().await?;
    let product = sqlx::query_as::<_, crate::models::Product>(&format!(
        "INSERT INTO products (name, category_id, provider_id, description, external_pack_id, \
         duration_months, price_in_credits, image, is_manual, credential_type) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) \
         RETURNING {PRODUCT_COLUMNS}"
    ))
    .bind(&body.name)
    .bind(body.category_id)
    .bind(body.provider_id)
    .bind(body.description.clone().unwrap_or_default())
    .bind(body.external_pack_id)
    .bind(body.duration_months)
    .bind(body.price_in_credits)
    .bind(&body.image)
    .bind(body.is_manual.unwrap_or(false))
    .bind(&body.credential_type)
    .fetch_one(&mut *tx)
    .await?;

    if let Some(variants) = &body.variants {
        for v in variants {
            sqlx::query(
                "INSERT INTO product_variants (product_id, duration_months, is_lifetime, \
                 external_pack_id, price_in_credits) VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(product.id)
            .bind(v.duration_months)
            .bind(v.is_lifetime)
            .bind(v.external_pack_id)
            .bind(v.price_in_credits)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;
    Ok(HttpResponse::Created().json(product))
}

#[derive(Debug, Deserialize)]
pub struct UpdateProductRequest {
    pub name: Option<String>,
    pub category_id: Option<Uuid>,
    pub provider_id: Option<Uuid>,
    pub description: Option<String>,
    pub external_pack_id: Option<i32>,
    pub duration_months: Option<i32>,
    pub price_in_credits: Option<Decimal>,
    pub image: Option<String>,
    pub is_active: Option<bool>,
    pub is_manual: Option<bool>,
    pub credential_type: Option<String>,
}

pub async fn update_product(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    user: AuthUser,
    body: web::Json<UpdateProductRequest>,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let product_id = path.into_inner();

    let mut qb: QueryBuilder<sqlx::Postgres> =
        QueryBuilder::new("UPDATE products SET updated_at = now()");
    if let Some(name) = &body.name {
        qb.push(", name = ").push_bind(name);
    }
    if let Some(category_id) = body.category_id {
        qb.push(", category_id = ").push_bind(category_id);
    }
    if let Some(provider_id) = body.provider_id {
        qb.push(", provider_id = ").push_bind(provider_id);
    }
    if let Some(description) = &body.description {
        qb.push(", description = ").push_bind(description);
    }
    if let Some(external_pack_id) = body.external_pack_id {
        qb.push(", external_pack_id = ").push_bind(external_pack_id);
    }
    if let Some(duration_months) = body.duration_months {
        qb.push(", duration_months = ").push_bind(duration_months);
    }
    if let Some(price_in_credits) = body.price_in_credits {
        qb.push(", price_in_credits = ").push_bind(price_in_credits);
    }
    if let Some(image) = &body.image {
        qb.push(", image = ").push_bind(image);
    }
    if let Some(is_active) = body.is_active {
        qb.push(", is_active = ").push_bind(is_active);
    }
    if let Some(is_manual) = body.is_manual {
        qb.push(", is_manual = ").push_bind(is_manual);
    }
    if let Some(credential_type) = &body.credential_type {
        qb.push(", credential_type = ").push_bind(credential_type);
    }
    qb.push(" WHERE id = ")
        .push_bind(product_id)
        .push(" RETURNING ");
    qb.push(PRODUCT_COLUMNS);

    let product = qb
        .build_query_as::<crate::models::Product>()
        .fetch_optional(pool.get_ref())
        .await?
        .ok_or(ApiError::NotFound("product not found".into()))?;

    Ok(HttpResponse::Ok().json(product))
}

/// Soft delete: is_active = false (mirrors Django admin deactivation).
pub async fn delete_product(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    user: AuthUser,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let product_id = path.into_inner();

    let result =
        sqlx::query("UPDATE products SET is_active = false, updated_at = now() WHERE id = $1")
            .bind(product_id)
            .execute(pool.get_ref())
            .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("product not found".into()));
    }
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "detail": "product deactivated",
    })))
}

#[derive(Debug, Deserialize)]
pub struct SyncProvidersRequest {
    pub provider_id: Option<Uuid>,
}

type ProviderRow = (Uuid, String, String, Option<String>, Option<Vec<u8>>);

pub async fn sync_providers(
    pool: web::Data<PgPool>,
    settings: web::Data<Settings>,
    user: AuthUser,
    body: web::Json<SyncProvidersRequest>,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;

    let mut qb: QueryBuilder<sqlx::Postgres> =
        QueryBuilder::new("SELECT id, slug, adapter_key, api_endpoint, api_token FROM providers");
    if let Some(provider_id) = body.provider_id {
        qb.push(" WHERE id = ").push_bind(provider_id);
    } else {
        qb.push(" WHERE is_active = true");
    }
    qb.push(" ORDER BY name");

    let providers_rows: Vec<ProviderRow> = qb.build_query_as().fetch_all(pool.get_ref()).await?;

    if providers_rows.is_empty() {
        return Err(ApiError::NotFound("no providers configured".into()));
    }

    // Case-insensitive category name -> id lookup, created on demand.
    let mut category_map: std::collections::HashMap<String, Uuid> =
        sqlx::query_as("SELECT lower(name), id FROM categories")
            .fetch_all(pool.get_ref())
            .await?
            .into_iter()
            .collect();

    let mut results = Vec::new();
    for row in providers_rows {
        let slug = row.1.clone();
        match sync_one_provider(pool.get_ref(), &mut category_map, &settings, row).await {
            Ok(json) => results.push(json),
            Err(err) => results.push(serde_json::json!({
                "provider": slug,
                "error": err.to_string(),
            })),
        }
    }

    Ok(HttpResponse::Ok().json(results))
}

async fn sync_one_provider(
    pool: &PgPool,
    category_map: &mut std::collections::HashMap<String, Uuid>,
    settings: &Settings,
    row: ProviderRow,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let (provider_id, slug, adapter_key, endpoint, api_token) = row;
    let adapter = providers::get_provider(
        &adapter_key,
        endpoint.as_deref(),
        api_token
            .as_deref()
            .and_then(|t| {
                crate::utils::crypto::decrypt_api_token(
                    t,
                    settings.master_encryption_key.as_bytes(),
                )
            })
            .as_deref(),
        settings,
    )?;

    let catalog = adapter.fetch_catalog().await?;

    for item in &catalog {
        if let Some(category) = &item.category {
            let key = category.to_lowercase();
            if let std::collections::hash_map::Entry::Vacant(entry) = category_map.entry(key) {
                sqlx::query(
                        "INSERT INTO categories (name, slug) VALUES ($1, $1) ON CONFLICT (slug) DO NOTHING",
                    )
                    .bind(category)
                    .execute(pool)
                    .await?;
                let cat_id: Uuid =
                    sqlx::query_scalar("SELECT id FROM categories WHERE lower(name) = $1")
                        .bind(entry.key())
                        .fetch_one(pool)
                        .await?;
                entry.insert(cat_id);
            }
        }
    }

    if catalog.is_empty() {
        return Ok(serde_json::json!({
            "provider": slug,
            "products_upserted": 0,
            "variants_upserted": 0,
        }));
    }

    // Bulk upsert products, keyed on (provider_id, external_pack_id).
    let mut product_qb: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(
        "INSERT INTO products (name, category_id, provider_id, external_pack_id, duration_months, \
             price_in_credits, is_active, is_manual) ",
    );
    product_qb.push_values(&catalog, |mut b, item| {
        let category_id = item
            .category
            .as_deref()
            .and_then(|c| category_map.get(&c.to_lowercase()).copied());
        b.push_bind(&item.name)
            .push_bind(category_id)
            .push_bind(provider_id)
            .push_bind(item.external_pack_id.parse::<i32>().ok())
            .push_bind(item.duration_months)
            .push_bind(item.price)
            .push_bind(true)
            .push_bind(false);
    });
    product_qb.push(
            " ON CONFLICT (provider_id, external_pack_id) WHERE external_pack_id IS NOT NULL DO UPDATE SET \
             name = EXCLUDED.name, category_id = EXCLUDED.category_id, \
             duration_months = EXCLUDED.duration_months, \
             price_in_credits = EXCLUDED.price_in_credits, is_active = true, updated_at = now() \
             RETURNING id, external_pack_id",
        );

    let upserted: Vec<(Uuid, Option<i32>)> = product_qb.build_query_as().fetch_all(pool).await?;
    let product_ids: std::collections::HashMap<i32, Uuid> = upserted
        .iter()
        .filter_map(|(id, pack)| pack.map(|p| (p, *id)))
        .collect();

    // Bulk upsert variants, keyed on (product_id, external_pack_id, duration_months).
    let variant_rows: Vec<(Uuid, i32, bool, i32, Decimal)> = catalog
        .iter()
        .filter_map(|item| {
            let pack_id = item.external_pack_id.parse::<i32>().ok()?;
            let product_id = product_ids.get(&pack_id).copied()?;
            Some((
                product_id,
                item.duration_months,
                item.duration_months <= 0,
                pack_id,
                item.price,
            ))
        })
        .collect();

    let mut variant_qb: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(
            "INSERT INTO product_variants (product_id, duration_months, is_lifetime, external_pack_id, \
             price_in_credits) ",
        );
    let variants_count = variant_rows.len();
    if variants_count > 0 {
        variant_qb.push_values(
            variant_rows,
            |mut b, (product_id, duration, is_lifetime, pack_id, price)| {
                b.push_bind(product_id)
                    .push_bind(duration)
                    .push_bind(is_lifetime)
                    .push_bind(pack_id)
                    .push_bind(price);
            },
        );
        variant_qb.push(
            " ON CONFLICT (product_id, external_pack_id, duration_months) DO UPDATE SET \
                 price_in_credits = EXCLUDED.price_in_credits, updated_at = now()",
        );
        variant_qb.build().execute(pool).await?;
    }

    Ok(serde_json::json!({
        "provider": slug,
        "products_upserted": upserted.len(),
        "variants_upserted": variants_count,
    }))
}

#[derive(Debug, Deserialize)]
pub struct WhatsAppOrderRequest {
    pub order_id: Uuid,
}

/// Synchronous fulfillment for WhatsApp-driven orders (mirrors Django admin flow).
pub async fn whatsapp_order(
    pool: web::Data<PgPool>,
    settings: web::Data<Settings>,
    user: AuthUser,
    body: web::Json<WhatsAppOrderRequest>,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let order_id = body.order_id;

    crate::services::orders::fulfill_order(pool.get_ref(), &settings, order_id)
        .await
        .map_err(|e| match e {
            crate::services::orders::FulfillmentError::Provider(e) => ApiError::Provider(e),
            crate::services::orders::FulfillmentError::Database(e) => ApiError::Database(e),
            crate::services::orders::FulfillmentError::OrderNotFound(id) => {
                ApiError::NotFound(format!("order not found: {id}"))
            }
        })?;

    let status: String = sqlx::query_scalar("SELECT status FROM orders WHERE uuid = $1")
        .bind(order_id)
        .fetch_one(pool.get_ref())
        .await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "order_id": order_id,
        "status": status,
        "message": "whatsapp order fulfilled",
    })))
}

// --- provider CRUD ----------------------------------------------------------

type ProviderFullRow = (
    Uuid,
    String,
    String,
    String,
    Option<String>,
    serde_json::Value,
    serde_json::Value,
    bool,
    bool,
);

const ALLOWED_FIELD_TYPES: &[&str] = &["text", "secret", "url", "number", "select"];

fn validate_display_fields(extra_config: &serde_json::Value) -> Result<(), ApiError> {
    let Some(fields) = extra_config
        .get("display")
        .and_then(|d| d.get("fields"))
        .and_then(|f| f.as_array())
    else {
        return Ok(());
    };
    for field in fields {
        let name = field.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let label = field.get("label").and_then(|v| v.as_str()).unwrap_or("");
        let ftype = field.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if name.is_empty() || label.is_empty() || ftype.is_empty() {
            return Err(ApiError::BadRequest(
                "extra_config.display.fields: each field needs name, label and type.".into(),
            ));
        }
        if !ALLOWED_FIELD_TYPES.contains(&ftype) {
            return Err(ApiError::BadRequest(format!(
                "extra_config.display.fields: unsupported type '{ftype}'. Allowed: {ALLOWED_FIELD_TYPES:?}."
            )));
        }
        if ftype == "select"
            && !field
                .get("options")
                .and_then(|o| o.as_array())
                .is_some_and(|o| !o.is_empty())
        {
            return Err(ApiError::BadRequest(
                "extra_config.display.fields: 'select' fields require a non-empty 'options' array."
                    .into(),
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct ProviderWriteRequest {
    pub name: String,
    pub slug: String,
    pub adapter_key: String,
    pub api_endpoint: Option<String>,
    pub extra_config: Option<serde_json::Value>,
    pub provider_config: Option<serde_json::Value>,
    pub is_active: Option<bool>,
    pub api_token: Option<String>,
}

fn validate_provider_write(req: &ProviderWriteRequest) -> Result<(), ApiError> {
    if req.name.trim().is_empty() {
        return Err(ApiError::BadRequest("name is required.".into()));
    }
    let slug = req.slug.trim().to_lowercase();
    if slug.is_empty() {
        return Err(ApiError::BadRequest("slug is required.".into()));
    }
    if !slug
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(ApiError::BadRequest(
            "slug may only contain lowercase letters, digits and hyphens.".into(),
        ));
    }
    if req.adapter_key.trim().is_empty() {
        return Err(ApiError::BadRequest("adapter_key is required.".into()));
    }
    if let Some(extra) = &req.extra_config {
        if !extra.is_object() {
            return Err(ApiError::BadRequest(
                "extra_config must be a JSON object.".into(),
            ));
        }
        validate_display_fields(extra)?;
    }
    if let Some(pc) = &req.provider_config {
        if !pc.is_object() {
            return Err(ApiError::BadRequest(
                "provider_config must be a JSON object.".into(),
            ));
        }
    }
    Ok(())
}

fn encrypt_token_or_none(
    token: &Option<String>,
    settings: &Settings,
) -> Result<Option<Vec<u8>>, ApiError> {
    match token {
        None => Ok(None),
        Some(t) if t.trim().is_empty() => Ok(None),
        Some(t) => crate::utils::crypto::encrypt(
            t.trim().as_bytes(),
            settings.master_encryption_key.as_bytes(),
        )
        .map(Some)
        .map_err(|_| ApiError::BadRequest("failed to encrypt api_token".into())),
    }
}

pub async fn admin_providers_get(
    pool: web::Data<PgPool>,
    user: AuthUser,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let provider_id = path.into_inner();
    let row: Option<ProviderFullRow> =
        sqlx::query_as(
            "SELECT id, name, slug, adapter_key, api_endpoint, extra_config, provider_config, is_active, \
                    (api_token IS NOT NULL AND octet_length(api_token) > 0) \
             FROM providers WHERE id = $1",
        )
        .bind(provider_id)
        .fetch_optional(pool.get_ref())
        .await?;
    let Some((
        id,
        name,
        slug,
        adapter_key,
        api_endpoint,
        extra_config,
        provider_config,
        is_active,
        has_token,
    )) = row
    else {
        return Err(ApiError::NotFound("provider not found".into()));
    };
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "id": id,
        "name": name,
        "slug": slug,
        "adapter_key": adapter_key,
        "api_endpoint": api_endpoint.unwrap_or_default(),
        "extra_config": extra_config,
        "provider_config": provider_config,
        "is_active": is_active,
        "has_token": has_token,
    })))
}

pub async fn admin_providers_create(
    pool: web::Data<PgPool>,
    settings: web::Data<Settings>,
    user: AuthUser,
    body: web::Json<ProviderWriteRequest>,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    validate_provider_write(&body)?;
    let slug = body.slug.trim().to_lowercase();
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM providers WHERE slug = $1)")
        .bind(&slug)
        .fetch_one(pool.get_ref())
        .await?;
    if exists {
        return Err(ApiError::BadRequest(format!(
            "provider with slug '{slug}' already exists."
        )));
    }
    let token = encrypt_token_or_none(&body.api_token, &settings)?;
    let extra = body.extra_config.clone().unwrap_or(serde_json::json!({}));
    let provider_config = body
        .provider_config
        .clone()
        .unwrap_or(serde_json::json!({}));
    let row: Option<(Uuid, String)> = sqlx::query_as(
        "INSERT INTO providers (name, slug, adapter_key, api_endpoint, api_token, extra_config, provider_config, is_active) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
         ON CONFLICT (slug) DO UPDATE SET name = EXCLUDED.name, updated_at = now() \
         RETURNING id, name",
    )
    .bind(body.name.trim())
    .bind(&slug)
    .bind(body.adapter_key.trim())
    .bind(body.api_endpoint.clone().unwrap_or_default().trim())
    .bind(token)
    .bind(extra)
    .bind(provider_config)
    .bind(body.is_active.unwrap_or(false))
    .fetch_optional(pool.get_ref())
    .await?;
    let (id, name) =
        row.ok_or_else(|| ApiError::BadRequest("provider could not be created".into()))?;
    Ok(HttpResponse::Created().json(serde_json::json!({
        "id": id,
        "name": name,
        "detail": "provider created",
    })))
}

pub async fn admin_providers_update(
    pool: web::Data<PgPool>,
    settings: web::Data<Settings>,
    user: AuthUser,
    path: web::Path<Uuid>,
    body: web::Json<ProviderWriteRequest>,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    validate_provider_write(&body)?;
    let provider_id = path.into_inner();
    let slug = body.slug.trim().to_lowercase();
    let slug_taken: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM providers WHERE slug = $1 AND id <> $2)")
            .bind(&slug)
            .bind(provider_id)
            .fetch_one(pool.get_ref())
            .await?;
    if slug_taken {
        return Err(ApiError::BadRequest(format!(
            "provider with slug '{slug}' already exists."
        )));
    }
    let token = encrypt_token_or_none(&body.api_token, &settings)?;
    let existing: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM providers WHERE id = $1 AND api_token IS NOT NULL AND octet_length(api_token) > 0)",
    )
    .bind(provider_id)
    .fetch_one(pool.get_ref())
    .await?;
    let result = sqlx::query(
        "UPDATE providers SET name = $1, slug = $2, adapter_key = $3, api_endpoint = $4, \
                extra_config = $5, provider_config = $6, is_active = $7, \
                api_token = COALESCE($8, api_token), updated_at = now() \
         WHERE id = $9",
    )
    .bind(body.name.trim())
    .bind(&slug)
    .bind(body.adapter_key.trim())
    .bind(body.api_endpoint.clone().unwrap_or_default().trim())
    .bind(body.extra_config.clone().unwrap_or(serde_json::json!({})))
    .bind(
        body.provider_config
            .clone()
            .unwrap_or(serde_json::json!({})),
    )
    .bind(body.is_active.unwrap_or(true))
    .bind(token)
    .bind(provider_id)
    .execute(pool.get_ref())
    .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("provider not found".into()));
    }
    let _ = existing;
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "id": provider_id,
        "detail": "provider updated",
    })))
}

pub async fn admin_providers_delete(
    pool: web::Data<PgPool>,
    user: AuthUser,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, ApiError> {
    require_admin(&user.0)?;
    let provider_id = path.into_inner();
    let product_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM products WHERE provider_id = $1")
            .bind(provider_id)
            .fetch_one(pool.get_ref())
            .await?;
    if product_count > 0 {
        return Err(ApiError::BadRequest(format!(
            "Cannot delete provider. It has {product_count} product(s) assigned."
        )));
    }
    let result = sqlx::query("DELETE FROM providers WHERE id = $1")
        .bind(provider_id)
        .execute(pool.get_ref())
        .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("provider not found".into()));
    }
    Ok(HttpResponse::NoContent().finish())
}
