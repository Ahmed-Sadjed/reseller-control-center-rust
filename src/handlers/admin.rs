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
            .and_then(|t| String::from_utf8(t.to_vec()).ok())
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
