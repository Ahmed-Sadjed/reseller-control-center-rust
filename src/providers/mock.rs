use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde_json::json;

use super::{
    error::ProviderError,
    types::{CatalogProduct, DeviceCheckResult, ProvisionContext, ProvisionedCredential},
    ProviderAdapter,
};

/// Deterministic fake provider used when USE_MOCK_PROVIDER is enabled.
/// Never performs real network calls; safe for staging and CI.
/// Supports the same adapter keys as the factory (mock/hotplayer/tivipanel/
/// promax) so every flow can be exercised end-to-end without real panels.
#[derive(Debug, Clone)]
pub struct MockAdapter {
    key: &'static str,
}

impl MockAdapter {
    pub fn new() -> Self {
        Self { key: "mock" }
    }

    pub fn named(key: &'static str) -> Self {
        Self { key }
    }
}

impl Default for MockAdapter {
    fn default() -> Self {
        Self::new()
    }
}

fn valid_mac(mac: &str) -> bool {
    let mac = mac.to_uppercase();
    mac.len() == 17
        && mac
            .split(':')
            .all(|octet| octet.len() == 2 && octet.chars().all(|c| c.is_ascii_alphanumeric()))
}

fn expires_after(duration_months: Option<i32>) -> Option<DateTime<Utc>> {
    let months = duration_months.filter(|m| *m > 0)?;
    Some(Utc::now() + chrono::Duration::days(30 * months as i64))
}

#[async_trait]
impl ProviderAdapter for MockAdapter {
    fn name(&self) -> &'static str {
        self.key
    }

    async fn check_device(&self, mac: &str) -> Result<DeviceCheckResult, ProviderError> {
        if valid_mac(mac) {
            Ok(DeviceCheckResult {
                allowed: true,
                reason: None,
                credits_required: Some(Decimal::from(0)),
                plan: None,
                expires_at: None,
            })
        } else {
            Ok(DeviceCheckResult {
                allowed: false,
                reason: Some("invalid MAC address format".to_string()),
                credits_required: None,
                plan: None,
                expires_at: None,
            })
        }
    }

    async fn provision(
        &self,
        ctx: &ProvisionContext,
    ) -> Result<ProvisionedCredential, ProviderError> {
        let username = ctx
            .preferred_username
            .clone()
            .unwrap_or_else(|| format!("mock_user_{}", &ctx.order_id.simple().to_string()[..8]));
        let password = ctx
            .preferred_password
            .clone()
            .unwrap_or_else(|| "mock-default-password".to_string());

        Ok(ProvisionedCredential {
            username: username.clone(),
            password: password.clone(),
            dns: Some("mock.example.com".to_string()),
            m3u_url: Some(format!(
                "http://mock.example.com/get.php?username={username}&password={password}"
            )),
            expires_at: expires_after(ctx.duration_months),
            extra: json!({ "mock": true, "provider": self.key }),
        })
    }

    async fn fetch_catalog(&self) -> Result<Vec<CatalogProduct>, ProviderError> {
        Ok(vec![
            CatalogProduct {
                external_pack_id: "9001".to_string(),
                name: "Mock 1 Month".to_string(),
                duration_months: 1,
                price: Decimal::from(5),
                category: Some("standard".to_string()),
            },
            CatalogProduct {
                external_pack_id: "9003".to_string(),
                name: "Mock 3 Months".to_string(),
                duration_months: 3,
                price: Decimal::from(13),
                category: Some("standard".to_string()),
            },
            CatalogProduct {
                external_pack_id: "9012".to_string(),
                name: "Mock 12 Months".to_string(),
                duration_months: 12,
                price: Decimal::from(45),
                category: Some("premium".to_string()),
            },
        ])
    }

    async fn fetch_options(&self, kind: &str) -> Result<serde_json::Value, ProviderError> {
        match kind {
            // Django parity: MockProviderAdapter implements get_templates()
            // (returns []) but NOT get_domains() -> 501 on the endpoint.
            "templates" => Ok(json!([])),
            "domains" => Err(ProviderError::Unsupported(format!(
                "domains not supported by mock provider {key}",
                key = self.key
            ))),
            "bouquets" => match self.key {
                "promax" => Ok(json!([
                    { "id": 1001, "name": "Mock Bouquet Sports" },
                    { "id": 1002, "name": "Mock Bouquet Family" },
                    { "id": 1003, "name": "Mock Bouquet Premium" }
                ])),
                _ => Err(ProviderError::Unsupported(format!(
                    "bouquets not supported by mock provider {key}",
                    key = self.key
                ))),
            },
            _ => Err(ProviderError::Unsupported(format!(
                "{kind} not supported by mock provider {key}",
                key = self.key
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ProviderAdapter;
    use uuid::Uuid;

    fn ctx(duration_months: Option<i32>) -> ProvisionContext {
        ProvisionContext {
            product_name: "Test Plan".to_string(),
            duration_months,
            external_pack_id: Some(7711),
            order_id: Uuid::new_v4(),
            customer_username: "reseller".to_string(),
            mac: None,
            note: None,
            preferred_username: None,
            preferred_password: None,
            template_id: None,
            dns_domain_id: None,
            extra: json!({}),
        }
    }

    #[actix_rt::test]
    async fn check_device_accepts_valid_mac() {
        let mock = MockAdapter::new();
        let upper = mock.check_device("AA:BB:CC:DD:EE:FF").await.unwrap();
        assert!(upper.allowed);
        let lower = mock.check_device("aa:bb:cc:dd:ee:ff").await.unwrap();
        assert!(lower.allowed, "lowercase MACs must be accepted");
        assert_eq!(lower.credits_required, Some(Decimal::from(0)));
    }

    #[actix_rt::test]
    async fn check_device_rejects_invalid_mac() {
        let mock = MockAdapter::new();
        for bad in [
            "invalid",
            "AA:BB:CC:DD:EE",
            "AA:BB:CC:DD:EE:!!",
            "AABBCCDDEEFF",
            "AA:BB:CC:DD:EE:F",
            "",
        ] {
            let res = mock.check_device(bad).await.unwrap();
            assert!(!res.allowed, "mac '{bad}' should be denied");
            assert!(res.reason.is_some(), "denied mac '{bad}' needs a reason");
        }
    }

    #[actix_rt::test]
    async fn provision_returns_mock_credentials() {
        let mock = MockAdapter::new();
        let mut c = ctx(Some(1));
        c.preferred_username = Some("user1".to_string());
        c.preferred_password = Some("pass1".to_string());
        let res = mock.provision(&c).await.unwrap();
        assert_eq!(res.username, "user1");
        assert_eq!(res.password, "pass1");
        assert_eq!(res.dns.as_deref(), Some("mock.example.com"));
        assert!(res.m3u_url.unwrap().contains("user1"));
        assert!(res.expires_at.is_some(), "1-month plan gets an expiry");
        assert_eq!(res.extra["mock"], true);
    }

    #[actix_rt::test]
    async fn provision_generates_username_when_absent() {
        let mock = MockAdapter::new();
        let res = mock.provision(&ctx(None)).await.unwrap();
        assert!(
            res.username.starts_with("mock_user_"),
            "generated username must be mock_*"
        );
        assert!(res.expires_at.is_none(), "lifetime plans have no expiry");
    }

    #[actix_rt::test]
    async fn provision_respects_preferred_credentials_for_tivipanel_key() {
        let mock = MockAdapter::named("tivipanel");
        let res = mock.provision(&ctx(Some(12))).await.unwrap();
        assert_eq!(res.extra["provider"], "tivipanel");
        assert_eq!(mock.name(), "tivipanel");
        assert!(res.m3u_url.unwrap().starts_with("http://mock.example.com/"));
    }

    #[actix_rt::test]
    async fn promax_mock_exposes_bouquets() {
        let mock = MockAdapter::named("promax");
        let bouquets = mock.fetch_options("bouquets").await.unwrap();
        assert_eq!(bouquets.as_array().unwrap().len(), 3);
        assert_eq!(bouquets[0]["id"], 1001);
    }

    #[actix_rt::test]
    async fn non_promax_mock_has_no_bouquets() {
        let mock = MockAdapter::named("tivipanel");
        assert!(mock.fetch_options("bouquets").await.is_err());
    }

    #[actix_rt::test]
    async fn mock_exposes_templates_for_golden_api_key() {
        // Django parity: MockProviderAdapter.get_templates() returns [].
        let mock = MockAdapter::named("golden_api");
        let templates = mock.fetch_options("templates").await.unwrap();
        assert_eq!(templates.as_array().unwrap().len(), 0);
        assert_eq!(mock.name(), "golden_api");
    }

    #[actix_rt::test]
    async fn mock_has_no_domains() {
        // Django parity: MockProviderAdapter has no get_domains() -> 501.
        let mock = MockAdapter::named("golden_api");
        assert!(mock.fetch_options("domains").await.is_err());
    }

    #[actix_rt::test]
    async fn fetch_catalog_returns_three_products() {
        let mock = MockAdapter::new();
        let catalog = mock.fetch_catalog().await.unwrap();
        assert_eq!(catalog.len(), 3);
        assert_eq!(catalog[0].external_pack_id, "9001");
        assert_eq!(catalog[1].external_pack_id, "9003");
        assert_eq!(catalog[2].external_pack_id, "9012");
        assert!(catalog.iter().all(|p| p.duration_months > 0));
    }
}
