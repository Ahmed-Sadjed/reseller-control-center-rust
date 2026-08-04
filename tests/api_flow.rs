use actix_test::TestServer;
use actix_web::http::StatusCode;
use reseller_control_center_rust::{
    app::build_app, config::Settings, db, queue, utils::crypto::hash_password,
};
use rust_decimal::Decimal;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::time::{Duration, Instant};
use uuid::Uuid;

const TEST_DB_NAME: &str = "reseller_rust_test";
const TEST_REDIS_DB: u8 = 15;
/// Settings with hard guards: tests must NEVER talk to a real provider or a
/// real (non-test) Redis namespace. Fails fast if the safety switch is off.
fn test_settings() -> Settings {
    let mut settings = Settings::from_env();
    assert!(
        settings.use_mock_provider,
        "USE_MOCK_PROVIDER must be true: tests must only use the MockAdapter, never a real provider API"
    );
    // All tests share one Redis DB and one peer IP (127.0.0.1), so the real
    // throttle limits would make the suite flaky: the anon bucket would
    // accumulate across tests and across runs for a full hour. Raise the
    // limits here; the dedicated throttle test sets its own purchase limit.
    settings.rate_limit_anon = 1_000_000;
    settings.rate_limit_user = 1_000_000;
    settings.rate_limit_purchase = 1_000_000;
    settings
}

/// Isolates tests on a dedicated Redis logical DB (15) so streams, blacklists
/// and cache keys never collide with the dev server on DB 0.
fn test_redis_url(settings: &Settings) -> String {
    let base = settings.redis_url.trim_end_matches('/').to_string();
    let without_db = base
        .rfind('/')
        .map(|i| {
            let (head, tail) = base.split_at(i);
            if tail[1..].chars().all(|c| c.is_ascii_digit()) {
                head.to_string()
            } else {
                base.clone()
            }
        })
        .unwrap_or(base);
    format!("{without_db}/{TEST_REDIS_DB}")
}

/// Creates (if missing) and migrates a dedicated test database, leaving the
/// dev database untouched. Returns a pool to it.
async fn ensure_test_db() -> PgPool {
    let admin_url = test_settings().database_url;
    let (base, _db) = admin_url.rsplit_once('/').unwrap();
    let admin_pool = PgPool::connect(&format!("{base}/postgres"))
        .await
        .expect("connect to postgres admin db");

    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM pg_database WHERE datname = $1)",
    )
    .bind(TEST_DB_NAME)
    .fetch_one(&admin_pool)
    .await
    .unwrap();

    if !exists {
        if let Err(e) = sqlx::query(&format!("CREATE DATABASE {TEST_DB_NAME}"))
            .execute(&admin_pool)
            .await
        {
            let is_duplicate = e
                .as_database_error()
                .and_then(|db| db.code())
                .map(|c| c.as_ref() == "23505")
                .unwrap_or(false);
            if !is_duplicate {
                panic!("create test database: {e}");
            }
        }
    }

    let url = format!("{base}/{TEST_DB_NAME}");
    let pool = db::create_pool(&url)
        .await
        .expect("connect to test database");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("run migrations on test database");
    pool
}

async fn seed_user(
    pool: &PgPool,
    username: &str,
    password: &str,
    role: &str,
    credits: &str,
) -> Uuid {
    let existing: Option<Uuid> =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE username = $1")
            .bind(username)
            .fetch_optional(pool)
            .await
            .unwrap();
    if let Some(id) = existing {
        return id;
    }
    let hash = hash_password(password).unwrap();
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO users (username, email, password_hash, role, credit_balance, is_active, is_staff, is_superuser) \
         VALUES ($1, $2, $3, $4, $5, true, false, false) RETURNING id",
    )
    .bind(username)
    .bind(format!("{username}@test.local"))
    .bind(hash)
    .bind(role)
    .bind(credits.parse::<Decimal>().unwrap())
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_catalog(pool: &PgPool) -> Uuid {
    let provider_id: Uuid = match sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM providers WHERE slug = 'test-provider'",
    )
    .fetch_optional(pool)
    .await
    .unwrap()
    {
        Some(id) => id,
        None => sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO providers (name, slug, adapter_key, api_endpoint) \
                 VALUES ('Test Provider', 'test-provider', 'mock', 'https://panel.mock.invalid') \
                 RETURNING id",
        )
        .fetch_one(pool)
        .await
        .unwrap(),
    };

    let category_id: Uuid =
        match sqlx::query_scalar::<_, Uuid>("SELECT id FROM categories WHERE slug = 'test-cat'")
            .fetch_optional(pool)
            .await
            .unwrap()
        {
            Some(id) => id,
            None => sqlx::query_scalar::<_, Uuid>(
                "INSERT INTO categories (name, slug) VALUES ('Test Cat', 'test-cat') RETURNING id",
            )
            .fetch_one(pool)
            .await
            .unwrap(),
        };

    let product_id: Uuid = sqlx::query_scalar(
        "INSERT INTO products (name, category_id, provider_id, external_pack_id, duration_months, \
         price_in_credits, is_active, is_manual) \
         VALUES ('Test Plan', $1, $2, 7711, 1, 10.00, true, false) \
         ON CONFLICT (provider_id, external_pack_id) WHERE external_pack_id IS NOT NULL \
         DO UPDATE SET name = EXCLUDED.name \
         RETURNING id",
    )
    .bind(category_id)
    .bind(provider_id)
    .fetch_one(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO product_variants (product_id, duration_months, is_lifetime, external_pack_id, \
         price_in_credits, is_active) \
         VALUES ($1, 1, false, 7711, 10.00, true) \
         ON CONFLICT (product_id, external_pack_id, duration_months) \
         DO UPDATE SET price_in_credits = EXCLUDED.price_in_credits",
    )
    .bind(product_id)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM product_variants WHERE product_id = $1 AND external_pack_id = 7711",
    )
    .bind(product_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn login(server: &TestServer, username: &str, password: &str) -> Value {
    let mut resp = server
        .post("/api/auth/login")
        .send_json(&json!({ "username": username, "password": password }))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    resp.json::<Value>().await.unwrap()
}

fn auth_header(token: &str) -> (&'static str, String) {
    ("Authorization", format!("Bearer {token}"))
}

/// Decimal values serialize as strings; numbers arrive as JSON numbers.
fn as_f64(v: &Value) -> f64 {
    v.as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .or_else(|| v.as_f64())
        .expect("value must be numeric")
}

/// Full purchase journey: login -> catalog -> reserve -> sync fulfillment (qty
/// <= threshold) -> credentials -> credentials list -> check-device ->
/// idempotent replay rejected.
#[actix_rt::test]
async fn full_purchase_flow_through_redis_stream() {
    let settings = test_settings();
    let pool = ensure_test_db().await;
    let redis_client = redis::Client::open(test_redis_url(&settings)).unwrap();
    let redis_conn = redis_client
        .get_multiplexed_async_connection()
        .await
        .unwrap();

    {
        let pool = pool.clone();
        let settings = settings.clone();
        let client = redis_client.clone();
        tokio::spawn(async move {
            queue::run_worker(pool, settings, client).await;
        });
    }

    let username = format!("flow_user_{}", std::process::id());
    seed_user(&pool, &username, "testpass123", "RESELLER", "1000.00").await;
    let variant_id = seed_catalog(&pool).await;

    let server =
        actix_test::start(move || build_app(pool.clone(), settings.clone(), redis_conn.clone()));

    // health
    let resp = server.get("/health").send().await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // login
    let login_body = login(&server, &username, "testpass123").await;
    let access = login_body["access"].as_str().unwrap();
    assert_eq!(login_body["user"]["role"], "RESELLER");

    // catalog shows the seeded product (paginated {results} response)
    let mut resp = server.get("/api/catalog/products").send().await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["count"], 1, "catalog count must be paginated");
    assert!(
        body["total_pages"].is_number(),
        "total_pages must be present"
    );
    let products = body["results"].as_array().unwrap();
    assert!(
        products.iter().any(|p| p["name"] == "Test Plan"),
        "seeded product must appear in catalog: {body}"
    );

    // stats includes live balance
    let mut resp = server
        .get("/api/stats")
        .append_header(auth_header(access))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(as_f64(&body["credit_balance"]), 1000.0);

    // missing Idempotency-Key -> 400
    let resp = server
        .post("/api/orders")
        .append_header(auth_header(access))
        .send_json(&json!({ "variant_id": variant_id, "quantity": 1 }))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // create order (qty 1 <= threshold) -> sync path: 201 COMPLETED with credentials
    let mut resp = server
        .post("/api/orders")
        .append_header(auth_header(access))
        .append_header(("Idempotency-Key", "flow-idem-1"))
        .send_json(&json!({ "variant_id": variant_id, "quantity": 1 }))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "COMPLETED");
    assert_eq!(as_f64(&body["total_credits"]), 10.0);
    assert_eq!(as_f64(&body["balance_after"]), 990.0);
    let order_id = body["order_id"].as_str().unwrap();
    assert!(
        body["credentials"].as_array().unwrap().len() == 1,
        "sync response must include the created credentials"
    );

    // poll status until the stream worker completes it
    let deadline = Instant::now() + Duration::from_secs(20);
    let status = loop {
        let mut resp = server
            .get(format!("/api/orders/{order_id}/status"))
            .append_header(auth_header(access))
            .send()
            .await
            .unwrap();
        let body: Value = resp.json().await.unwrap();
        if body["status"] == "COMPLETED" {
            break body;
        }
        assert!(
            Instant::now() < deadline,
            "order never completed via worker: {body}"
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    };
    assert_eq!(status["status"], "COMPLETED");

    // exactly one credential was created (sync path)
    let mut resp = server
        .get(format!("/api/orders/{order_id}/credentials"))
        .append_header(auth_header(access))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let creds: Value = resp.json().await.unwrap();
    let creds = creds.as_array().unwrap();
    assert_eq!(creds.len(), 1, "worker must persist exactly one credential");
    let username = creds[0]["streaming_username"].as_str().unwrap();
    assert!(
        username.starts_with("mock_"),
        "generated mock username must be mock_*, got '{username}'"
    );
    assert!(creds[0]["dns_domain"].as_str().is_some());
    assert!(
        creds[0]["password"].as_str().is_some(),
        "completed orders must expose the decrypted password"
    );
    assert_eq!(
        creds[0]["password"].as_str().unwrap(),
        "mock-default-password",
        "decrypted password must round-trip through AES-256-GCM"
    );

    // replaying the same idempotency key -> 409 with the original order
    let mut resp = server
        .post("/api/orders")
        .append_header(auth_header(access))
        .append_header(("Idempotency-Key", "flow-idem-1"))
        .send_json(&json!({ "variant_id": variant_id, "quantity": 1 }))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["order_id"], order_id);

    // balance deducted exactly once
    let mut resp = server
        .get("/api/stats")
        .append_header(auth_header(access))
        .send()
        .await
        .unwrap();
    let body: Value = resp.json().await.unwrap();
    assert_eq!(as_f64(&body["credit_balance"]), 990.0);

    // credentials list: paginated, exposes username/url but NEVER the password
    let mut resp = server
        .get("/api/credentials")
        .append_header(auth_header(access))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();
    let items = body["results"].as_array().unwrap();
    assert_eq!(
        items.len(),
        1,
        "credentials list must contain the credential"
    );
    assert!(
        items[0]["username"].as_str().is_some(),
        "list item must expose username"
    );
    assert!(
        items[0]["url"].as_str().is_some(),
        "list item must expose the m3u host as url"
    );
    assert!(
        items[0]["password"].is_null(),
        "credentials list must NEVER expose the password"
    );

    // check-device: invalid MAC -> 400, well-formed MAC -> 200 (no active
    // hotplayer provider in the test DB -> error branch, never a real call)
    let resp = server
        .post("/api/check-device")
        .append_header(auth_header(access))
        .send_json(&json!({ "mac": "not-a-mac" }))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let mut resp = server
        .post("/api/check-device")
        .append_header(auth_header(access))
        .send_json(&json!({ "mac": "00:1A:79:12:34:56" }))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["found"], false);
    assert_eq!(body["status"], "error");
}

#[actix_rt::test]
async fn refresh_rotation_blacklists_old_token() {
    let settings = test_settings();
    let pool = ensure_test_db().await;
    let redis_client = redis::Client::open(test_redis_url(&settings)).unwrap();
    let redis_conn = redis_client
        .get_multiplexed_async_connection()
        .await
        .unwrap();

    let username = format!("refresh_user_{}", std::process::id());
    seed_user(&pool, &username, "testpass123", "RESELLER", "500.00").await;

    let server =
        actix_test::start(move || build_app(pool.clone(), settings.clone(), redis_conn.clone()));

    let login_body = login(&server, &username, "testpass123").await;
    let access = login_body["access"].as_str().unwrap();
    let old_refresh = login_body["refresh"].as_str().unwrap().to_string();

    // refresh rotates: old refresh is blacklisted, new pair issued
    let mut resp = server
        .post("/api/auth/refresh")
        .send_json(&json!({ "refresh": old_refresh }))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();
    let new_access = body["access"].as_str().unwrap().to_string();
    let new_refresh = body["refresh"].as_str().unwrap().to_string();

    // old access token still valid until logout
    let resp = server
        .get("/api/auth/me")
        .append_header(auth_header(access))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // reusing the consumed refresh token -> 401
    let resp = server
        .post("/api/auth/refresh")
        .send_json(&json!({ "refresh": old_refresh }))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // new refresh token still works
    let resp = server
        .post("/api/auth/refresh")
        .send_json(&json!({ "refresh": new_refresh }))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // logout blacklists access + refresh; /me then rejected
    let resp = server
        .post("/api/auth/logout")
        .send_json(&json!({ "refresh": new_refresh, "access": new_access }))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let resp = server
        .get("/api/auth/me")
        .append_header(auth_header(&new_access))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_rt::test]
async fn reseller_is_forbidden_on_admin_endpoints() {
    let settings = test_settings();
    let pool = ensure_test_db().await;
    let redis_client = redis::Client::open(test_redis_url(&settings)).unwrap();
    let redis_conn = redis_client
        .get_multiplexed_async_connection()
        .await
        .unwrap();

    let username = format!("rbac_user_{}", std::process::id());
    seed_user(&pool, &username, "testpass123", "RESELLER", "500.00").await;

    let server =
        actix_test::start(move || build_app(pool.clone(), settings.clone(), redis_conn.clone()));

    let login_body = login(&server, &username, "testpass123").await;
    let access = login_body["access"].as_str().unwrap();

    let resp = server
        .post("/api/dashboard/providers/sync")
        .append_header(auth_header(access))
        .send_json(&json!({}))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    let resp = server
        .post("/api/dashboard/whatsapp-orders")
        .append_header(auth_header(access))
        .send_json(&json!({ "order_id": Uuid::new_v4() }))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[actix_rt::test]
async fn unauthenticated_requests_rejected() {
    let settings = test_settings();
    let pool = ensure_test_db().await;
    let redis_client = redis::Client::open(test_redis_url(&settings)).unwrap();
    let redis_conn = redis_client
        .get_multiplexed_async_connection()
        .await
        .unwrap();

    let server =
        actix_test::start(move || build_app(pool.clone(), settings.clone(), redis_conn.clone()));

    let resp = server.get("/api/auth/me").send().await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    let resp = server.post("/api/orders").send().await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// Quantity above the async threshold (10) -> 202 PENDING, fulfilled by the
/// Redis Stream worker with one credential per unit.
#[actix_rt::test]
async fn async_purchase_enqueued_via_redis_stream() {
    let mut settings = test_settings();
    // Isolate this test's worker from `full_purchase_flow_through_redis_stream`
    // (same Redis DB 15): a distinct stream key means a worker killed by the
    // other test's runtime can never orphan this test's order.
    settings.redis_stream_key = "orders:fulfill:worker2".to_string();
    let pool = ensure_test_db().await;
    let redis_client = redis::Client::open(test_redis_url(&settings)).unwrap();
    let redis_conn = redis_client
        .get_multiplexed_async_connection()
        .await
        .unwrap();

    {
        let pool = pool.clone();
        let settings = settings.clone();
        let client = redis_client.clone();
        tokio::spawn(async move {
            queue::run_worker(pool, settings, client).await;
        });
    }

    let username = format!("bulk_user_{}", std::process::id());
    seed_user(&pool, &username, "testpass123", "RESELLER", "1000.00").await;
    let variant_id = seed_catalog(&pool).await;

    let server =
        actix_test::start(move || build_app(pool.clone(), settings.clone(), redis_conn.clone()));

    let login_body = login(&server, &username, "testpass123").await;
    let access = login_body["access"].as_str().unwrap();

    // qty 11 > ASYNC_THRESHOLD -> accepted asynchronously with status PENDING
    let mut resp = server
        .post("/api/orders")
        .append_header(auth_header(access))
        .append_header(("Idempotency-Key", "bulk-idem-1"))
        .send_json(&json!({ "variant_id": variant_id, "quantity": 11 }))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "PENDING");
    assert_eq!(as_f64(&body["total_credits"]), 110.0);
    assert_eq!(as_f64(&body["balance_after"]), 890.0);
    let order_id = body["order_id"].as_str().unwrap();

    // poll status until the stream worker completes it
    let deadline = Instant::now() + Duration::from_secs(20);
    let status = loop {
        let mut resp = server
            .get(format!("/api/orders/{order_id}/status"))
            .append_header(auth_header(access))
            .send()
            .await
            .unwrap();
        let body: Value = resp.json().await.unwrap();
        if body["status"] == "COMPLETED" {
            break body;
        }
        assert!(
            Instant::now() < deadline,
            "async order never completed via worker: {body}"
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    };
    assert_eq!(status["status"], "COMPLETED");

    let mut resp = server
        .get(format!("/api/orders/{order_id}/credentials"))
        .append_header(auth_header(access))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let creds: Value = resp.json().await.unwrap();
    assert_eq!(
        creds.as_array().unwrap().len(),
        11,
        "one credential per purchased unit"
    );
}

/// Purchase endpoint is rate-limited to RATE_LIMIT_PURCHASE (5/min) per user:
/// the 6th POST /api/orders in the same minute returns 429 with the Django
/// throttled body shape and a Retry-After header. Uses a fresh user so the
/// bucket never collides with other tests (each test owns unique Redis keys).
#[actix_rt::test]
async fn purchase_throttle_rejects_after_five_per_minute() {
    let mut settings = test_settings();
    settings.rate_limit_purchase = 5;
    let pool = ensure_test_db().await;
    let redis_client = redis::Client::open(test_redis_url(&settings)).unwrap();
    let redis_conn = redis_client
        .get_multiplexed_async_connection()
        .await
        .unwrap();

    let username = format!("throttle_user_{}", std::process::id());
    seed_user(&pool, &username, "testpass123", "RESELLER", "500.00").await;

    let server =
        actix_test::start(move || build_app(pool.clone(), settings.clone(), redis_conn.clone()));

    let login_body = login(&server, &username, "testpass123").await;
    let access = login_body["access"].as_str().unwrap();

    // Five requests under the limit: handler-level 400 (unknown variant) is
    // fine — the middleware counts every POST to /api/orders regardless.
    for i in 0..5 {
        let resp = server
            .post("/api/orders")
            .append_header(auth_header(access))
            .append_header(("Idempotency-Key", format!("throttle-idem-{i}")))
            .send_json(&json!({ "variant_id": Uuid::new_v4(), "quantity": 1 }))
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "unknown variant must fail with 404, not be throttled yet"
        );
    }

    // Sixth request in the same minute -> 429 with Django's throttled body
    let mut resp = server
        .post("/api/orders")
        .append_header(auth_header(access))
        .append_header(("Idempotency-Key", "throttle-idem-6"))
        .send_json(&json!({ "variant_id": Uuid::new_v4(), "quantity": 1 }))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(
        resp.headers().get("retry-after").is_some(),
        "429 must carry a Retry-After header"
    );
    let body: Value = resp.json().await.unwrap();
    let detail = body["detail"].as_str().unwrap();
    assert!(
        detail.starts_with("Request was throttled. Expected available in"),
        "body must match Django throttle shape, got: {detail}"
    );
}
