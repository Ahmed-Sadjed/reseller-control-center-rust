use async_trait::async_trait;
use serde_json::json;

use super::{
    error::ProviderError,
    types::{CatalogProduct, DeviceCheckResult, ProvisionContext, ProvisionedCredential},
    ProviderAdapter,
};

/// Real HTTP adapter for the HotPlayer Reseller API
/// (https://hotplayer.app/api/v1/reseller/).
/// Only reachable when USE_MOCK_PROVIDER=false (see factory.rs).
#[derive(Debug, Clone)]
pub struct HotPlayerAdapter {
    base_url: String,
    api_token: String,
    client: reqwest::Client,
}

impl HotPlayerAdapter {
    pub fn new(base_url: &str, api_token: &str, client: reqwest::Client) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_token: api_token.to_string(),
            client,
        }
    }

    async fn get_json(&self, path: &str) -> Result<serde_json::Value, ProviderError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("ApiKey {}", self.api_token))
            .send()
            .await
            .map_err(|e| ProviderError::Request(format!("{url}: {e}")))?;

        if !resp.status().is_success() {
            return Err(ProviderError::Remote(format!(
                "{url}: http {}",
                resp.status()
            )));
        }

        resp.json()
            .await
            .map_err(|e| ProviderError::Request(format!("bad response from {url}: {e}")))
    }

    async fn post_json(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, ProviderError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("ApiKey {}", self.api_token))
            .header("Content-Type", "application/json; charset=UTF-8")
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Request(format!("{url}: {e}")))?;

        if !resp.status().is_success() {
            return Err(ProviderError::Remote(format!(
                "{url}: http {}",
                resp.status()
            )));
        }

        resp.json()
            .await
            .map_err(|e| ProviderError::Request(format!("bad response from {url}: {e}")))
    }

    fn subscription_for(&self, ctx: &ProvisionContext) -> Result<&'static str, ProviderError> {
        match ctx.duration_months {
            Some(12) => Ok("YEAR_1"),
            Some(0) | None => Ok("FOREVER"),
            Some(other) => Err(ProviderError::Remote(format!(
                "HotPlayer supports only 12-month (YEAR_1) or lifetime (FOREVER) \
                 subscriptions, got {other} month(s)"
            ))),
        }
    }
}

#[async_trait]
impl ProviderAdapter for HotPlayerAdapter {
    fn name(&self) -> &'static str {
        "hotplayer"
    }

    async fn check_device(&self, mac: &str) -> Result<DeviceCheckResult, ProviderError> {
        let payload = self.get_json(&format!("/check-device/{mac}")).await?;
        let plan = payload
            .get("plan")
            .and_then(|v| v.as_str())
            .map(String::from);
        let expires_at = payload
            .get("expiration")
            .and_then(|v| v.as_i64())
            .and_then(chrono::DateTime::<chrono::Utc>::from_timestamp_millis);
        match payload.get("status").and_then(|v| v.as_str()) {
            Some("success") => Ok(DeviceCheckResult {
                allowed: true,
                reason: None,
                credits_required: None,
                plan,
                expires_at,
            }),
            Some("failed") => Ok(DeviceCheckResult {
                allowed: false,
                reason: payload
                    .get("message")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                credits_required: None,
                plan,
                expires_at,
            }),
            _ => Err(ProviderError::Remote(
                "unexpected check-device response".to_string(),
            )),
        }
    }

    async fn provision(
        &self,
        ctx: &ProvisionContext,
    ) -> Result<ProvisionedCredential, ProviderError> {
        let mac = ctx.mac.clone().ok_or_else(|| {
            ProviderError::Remote("MAC address is required for HotPlayer products".to_string())
        })?;
        let subscription = self.subscription_for(ctx)?;

        let mut body = serde_json::Map::new();
        body.insert("mac".into(), json!(mac));
        body.insert("subscription".into(), json!(subscription));
        if let Some(n) = &ctx.note {
            if !n.trim().is_empty() {
                body.insert("note".into(), json!(n));
            }
        }
        body.insert("extend".into(), json!(false));

        let payload = self.post_json("/activate", body.into()).await?;

        if payload.get("status").and_then(|v| v.as_str()) != Some("success") {
            return Err(ProviderError::Remote(
                payload
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("activation failed")
                    .to_string(),
            ));
        }

        let expires_at = match subscription {
            "FOREVER" => None,
            _ => Some(chrono::Utc::now() + chrono::Duration::days(365)),
        };

        Ok(ProvisionedCredential {
            username: mac,
            password: String::new(),
            dns: None,
            m3u_url: None,
            expires_at,
            extra: payload,
        })
    }

    async fn fetch_catalog(&self) -> Result<Vec<CatalogProduct>, ProviderError> {
        Ok(Vec::new())
    }
}
