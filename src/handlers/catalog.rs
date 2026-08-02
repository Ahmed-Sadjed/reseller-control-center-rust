use actix_web::{web, HttpRequest, HttpResponse};
use redis::AsyncCommands;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    config::Settings,
    error::ApiError,
    middleware::AuthUser,
    models::{duration_display, ProductListItem, ProductVariant},
};

const CATEGORIES_CACHE_TTL: u64 = 300;
const PRODUCTS_CACHE_TTL: u64 = 300;

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
struct CategoryWithCount {
    id: Uuid,
    name: String,
    slug: String,
    description: String,
    image: Option<String>,
    is_active: bool,
    sort_order: i32,
    product_count: i64,
}

pub async fn categories(
    pool: web::Data<PgPool>,
    redis_conn: web::Data<redis::aio::MultiplexedConnection>,
) -> Result<HttpResponse, ApiError> {
    let cache_key = "catalog:categories";
    let mut conn = redis_conn.get_ref().clone();

    if let Some(cached) = conn.get::<_, Option<String>>(cache_key).await? {
        return Ok(HttpResponse::Ok()
            .content_type("application/json")
            .body(cached));
    }

    let rows = sqlx::query_as::<_, CategoryWithCount>(
        "SELECT c.id, c.name, c.slug, c.description, c.image, c.is_active, c.sort_order, \
                (SELECT count(*) FROM products p WHERE p.category_id = c.id AND p.is_active = true)::bigint \
                AS product_count \
         FROM categories c \
         WHERE c.is_active = true \
         ORDER BY c.sort_order ASC, c.name ASC",
    )
    .fetch_all(pool.get_ref())
    .await?;

    let body = serde_json::to_string(&rows)?;
    conn.set_ex::<_, _, ()>(cache_key, &body, CATEGORIES_CACHE_TTL)
        .await?;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(body))
}

#[derive(Debug, serde::Deserialize)]
pub struct ProductQuery {
    pub search: Option<String>,
    pub category: Option<String>,
    pub provider: Option<Uuid>,
    pub page: Option<i64>,
    #[serde(rename = "page_size")]
    pub page_size: Option<i64>,
}

pub async fn products(
    pool: web::Data<PgPool>,
    redis_conn: web::Data<redis::aio::MultiplexedConnection>,
    req: HttpRequest,
    query: web::Query<ProductQuery>,
) -> Result<HttpResponse, ApiError> {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(20).clamp(1, 100);
    let cache_key = format!(
        "catalog:products:{}:{}:{}:{}:{}",
        query.search.as_deref().unwrap_or(""),
        query.category.as_deref().unwrap_or(""),
        query.provider.map(|c| c.to_string()).unwrap_or_default(),
        page,
        page_size
    );

    let mut conn = redis_conn.get_ref().clone();
    if let Some(cached) = conn.get::<_, Option<String>>(&cache_key).await? {
        return Ok(HttpResponse::Ok()
            .content_type("application/json")
            .body(cached));
    }

    // Single-flight: on a cache miss only ONE request recomputes; the others
    // wait briefly for the winner so 800 VUs never stampede the DB pool.
    let lock_key = format!("{cache_key}:lock");
    let locked: bool = redis::cmd("SET")
        .arg(&lock_key)
        .arg("1")
        .arg("NX")
        .arg("EX")
        .arg("5")
        .query_async::<Option<String>>(&mut conn)
        .await
        .map(|r| r.is_some())
        .unwrap_or(false);
    if !locked {
        for _ in 0..40 {
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            if let Some(cached) = conn.get::<_, Option<String>>(&cache_key).await? {
                return Ok(HttpResponse::Ok()
                    .content_type("application/json")
                    .body(cached));
            }
        }
        // Lock holder gave up or is too slow: compute anyway.
    }

    // Django parity: `category` filters by category SLUG; `provider` by UUID.
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM products p \
         LEFT JOIN categories c ON c.id = p.category_id \
         WHERE p.is_active = true \
           AND ($1::text IS NULL OR p.name ILIKE '%' || $1 || '%') \
           AND ($2::text IS NULL OR c.slug = $2) \
           AND ($3::uuid IS NULL OR p.provider_id = $3)",
    )
    .bind(&query.search)
    .bind(&query.category)
    .bind(query.provider)
    .fetch_one(pool.get_ref())
    .await?;

    let total_pages = (count as i64 + page_size - 1) / page_size;

    // Single LEFT JOIN query avoids N+1 for category/provider names.
    let rows = sqlx::query_as::<_, ProductRow>(
        "SELECT p.id, p.name, p.category_id, c.name AS category_name, c.slug AS category_slug, \
                p.provider_id, prov.name AS provider_name, prov.adapter_key AS provider_key, \
                p.description, p.image, p.is_active, p.is_manual, p.credential_type, p.created_at, p.updated_at, \
                (SELECT count(*) FROM credentials cr JOIN orders o ON o.id = cr.order_id \
                  WHERE o.product_id = p.id AND cr.is_revoked = false)::bigint AS available_credentials \
         FROM products p \
         LEFT JOIN categories c ON c.id = p.category_id \
         LEFT JOIN providers prov ON prov.id = p.provider_id \
         WHERE p.is_active = true \
           AND ($1::text IS NULL OR p.name ILIKE '%' || $1 || '%') \
           AND ($2::text IS NULL OR c.slug = $2) \
           AND ($3::uuid IS NULL OR p.provider_id = $3) \
         ORDER BY p.created_at DESC \
         LIMIT $4 OFFSET $5",
    )
    .bind(&query.search)
    .bind(&query.category)
    .bind(query.provider)
    .bind(page_size)
    .bind((page - 1) * page_size)
    .fetch_all(pool.get_ref())
    .await?;

    // Collect variants in one query per page (batched, not N+1 per product).
    let product_ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let mut variants: Vec<ProductVariant> = Vec::new();
    if !product_ids.is_empty() {
        variants = sqlx::query_as::<_, ProductVariant>(
            "SELECT id, product_id, duration_months, is_lifetime, external_pack_id, price_in_credits, \
             is_active, created_at, updated_at FROM product_variants \
             WHERE product_id = ANY($1) AND is_active = true ORDER BY duration_months ASC",
        )
        .bind(&product_ids)
        .fetch_all(pool.get_ref())
        .await?;
        for v in variants.iter_mut() {
            v.display_name = Some(duration_display(v.duration_months, v.is_lifetime));
        }
    }

    let items: Vec<ProductListItem> = rows
        .into_iter()
        .map(|r| ProductListItem {
            id: r.id,
            name: r.name,
            category_id: r.category_id,
            category_name: r.category_name,
            category_slug: r.category_slug,
            provider_id: r.provider_id,
            provider_name: r.provider_name,
            provider_key: r.provider_key,
            description: r.description,
            image: r.image,
            is_active: r.is_active,
            is_manual: r.is_manual,
            available_credentials: r.available_credentials,
            variants: variants
                .iter()
                .filter(|v| v.product_id == r.id)
                .cloned()
                .collect(),
        })
        .collect();

    let next = if page < total_pages.max(1) {
        Some(build_page_url(&req, &query, page + 1, page_size))
    } else {
        None
    };
    let previous = if page > 1 {
        Some(build_page_url(&req, &query, page - 1, page_size))
    } else {
        None
    };

    let body = serde_json::to_string(&serde_json::json!({
        "count": count,
        "total_pages": total_pages,
        "next": next,
        "previous": previous,
        "results": items,
    }))?;
    conn.set_ex::<_, _, ()>(&cache_key, &body, PRODUCTS_CACHE_TTL)
        .await?;
    if locked {
        let _: redis::RedisResult<()> = redis::cmd("DEL").arg(&lock_key).query_async(&mut conn).await;
    }

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(body))
}

/// Relative next/previous links mirroring Django's pagination (frontend only
/// consumes `total_pages`/`results`, so relative paths are sufficient).
fn build_page_url(
    req: &HttpRequest,
    query: &ProductQuery,
    page: i64,
    page_size: i64,
) -> String {
    let mut params: Vec<(String, String)> = Vec::new();
    if let Some(s) = &query.search {
        params.push(("search".to_string(), s.clone()));
    }
    if let Some(c) = &query.category {
        params.push(("category".to_string(), c.clone()));
    }
    if let Some(p) = query.provider {
        params.push(("provider".to_string(), p.to_string()));
    }
    params.push(("page".to_string(), page.to_string()));
    if page_size != 20 {
        params.push(("page_size".to_string(), page_size.to_string()));
    }
    let qs = params
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");
    format!("{}?{qs}", req.path())
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ProductRow {
    id: Uuid,
    name: String,
    category_id: Option<Uuid>,
    category_name: Option<String>,
    category_slug: Option<String>,
    provider_id: Option<Uuid>,
    provider_name: Option<String>,
    provider_key: Option<String>,
    description: String,
    image: Option<String>,
    is_active: bool,
    is_manual: bool,
    credential_type: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    available_credentials: Option<i64>,
}

/// GET /api/promax-bouquets/?provider_id=<uuid> — mirrors Django's
/// PromaxBouquetsView. The pack id selected here is later sent as
/// `template_id` on checkout (ProMax `pack` param).
pub async fn promax_bouquets(
    pool: web::Data<PgPool>,
    settings: web::Data<Settings>,
    query: web::Query<ProviderQuery>,
    _user: AuthUser,
) -> Result<HttpResponse, ApiError> {
    let provider_id = query
        .provider_id
        .ok_or_else(|| ApiError::bad_request("provider_id is required."))?;
    let provider = sqlx::query_as::<_, (String, Option<String>, Option<Vec<u8>>)>(
        "SELECT adapter_key, api_endpoint, api_token FROM providers \
         WHERE id = $1 AND is_active = true",
    )
    .bind(provider_id)
    .fetch_optional(pool.get_ref())
    .await?
    .ok_or_else(|| ApiError::NotFound("provider".into()))?;

    let (adapter_key, endpoint, api_token) = provider;
    if adapter_key != "promax" {
        return Err(ApiError::bad_request(
            "Bouquets are only available for Promax providers.",
        ));
    }

    let token = api_token
        .as_deref()
        .and_then(|t| String::from_utf8(t.to_vec()).ok());
    let adapter = crate::providers::get_provider(
        &adapter_key,
        endpoint.as_deref(),
        token.as_deref(),
        &settings,
    )?;
    let bouquets = adapter.fetch_options("bouquets").await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({ "bouquets": bouquets })))
}

/// Shared golden-api endpoint logic: fetch the provider, enforce the
/// adapter_key, run `fetch_options(kind)` and map errors like Django
/// (AttributeError -> 501, everything else -> 400 with the raw message).
async fn golden_options(
    pool: web::Data<PgPool>,
    settings: web::Data<Settings>,
    query: web::Query<ProviderQuery>,
    kind: &str,
    not_supported_msg: &str,
    wrong_adapter_msg: &str,
) -> Result<HttpResponse, ApiError> {
    let provider_id = query
        .provider_id
        .ok_or_else(|| ApiError::bad_request("provider_id is required."))?;
    let provider = sqlx::query_as::<_, (String, Option<String>, Option<Vec<u8>>)>(
        "SELECT adapter_key, api_endpoint, api_token FROM providers \
         WHERE id = $1 AND is_active = true",
    )
    .bind(provider_id)
    .fetch_optional(pool.get_ref())
    .await?
    .ok_or_else(|| ApiError::NotFound("provider".into()))?;

    let (adapter_key, endpoint, api_token) = provider;
    if adapter_key != "golden_api" {
        return Err(ApiError::bad_request(wrong_adapter_msg));
    }

    let token = api_token
        .as_deref()
        .and_then(|t| String::from_utf8(t.to_vec()).ok());
    let adapter = crate::providers::get_provider(
        &adapter_key,
        endpoint.as_deref(),
        token.as_deref(),
        &settings,
    )?;
    match adapter.fetch_options(kind).await {
        Ok(value) => Ok(HttpResponse::Ok().json(serde_json::json!({ kind: value }))),
        Err(crate::providers::ProviderError::Unsupported(_)) => {
            Err(ApiError::NotSupported(not_supported_msg.to_string()))
        }
        Err(e) => Err(ApiError::bad_request(e.to_string())),
    }
}

/// GET /api/golden-templates/?provider_id=<uuid> — mirrors Django's
/// GoldenTemplatesView (Golden OTT bouquet templates).
pub async fn golden_templates(
    pool: web::Data<PgPool>,
    settings: web::Data<Settings>,
    query: web::Query<ProviderQuery>,
    _user: AuthUser,
) -> Result<HttpResponse, ApiError> {
    golden_options(
        pool,
        settings,
        query,
        "templates",
        "Adapter does not support templates.",
        "Templates are only available for Golden API providers.",
    )
    .await
}

/// GET /api/golden-domains/?provider_id=<uuid> — mirrors Django's
/// GoldenDomainsView (Golden OTT DNS domains).
pub async fn golden_domains(
    pool: web::Data<PgPool>,
    settings: web::Data<Settings>,
    query: web::Query<ProviderQuery>,
    _user: AuthUser,
) -> Result<HttpResponse, ApiError> {
    golden_options(
        pool,
        settings,
        query,
        "domains",
        "Adapter does not support domains.",
        "Domains are only available for Golden API providers.",
    )
    .await
}

#[derive(Debug, serde::Deserialize)]
pub struct ProviderQuery {
    pub provider_id: Option<Uuid>,
}
