use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;

use super::error::ProviderError;

#[derive(Debug, Clone, serde::Serialize)]
pub struct DeviceCheckResult {
    pub allowed: bool,
    pub reason: Option<String>,
    pub credits_required: Option<Decimal>,
    /// Panel subscription code (e.g. "YEAR_1", "FOREVER").
    pub plan: Option<String>,
    /// Device's current subscription expiry, if the panel reports one.
    pub expires_at: Option<DateTime<Utc>>,
}

/// One fully provisioned line returned by a panel (M3U accounts only).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProvisionedCredential {
    pub username: String,
    pub password: String,
    pub dns: Option<String>,
    pub m3u_url: Option<String>,
    /// None => lifetime. Panels that do not return an expiry get a computed
    /// one (now + duration_months * 30 days) in the adapter.
    pub expires_at: Option<DateTime<Utc>>,
    /// Raw provider response, stored verbatim in credentials.data.
    pub extra: serde_json::Value,
}

/// Everything a panel needs to create ONE line. `provision()` is called once
/// per purchased quantity.
#[derive(Debug, Clone)]
pub struct ProvisionContext {
    pub product_name: String,
    /// Variant duration in months; Some(0) or None => lifetime.
    pub duration_months: Option<i32>,
    pub external_pack_id: Option<i32>,
    pub order_id: Uuid,
    pub customer_username: String,
    pub mac: Option<String>,
    pub note: Option<String>,
    pub preferred_username: Option<String>,
    pub preferred_password: Option<String>,
    /// TiviPanel template id / ProMax bouquet id (comes from checkout).
    pub template_id: Option<String>,
    pub dns_domain_id: Option<String>,
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CatalogProduct {
    pub external_pack_id: String,
    pub name: String,
    pub duration_months: i32,
    pub price: Decimal,
    pub category: Option<String>,
}

#[async_trait]
pub trait ProviderAdapter: Send + Sync {
    fn name(&self) -> &'static str;

    async fn check_device(&self, mac: &str) -> Result<DeviceCheckResult, ProviderError>;

    /// Provision a single line/credential. Never called while
    /// USE_MOCK_PROVIDER is true (factory safety switch).
    async fn provision(&self, ctx: &ProvisionContext) -> Result<ProvisionedCredential, ProviderError>;

    async fn fetch_catalog(&self) -> Result<Vec<CatalogProduct>, ProviderError>;

    /// Panel lookup endpoints: "templates", "domains" (golden_api),
    /// "bouquets" (promax). Unsupported by default.
    async fn fetch_options(&self, kind: &str) -> Result<serde_json::Value, ProviderError> {
        Err(ProviderError::Unsupported(format!(
            "{kind} not supported by this provider"
        )))
    }
}
