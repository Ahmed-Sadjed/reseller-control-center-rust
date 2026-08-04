//! Gold Panel reseller API (adapter key `goldpanel`) — GET with query params.
//! The pure helpers below do NO network calls and are fully unit-tested;
//! only the `GoldPanelAdapter` at the bottom performs GETs (and it is only
//! reachable when USE_MOCK_PROVIDER=false, see factory.rs).
//! Mirrors backend/api/providers/gold_panel.py:
//!   create  : GET <api_endpoint>?action=new&type=m3u&sub=<months>&pack=<id>&api_key=..
//! Status check is `result.status != 'true'` (string), the response may be a
//! dict or a one-element list, and the m3u url's query string carries the
//! line credentials.

use async_trait::async_trait;
use reqwest::Url;
use serde::Deserialize;

use super::{
    error::ProviderError,
    promax::dns_from_m3u_url,
    types::{CatalogProduct, DeviceCheckResult, ProvisionContext, ProvisionedCredential},
    ProviderAdapter,
};

/// Default DNS domain and streaming port when the provider's extra_config
/// does not override them (Django parity: 8k.cms-only.ru:8080).
pub fn default_dns_domain(extra: &serde_json::Value) -> String {
    extra
        .get("dns_domain")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("8k.cms-only.ru")
        .to_string()
}

pub fn default_port(extra: &serde_json::Value) -> u16 {
    extra
        .get("port")
        .and_then(|v| v.as_u64())
        .map(|p| p as u16)
        .unwrap_or(8080)
}

#[derive(Debug, Deserialize)]
pub struct GoldPanelLine {
    pub status: String,
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub url: String,
}

/// action=new URL. `sub` is the variant duration in months (0 = lifetime).
pub fn build_new_url(
    api_endpoint: &str,
    api_key: &str,
    sub: i32,
    pack: i32,
    country: Option<&str>,
    notes: Option<&str>,
) -> Result<Url, ProviderError> {
    let mut url = Url::parse(api_endpoint).map_err(|e| {
        ProviderError::Request(format!(
            "invalid goldpanel api_endpoint '{api_endpoint}': {e}"
        ))
    })?;
    url.query_pairs_mut()
        .append_pair("action", "new")
        .append_pair("type", "m3u")
        .append_pair("sub", &sub.to_string())
        .append_pair("pack", &pack.to_string())
        .append_pair("api_key", api_key);
    if let Some(c) = country {
        url.query_pairs_mut().append_pair("country", c);
    }
    if let Some(n) = notes {
        url.query_pairs_mut().append_pair("notes", n);
    }
    Ok(url)
}

/// Parse an `action=new` response into a provisioned credential. The panel
/// returns a dict or a one-element list; the caller fills `expires_at`.
pub fn parse_new_response(
    body: &str,
    dns_domain: &str,
    port: u16,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<ProvisionedCredential, ProviderError> {
    let raw: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        ProviderError::Remote(format!("goldpanel: malformed response ({e}): {body}"))
    })?;
    let result = match raw {
        serde_json::Value::Array(arr) => arr
            .first()
            .ok_or_else(|| ProviderError::Remote("goldpanel: empty response list".to_string()))?
            .clone(),
        other => other,
    };
    let line: GoldPanelLine = serde_json::from_value(result.clone())
        .map_err(|e| ProviderError::Remote(format!("goldpanel: malformed line ({e}): {result}")))?;
    if line.status != "true" {
        return Err(ProviderError::Remote(format!(
            "Gold Panel error: {}",
            if line.message.is_empty() {
                "line creation failed".to_string()
            } else {
                line.message
            }
        )));
    }
    if line.user_id.is_empty() {
        return Err(ProviderError::Remote(
            "Missing 'user_id' in Gold Panel response".to_string(),
        ));
    }

    // username/password live in the m3u url query string; fall back to
    // user_id, and password defaults to username (Django parity).
    let mut username = line.user_id.clone();
    let mut password = String::new();
    if !line.url.is_empty() {
        let query: std::collections::HashMap<String, String> = Url::parse(&line.url)
            .map_err(|e| ProviderError::Remote(format!("goldpanel: bad m3u url: {e}")))?
            .query_pairs()
            .into_owned()
            .collect();
        username = query
            .get("username")
            .filter(|s| !s.is_empty())
            .cloned()
            .unwrap_or(line.user_id.clone());
        password = query.get("password").cloned().unwrap_or_default();
    }
    if password.is_empty() {
        password = username.clone();
    }

    let m3u_url = if !line.url.is_empty() {
        line.url.clone()
    } else {
        format!("https://{dns_domain}:{port}/get.php?username={username}&password={password}")
    };
    let dns = dns_from_m3u_url(&m3u_url).unwrap_or_else(|_| format!("https://{dns_domain}:{port}"));

    Ok(ProvisionedCredential {
        username,
        password,
        dns: Some(dns),
        m3u_url: Some(m3u_url),
        expires_at,
        extra: serde_json::json!({
            "provider": "goldpanel",
            "user_id": line.user_id,
            "status": line.status,
            "message": line.message,
        }),
    })
}

/// Real HTTP adapter for Gold Panel reseller panels.
/// Only reachable when USE_MOCK_PROVIDER=false (see factory.rs).
#[derive(Debug, Clone)]
pub struct GoldPanelAdapter {
    api_endpoint: String,
    api_key: String,
    client: reqwest::Client,
}

impl GoldPanelAdapter {
    pub fn new(api_endpoint: &str, api_key: &str, client: reqwest::Client) -> Self {
        Self {
            api_endpoint: api_endpoint.to_string(),
            api_key: api_key.to_string(),
            client,
        }
    }

    async fn get_text(&self, url: &Url) -> Result<String, ProviderError> {
        let resp = self
            .client
            .get(url.clone())
            .send()
            .await
            .map_err(|e| ProviderError::Request(format!("{url}: {e}")))?;
        if !resp.status().is_success() {
            return Err(ProviderError::Remote(format!(
                "{url}: http {}",
                resp.status()
            )));
        }
        resp.text()
            .await
            .map_err(|e| ProviderError::Request(format!("bad response from {url}: {e}")))
    }
}

#[async_trait]
impl ProviderAdapter for GoldPanelAdapter {
    fn name(&self) -> &'static str {
        "goldpanel"
    }

    async fn check_device(&self, _mac: &str) -> Result<DeviceCheckResult, ProviderError> {
        Err(ProviderError::Unsupported(
            "goldpanel: device check not available (M3U only)".to_string(),
        ))
    }

    async fn provision(
        &self,
        ctx: &ProvisionContext,
    ) -> Result<ProvisionedCredential, ProviderError> {
        let pack_id = ctx.external_pack_id.ok_or_else(|| {
            ProviderError::Request("goldpanel: product has no external_pack_id".to_string())
        })?;
        let sub = ctx.duration_months.unwrap_or(0);
        let url = build_new_url(
            &self.api_endpoint,
            &self.api_key,
            sub,
            pack_id,
            None,
            Some(&ctx.order_id.to_string()),
        )?;
        let body = self.get_text(&url).await?;
        let dns = default_dns_domain(&ctx.extra);
        let port = default_port(&ctx.extra);
        let expires_at = ctx
            .duration_months
            .filter(|m| *m > 0)
            .map(|m| chrono::Utc::now() + chrono::Duration::days(30 * m as i64));
        parse_new_response(&body, &dns, port, expires_at)
    }

    async fn fetch_catalog(&self) -> Result<Vec<CatalogProduct>, ProviderError> {
        Err(ProviderError::Unsupported(
            "goldpanel: no catalog sync".to_string(),
        ))
    }
}

// Parse-only unit tests: no network involved.
#[cfg(test)]
mod tests {
    use super::*;

    const ENDPOINT: &str = "https://panel.goldpanel.example.com/api.php";

    #[test]
    fn new_url_contains_expected_params() {
        let url = build_new_url(ENDPOINT, "KEY", 6, 9, None, Some("uuid-2"))
            .unwrap()
            .to_string();
        assert!(url.starts_with(ENDPOINT));
        assert!(url.contains("action=new"));
        assert!(url.contains("type=m3u"));
        assert!(url.contains("sub=6"));
        assert!(url.contains("pack=9"));
        assert!(url.contains("api_key=KEY"));
    }

    #[test]
    fn dict_response_parsed_with_url_credentials() {
        let body = r#"{"status":"true","user_id":"g123","url":"https://stream.gp.example.com/get.php?username=g123&password=x9"}"#;
        let cred = parse_new_response(body, "8k.cms-only.ru", 8080, None).unwrap();
        assert_eq!(cred.username, "g123");
        assert_eq!(cred.password, "x9");
        assert_eq!(cred.dns.as_deref(), Some("https://stream.gp.example.com"));
        assert_eq!(cred.extra["provider"], "goldpanel");
    }

    #[test]
    fn list_response_parsed_too() {
        let body = r#"[{"status":"true","user_id":"l1","url":""}]"#;
        let cred = parse_new_response(body, "8k.cms-only.ru", 8080, None).unwrap();
        assert_eq!(cred.username, "l1");
        assert_eq!(cred.password, "l1");
        assert_eq!(
            cred.m3u_url.as_deref(),
            Some("https://8k.cms-only.ru:8080/get.php?username=l1&password=l1")
        );
    }

    #[test]
    fn non_true_status_is_remote_error() {
        let err = parse_new_response(
            r#"{"status":"false","message":"no credit"}"#,
            "dns",
            1,
            None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("Gold Panel error: no credit"));

        let err = parse_new_response(r#"{"status":"error"}"#, "dns", 1, None).unwrap_err();
        assert!(err.to_string().contains("Gold Panel error"));
    }

    #[test]
    fn missing_user_id_is_remote_error() {
        assert!(parse_new_response(r#"{"status":"true"}"#, "dns", 1, None).is_err());
        assert!(parse_new_response("[]", "dns", 1, None).is_err());
    }

    #[test]
    fn defaults_used_from_extra_config() {
        assert_eq!(default_dns_domain(&serde_json::json!({})), "8k.cms-only.ru");
        assert_eq!(default_port(&serde_json::json!({"port": 9090})), 9090);
    }
}
