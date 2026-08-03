use std::env;

#[derive(Debug, Clone)]
pub struct Settings {
    pub database_url: String,
    pub redis_url: String,
    pub jwt_secret: String,
    pub jwt_access_ttl_minutes: i64,
    pub jwt_refresh_ttl_days: i64,
    pub use_mock_provider: bool,
    pub master_encryption_key: String,
    pub workers: usize,
    pub async_threshold: i32,
    pub rate_limit_anon: u64,
    pub rate_limit_user: u64,
    pub rate_limit_purchase: u64,
    pub rust_log: String,
    pub redis_stream_key: String,
    pub media_dir: String,
}

impl Settings {
    pub fn from_env() -> Self {
        dotenvy::dotenv().ok();

        Self {
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://reseller_user:dev_password_123@localhost:5432/reseller_db".to_string()),
            redis_url: env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string()),
            jwt_secret: env::var("JWT_SECRET").unwrap_or_else(|_| "super_secret_change_me_32chars!".to_string()),
            jwt_access_ttl_minutes: env::var("JWT_ACCESS_TTL_MINUTES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(60),
            jwt_refresh_ttl_days: env::var("JWT_REFRESH_TTL_DAYS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(7),
            // SAFETY SWITCH: defaults to true. Real provider calls ONLY when explicitly "false".
            use_mock_provider: env::var("USE_MOCK_PROVIDER")
                .unwrap_or_else(|_| "true".to_string())
                .to_lowercase()
                != "false",
            master_encryption_key: env::var("MASTER_ENCRYPTION_KEY")
                .unwrap_or_else(|_| "staging_aes_256_key_32bytes12345".to_string()),
            workers: env::var("WORKERS").ok().and_then(|v| v.parse().ok()).unwrap_or(16),
            async_threshold: env::var("ASYNC_THRESHOLD")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10),
            rate_limit_anon: env::var("RATE_LIMIT_ANON")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(30),
            rate_limit_user: env::var("RATE_LIMIT_USER")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(100),
            rate_limit_purchase: env::var("RATE_LIMIT_PURCHASE")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(5),
            rust_log: env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
            redis_stream_key: env::var("REDIS_STREAM_KEY")
                .unwrap_or_else(|_| "orders:fulfill".to_string()),
            media_dir: env::var("MEDIA_DIR").unwrap_or_else(|_| "media".to_string()),
        }
    }
}
