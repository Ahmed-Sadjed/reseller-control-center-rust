use actix_cors::Cors;
use actix_web::{
    dev::{ServiceFactory, ServiceRequest, ServiceResponse},
    http::header,
    web, App, Error, HttpResponse,
};
use sqlx::PgPool;

use crate::{config::Settings, handlers};

async fn health(pool: web::Data<PgPool>) -> HttpResponse {
    let db_ok = sqlx::query("SELECT 1")
        .execute(pool.get_ref())
        .await
        .is_ok();
    HttpResponse::Ok().json(serde_json::json!({
        "status": "healthy",
        "service": "reseller-control-center-rust",
        "database": if db_ok { "ok" } else { "error" },
    }))
}

/// The React frontend was built against the old Django backend, which
/// accepted trailing slashes on every path (e.g. /api/auth/login/). Register
/// every route with both forms so the existing UI works unchanged.
fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/auth")
            .route("/login", web::post().to(handlers::auth::login))
            .route("/login/", web::post().to(handlers::auth::login))
            .route("/refresh", web::post().to(handlers::auth::refresh))
            .route("/refresh/", web::post().to(handlers::auth::refresh))
            .route("/logout", web::post().to(handlers::auth::logout))
            .route("/logout/", web::post().to(handlers::auth::logout))
            .route("/me", web::get().to(handlers::auth::me))
            .route("/me/", web::get().to(handlers::auth::me)),
    )
    .service(
        web::scope("/api/catalog")
            .route("/categories", web::get().to(handlers::catalog::categories))
            .route("/categories/", web::get().to(handlers::catalog::categories))
            .route("/products", web::get().to(handlers::catalog::products))
            .route("/products/", web::get().to(handlers::catalog::products)),
    )
    .service(
        web::scope("/api/orders")
            .route("", web::post().to(handlers::orders::create_order))
            .route("", web::get().to(handlers::orders::list_orders))
            .route("/", web::post().to(handlers::orders::create_order))
            .route("/", web::get().to(handlers::orders::list_orders))
            .route("/{uuid}", web::get().to(handlers::orders::order_detail))
            .route("/{uuid}/", web::get().to(handlers::orders::order_detail))
            .route(
                "/{uuid}/status",
                web::get().to(handlers::orders::order_status),
            )
            .route(
                "/{uuid}/status/",
                web::get().to(handlers::orders::order_status),
            )
            .route(
                "/{uuid}/credentials",
                web::get().to(handlers::orders::order_credentials),
            )
            .route(
                "/{uuid}/credentials/",
                web::get().to(handlers::orders::order_credentials),
            ),
    )
    .service(
        web::scope("/api/stats")
            .route("", web::get().to(handlers::stats::stats))
            .route("/", web::get().to(handlers::stats::stats)),
    )
    .service(
        web::scope("/api/promax-bouquets")
            .route("", web::get().to(handlers::catalog::promax_bouquets))
            .route("/", web::get().to(handlers::catalog::promax_bouquets)),
    )
    .service(
        web::scope("/api/dashboard")
            .route(
                "/resellers",
                web::get().to(handlers::dashboard::resellers_list),
            )
            .route(
                "/resellers/",
                web::get().to(handlers::dashboard::resellers_list),
            )
            .route(
                "/resellers",
                web::post().to(handlers::dashboard::resellers_create),
            )
            .route(
                "/resellers/",
                web::post().to(handlers::dashboard::resellers_create),
            )
            .route(
                "/resellers/{id}",
                web::get().to(handlers::dashboard::reseller_detail),
            )
            .route(
                "/resellers/{id}/",
                web::get().to(handlers::dashboard::reseller_detail),
            )
            .route(
                "/resellers/{id}",
                web::put().to(handlers::dashboard::reseller_update),
            )
            .route(
                "/resellers/{id}/",
                web::put().to(handlers::dashboard::reseller_update),
            )
            .route(
                "/resellers/{id}",
                web::delete().to(handlers::dashboard::reseller_delete),
            )
            .route(
                "/resellers/{id}/",
                web::delete().to(handlers::dashboard::reseller_delete),
            )
            .route(
                "/resellers/{id}/credits",
                web::post().to(handlers::dashboard::reseller_credits),
            )
            .route(
                "/resellers/{id}/credits/",
                web::post().to(handlers::dashboard::reseller_credits),
            )
            .route(
                "/resellers/{id}/transactions",
                web::get().to(handlers::dashboard::reseller_transactions),
            )
            .route(
                "/resellers/{id}/transactions/",
                web::get().to(handlers::dashboard::reseller_transactions),
            )
            .route(
                "/resellers/{id}/orders",
                web::get().to(handlers::dashboard::reseller_orders),
            )
            .route(
                "/resellers/{id}/orders/",
                web::get().to(handlers::dashboard::reseller_orders),
            )
            .route(
                "/resellers/{id}/toggle",
                web::post().to(handlers::dashboard::reseller_toggle),
            )
            .route(
                "/resellers/{id}/toggle/",
                web::post().to(handlers::dashboard::reseller_toggle),
            )
            .route(
                "/settings",
                web::get().to(handlers::dashboard::settings_get),
            )
            .route(
                "/settings/",
                web::get().to(handlers::dashboard::settings_get),
            )
            .route(
                "/settings",
                web::put().to(handlers::dashboard::settings_put),
            )
            .route(
                "/settings/",
                web::put().to(handlers::dashboard::settings_put),
            )
            .route(
                "/products",
                web::get().to(handlers::dashboard::admin_products_list),
            )
            .route(
                "/products/",
                web::get().to(handlers::dashboard::admin_products_list),
            )
            .route("/products", web::post().to(handlers::admin::create_product))
            .route(
                "/products/",
                web::post().to(handlers::admin::create_product),
            )
            .route(
                "/products/create",
                web::post().to(handlers::dashboard::admin_products_create),
            )
            .route(
                "/products/create/",
                web::post().to(handlers::dashboard::admin_products_create),
            )
            .route(
                "/products/{id}",
                web::put().to(handlers::dashboard::admin_products_update),
            )
            .route(
                "/products/{id}/",
                web::put().to(handlers::dashboard::admin_products_update),
            )
            .route(
                "/products/{id}",
                web::delete().to(handlers::dashboard::admin_products_delete),
            )
            .route(
                "/products/{id}/",
                web::delete().to(handlers::dashboard::admin_products_delete),
            )
            .route(
                "/products/{id}/variants",
                web::get().to(handlers::dashboard::variants_list),
            )
            .route(
                "/products/{id}/variants/",
                web::get().to(handlers::dashboard::variants_list),
            )
            .route(
                "/products/{id}/variants/create",
                web::post().to(handlers::dashboard::variant_create),
            )
            .route(
                "/products/{id}/variants/create/",
                web::post().to(handlers::dashboard::variant_create),
            )
            .route(
                "/products/{id}/variants/{vid}",
                web::put().to(handlers::dashboard::variant_update),
            )
            .route(
                "/products/{id}/variants/{vid}/",
                web::put().to(handlers::dashboard::variant_update),
            )
            .route(
                "/products/{id}/variants/{vid}",
                web::delete().to(handlers::dashboard::variant_delete),
            )
            .route(
                "/products/{id}/variants/{vid}/",
                web::delete().to(handlers::dashboard::variant_delete),
            )
            .route(
                "/categories",
                web::get().to(handlers::dashboard::admin_categories_list),
            )
            .route(
                "/categories/",
                web::get().to(handlers::dashboard::admin_categories_list),
            )
            .route(
                "/categories/create",
                web::post().to(handlers::dashboard::admin_category_create),
            )
            .route(
                "/categories/create/",
                web::post().to(handlers::dashboard::admin_category_create),
            )
            .route(
                "/categories/{id}",
                web::put().to(handlers::dashboard::admin_category_update),
            )
            .route(
                "/categories/{id}/",
                web::put().to(handlers::dashboard::admin_category_update),
            )
            .route(
                "/categories/{id}",
                web::delete().to(handlers::dashboard::admin_category_delete),
            )
            .route(
                "/categories/{id}/",
                web::delete().to(handlers::dashboard::admin_category_delete),
            )
            .route(
                "/providers",
                web::get().to(handlers::dashboard::admin_providers_list),
            )
            .route(
                "/providers/",
                web::get().to(handlers::dashboard::admin_providers_list),
            )
            .route(
                "/providers",
                web::post().to(handlers::admin::admin_providers_create),
            )
            .route(
                "/providers/",
                web::post().to(handlers::admin::admin_providers_create),
            )
            .route(
                "/providers/sync",
                web::post().to(handlers::admin::sync_providers),
            )
            .route(
                "/providers/sync/",
                web::post().to(handlers::admin::sync_providers),
            )
            .route(
                "/providers/{id}",
                web::get().to(handlers::admin::admin_providers_get),
            )
            .route(
                "/providers/{id}/",
                web::get().to(handlers::admin::admin_providers_get),
            )
            .route(
                "/providers/{id}",
                web::put().to(handlers::admin::admin_providers_update),
            )
            .route(
                "/providers/{id}/",
                web::put().to(handlers::admin::admin_providers_update),
            )
            .route(
                "/providers/{id}",
                web::delete().to(handlers::admin::admin_providers_delete),
            )
            .route(
                "/providers/{id}/",
                web::delete().to(handlers::admin::admin_providers_delete),
            )
            .route(
                "/manual-products",
                web::get().to(handlers::dashboard::manual_products_list),
            )
            .route(
                "/manual-products/",
                web::get().to(handlers::dashboard::manual_products_list),
            )
            .route(
                "/manual-products/{id}",
                web::get().to(handlers::dashboard::manual_product_detail),
            )
            .route(
                "/manual-products/{id}/",
                web::get().to(handlers::dashboard::manual_product_detail),
            )
            .route(
                "/manual-products/{id}/credentials",
                web::post().to(handlers::dashboard::credential_create),
            )
            .route(
                "/manual-products/{id}/credentials/",
                web::post().to(handlers::dashboard::credential_create),
            )
            .route(
                "/manual-products/{id}/credentials/bulk",
                web::post().to(handlers::dashboard::credential_bulk_create),
            )
            .route(
                "/manual-products/{id}/credentials/bulk/",
                web::post().to(handlers::dashboard::credential_bulk_create),
            )
            .route(
                "/credentials/{id}",
                web::put().to(handlers::dashboard::credential_update),
            )
            .route(
                "/credentials/{id}/",
                web::put().to(handlers::dashboard::credential_update),
            )
            .route(
                "/credentials/{id}",
                web::delete().to(handlers::dashboard::credential_delete),
            )
            .route(
                "/credentials/{id}/",
                web::delete().to(handlers::dashboard::credential_delete),
            )
            .route(
                "/whatsapp-orders",
                web::get().to(handlers::dashboard::whatsapp_orders_list),
            )
            .route(
                "/whatsapp-orders/",
                web::get().to(handlers::dashboard::whatsapp_orders_list),
            )
            .route(
                "/whatsapp-orders/{uuid}/complete",
                web::post().to(handlers::dashboard::whatsapp_order_complete),
            )
            .route(
                "/whatsapp-orders/{uuid}/complete/",
                web::post().to(handlers::dashboard::whatsapp_order_complete),
            )
            .route(
                "/whatsapp-orders",
                web::post().to(handlers::admin::whatsapp_order),
            )
            .route(
                "/whatsapp-orders/",
                web::post().to(handlers::admin::whatsapp_order),
            )
            .route(
                "/stats",
                web::get().to(handlers::dashboard::dashboard_stats),
            )
            .route(
                "/stats/",
                web::get().to(handlers::dashboard::dashboard_stats),
            )
            .route(
                "/top-resellers",
                web::get().to(handlers::dashboard::top_resellers),
            )
            .route(
                "/top-resellers/",
                web::get().to(handlers::dashboard::top_resellers),
            )
            .route(
                "/recent-activity",
                web::get().to(handlers::dashboard::recent_activity),
            )
            .route(
                "/recent-activity/",
                web::get().to(handlers::dashboard::recent_activity),
            )
            .route(
                "/provider-health",
                web::get().to(handlers::dashboard::provider_health),
            )
            .route(
                "/provider-health/",
                web::get().to(handlers::dashboard::provider_health),
            ),
    )
    // Django-compatible aliases: the React frontend calls these paths.
    // MUST be registered last: actix-web 4.14's router matches in registration
    // order without backtracking, so a bare /api scope would shadow the more
    // specific /api/* scopes registered before it.
    .service(
        web::scope("/api")
            .route("/categories", web::get().to(handlers::catalog::categories))
            .route("/categories/", web::get().to(handlers::catalog::categories))
            .route("/products", web::get().to(handlers::catalog::products))
            .route("/products/", web::get().to(handlers::catalog::products))
            .route("/purchase", web::post().to(handlers::orders::create_order))
            .route("/purchase/", web::post().to(handlers::orders::create_order))
            .route(
                "/check-device",
                web::post().to(handlers::orders::check_device),
            )
            .route(
                "/check-device/",
                web::post().to(handlers::orders::check_device),
            )
            .route(
                "/credentials",
                web::get().to(handlers::orders::credentials_list),
            )
            .route(
                "/credentials/",
                web::get().to(handlers::orders::credentials_list),
            )
            .route(
                "/golden-templates",
                web::get().to(handlers::catalog::golden_templates),
            )
            .route(
                "/golden-templates/",
                web::get().to(handlers::catalog::golden_templates),
            )
            .route(
                "/golden-domains",
                web::get().to(handlers::catalog::golden_domains),
            )
            .route(
                "/golden-domains/",
                web::get().to(handlers::catalog::golden_domains),
            ),
    );
}

/// Shared app factory used by both the binary and integration tests.
/// `redis_conn` is a single shared multiplexed Redis connection: clones of a
/// MultiplexedConnection reuse the same underlying TCP socket (no per-request
/// connect churn, no ephemeral-port exhaustion under load).
pub fn build_app(
    pool: PgPool,
    settings: Settings,
    redis_conn: redis::aio::MultiplexedConnection,
) -> App<
    impl ServiceFactory<
        ServiceRequest,
        Config = (),
        Response = ServiceResponse<impl actix_web::body::MessageBody>,
        Error = Error,
        InitError = (),
    >,
> {
    App::new()
        .app_data(web::Data::new(settings))
        .app_data(web::Data::new(pool))
        .app_data(web::Data::new(redis_conn))
        // CORS mirrors Django: localhost dev origins with credentials, plus
        // the Idempotency-Key header the storefront sends on purchases.
        // NOTE: HeaderName::from_static panics on uppercase — use lowercase.
        .wrap(
            Cors::default()
                .allowed_origin("http://localhost:5173")
                .allowed_origin("http://localhost:80")
                .allowed_methods(["GET", "POST", "PUT", "DELETE", "OPTIONS"])
                .allowed_headers([
                    header::AUTHORIZATION,
                    header::CONTENT_TYPE,
                    header::ACCEPT,
                    header::ORIGIN,
                    header::HeaderName::from_static("idempotency-key"),
                ])
                .max_age(3600)
                .supports_credentials(),
        )
        // Django parity throttles: anon 30/hour per IP, user 100/minute,
        // purchase 5/minute (Redis-backed, fail-open).
        .wrap(crate::middleware::RateLimit)
        .route("/health", web::get().to(health))
        .configure(configure_routes)
}
