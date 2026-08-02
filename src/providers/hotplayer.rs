use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use super::{
    error::ProviderError,
    types::{CatalogProduct, DeviceCheckResult, ProvisionContext, ProvisionedCredential},
    ProviderAdapter,
};

/// Real HTTP adapter for HotPlayer panels.
/// Only reachable when USE_MOCK_PROVIDER=false (see factory.rs).
#[derive(Debug, Clone)]
pub struct HotPlayerAdapter {
    base_url: String,
    api_token: String,
    client: reqwest::Client,
}

#[derive(Debug, Deserialize)]
struct HotPlayerResponse<T> {
    status: String,
    message: Option<String>,
    data: Option<T>,
}

impl HotPlayerAdapter {
    pub fn new(base_url: &str, api_token: &str, client: reqwest::Client) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_token: api_token.to_string(),
            client,
        }
    }

    async fn post_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> Result<T, ProviderError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.api_token)
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

        let payload: HotPlayerResponse<T> = resp
            .json()
            .await
            .map_err(|e| ProviderError::Request(format!("bad response from {url}: {e}")))?;

        if payload.status != "success" {
            return Err(ProviderError::Remote(
                payload.message.unwrap_or_else(|| "unknown provider error".to_string()),
            ));
        }

        payload
            .data
            .ok_or_else(|| ProviderError::Remote("empty response data".to_string()))
    }
}

#[async_trait]
impl ProviderAdapter for HotPlayerAdapter {
    fn name(&self) -> &'static str {
        "hotplayer"
    }

    async fn check_device(&self, mac: &str) -> Result<DeviceCheckResult, ProviderError> {
        let data: serde_json::Value = self
            .post_json(
                "/api/v1/device/check",
                json!({ "mac": mac.to_uppercase() }),
            )
            .await?;
        Ok(DeviceCheckResult {
            allowed: data
                .get("allowed")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            reason: data.get("reason").and_then(|v| v.as_str()).map(String::from),
            credits_required: data
                .get("credits_required")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok()),
        })
    }

    async fn provision(
        &self,
        ctx: &ProvisionContext,
    ) -> Result<ProvisionedCredential, ProviderError> {
        let mut body = serde_json::Map::new();
        if let Some(u) = &ctx.preferred_username {
            body.insert("username".into(), json!(u));
        }
        if let Some(p) = &ctx.preferred_password {
            body.insert("password".into(), json!(p));
        }
        if let Some(t) = &ctx.template_id {
            body.insert("template_id".into(), json!(t));
        }
        if let Some(d) = &ctx.dns_domain_id {
            body.insert("dns_domain_id".into(), json!(d));
        }
        if let Some(m) = &ctx.mac {
            body.insert("mac".into(), json!(m.to_uppercase()));
        }
        let data: serde_json::Value = self.post_json("/api/v1/playlists", body.into()).await?;

        Ok(ProvisionedCredential {
            username: data
                .get("streaming_username")
                .and_then(|v| v.as_str())
                .or_else(|| data.get("external_username").and_then(|v| v.as_str()))
                .unwrap_or_default()
                .to_string(),
            password: data
                .get("password")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            dns: data
                .get("dns_domain")
                .and_then(|v| v.as_str())
                .map(String::from),
            m3u_url: data.get("m3u_url").and_then(|v| v.as_str()).map(String::from),
            expires_at: data
                .get("expires_at")
                .and_then(|v| v.as_str())
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc)),
            extra: data,
        })
    }

    async fn fetch_catalog(&self) -> Result<Vec<CatalogProduct>, ProviderError> {
        let data: Vec<serde_json::Value> =
            self.post_json("/api/v1/catalog", json!({})).await?;
        let mut products = Vec::with_capacity(data.len());
        for item in data {
            products.push(CatalogProduct {
                external_pack_id: item
                    .get("external_pack_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                name: item
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                duration_months: item
                    .get("duration_months")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(1) as i32,
                price: item
                    .get("price")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_default(),
                category: item
                    .get("category")
                    .and_then(|v| v.as_str())
                    .map(String::from),
            });
        }
        Ok(products)
    }
}
