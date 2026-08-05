//! CMS-only reseller panels (adapter key `neo4k`) — GET with query params.
//! The pure helpers below do NO network calls and are fully unit-tested;
//! only the `CmsOnlyAdapter` at the bottom performs GETs (and it is only
//! reachable when USE_MOCK_PROVIDER=false, see factory.rs).
//! Mirrors backend/api/providers/cms_only.py:
//!   create  : GET <api_endpoint>?action=new&type=m3u&sub=<months>&pack=<id>&api_key=..
//! The response body contains the full m3u `url`; username/password are read
//! from its query string (fallbacks: password=username, username=data fields).

use async_trait::async_trait;
use reqwest::Url;
use rust_decimal::Decimal;
use serde::Deserialize;

use super::{
    error::ProviderError,
    promax::dns_from_m3u_url,
    types::{CatalogProduct, DeviceCheckResult, ProvisionContext, ProvisionedCredential},
    ProviderAdapter, StatusValue, StringOrNum,
};

/// Default DNS domain and streaming port when the provider's extra_config
/// does not override them (Django parity: kmapp.xyz:8080).
pub fn default_dns_domain(extra: &serde_json::Value) -> String {
    extra
        .get("dns_domain")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("kmapp.xyz")
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
pub struct CmsNewLine {
    pub status: StatusValue,
    #[serde(default)]
    pub user_id: StringOrNum,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub result: String,
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
            "invalid cms_only api_endpoint '{api_endpoint}': {e}"
        ))
    })?;
    url.query_pairs_mut()
        .append_pair("action", "new")
        .append_pair("type", "m3u")
        .append_pair("sub", &sub.to_string())
        .append_pair("pack", &pack.to_string());
    if let Some(c) = country {
        url.query_pairs_mut().append_pair("country", c);
    }
    if let Some(n) = notes {
        url.query_pairs_mut().append_pair("notes", n);
    }
    url.query_pairs_mut().append_pair("api_key", api_key);
    Ok(url)
}

/// action=bouquet URL: the panel's package list (read-only).
pub fn build_catalog_url(api_endpoint: &str, api_key: &str) -> Result<Url, ProviderError> {
    let mut url = Url::parse(api_endpoint).map_err(|e| {
        ProviderError::Request(format!(
            "invalid cms_only api_endpoint '{api_endpoint}': {e}"
        ))
    })?;
    url.query_pairs_mut()
        .append_pair("action", "bouquet")
        .append_pair("api_key", api_key);
    Ok(url)
}

fn catalog_price(message: &str) -> Decimal {
    if let Some(idx) = message.rfind(" - ") {
        let tail = &message[idx + 3..];
        let tail = tail
            .strip_suffix(" Credits")
            .or_else(|| tail.strip_suffix(" Credit"));
        if let Some(credits) = tail {
            if let Ok(price) = credits.trim().parse::<Decimal>() {
                return price;
            }
        }
    }
    Decimal::ZERO
}

/// Parse an `action=bouquet` response tolerantly: items may carry
/// `id` (int or string) and the display text under `message`, `package`
/// or `name`, optionally suffixed with ` - N Credits`.
pub fn parse_catalog(body: &str) -> Result<Vec<CatalogProduct>, ProviderError> {
    let items: Vec<serde_json::Value> = serde_json::from_str(body)
        .map_err(|e| ProviderError::Remote(format!("neo4k: malformed catalog ({e}): {body}")))?;
    let mut out = Vec::new();
    for item in items {
        let id = item.get("id").and_then(|v| {
            v.as_str()
                .map(String::from)
                .or_else(|| v.as_i64().map(|n| n.to_string()))
        });
        let Some(id) = id else {
            continue;
        };
        let message = item
            .get("message")
            .or_else(|| item.get("package"))
            .or_else(|| item.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let mut name = message;
        if let Some(idx) = message.rfind(" - ") {
            let tail = &message[idx + 3..];
            let tail = tail
                .strip_suffix(" Credits")
                .or_else(|| tail.strip_suffix(" Credit"));
            if tail.is_some_and(|t| t.trim().parse::<Decimal>().is_ok()) {
                name = &message[..idx];
            }
        }
        out.push(CatalogProduct {
            external_pack_id: id,
            name: name.trim().to_string(),
            duration_months: 1,
            price: catalog_price(message),
            category: None,
        });
    }
    Ok(out)
}

/// Build the streaming m3u url: the panel's `url` when present, otherwise a
/// synthesized `https://{dns}:{port}/get.php?username=..&password=..`.
pub fn build_m3u_url(
    full_url: &str,
    dns: &str,
    port: u16,
    username: &str,
    password: &str,
) -> String {
    if !full_url.is_empty() {
        return full_url.to_string();
    }
    format!("https://{dns}:{port}/get.php?username={username}&password={password}")
}

/// Parse an `action=new` response into a provisioned credential.
/// The panel returns a bare object or a one-element list; `status` may be a
/// string (`"success"`/`"error"`) or boolean. The caller fills `expires_at`
/// (computed from the variant duration).
pub fn parse_new_response(
    body: &str,
    dns_domain: &str,
    port: u16,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<ProvisionedCredential, ProviderError> {
    let raw: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| ProviderError::Remote(format!("neo4k: malformed response ({e}): {body}")))?;
    let result = super::unwrap_response_object(raw, "neo4k")?;
    let parsed: CmsNewLine = serde_json::from_value(result)
        .map_err(|e| ProviderError::Remote(format!("neo4k: malformed line ({e}): {body}")))?;
    if parsed.status.is_error() {
        let msg = if !parsed.message.is_empty() {
            parsed.message
        } else {
            parsed.result
        };
        return Err(ProviderError::Remote(format!(
            "Provider error: {}",
            if msg.is_empty() {
                "line creation failed".to_string()
            } else {
                msg
            }
        )));
    }
    if parsed.user_id.as_string().is_empty() {
        return Err(ProviderError::Remote(
            "Missing 'user_id' in provider response".to_string(),
        ));
    }

    // username/password live in the m3u url query string; fall back to
    // user_id, and password defaults to username (Django parity).
    let mut username = parsed.user_id.as_string();
    let mut password = String::new();
    if !parsed.url.is_empty() {
        let query: std::collections::HashMap<String, String> = Url::parse(&parsed.url)
            .map_err(|e| ProviderError::Remote(format!("neo4k: bad m3u url: {e}")))?
            .query_pairs()
            .into_owned()
            .collect();
        username = query
            .get("username")
            .filter(|s| !s.is_empty())
            .cloned()
            .unwrap_or_else(|| parsed.user_id.as_string());
        password = query.get("password").cloned().unwrap_or_default();
    }
    if password.is_empty() {
        password = username.clone();
    }

    let m3u_url = build_m3u_url(&parsed.url, dns_domain, port, &username, &password);
    let dns = dns_from_m3u_url(&m3u_url).unwrap_or_else(|_| format!("https://{dns_domain}:{port}"));

    Ok(ProvisionedCredential {
        username,
        password,
        dns: Some(dns),
        m3u_url: Some(m3u_url),
        expires_at,
        extra: serde_json::json!({
            "provider": "neo4k",
            "user_id": parsed.user_id.as_string(),
            "status": parsed.status,
            "message": parsed.message,
            "result": parsed.result,
        }),
    })
}

/// Real HTTP adapter for CMS-only panels (neo4k).
/// Only reachable when USE_MOCK_PROVIDER=false (see factory.rs).
#[derive(Debug, Clone)]
pub struct CmsOnlyAdapter {
    api_endpoint: String,
    api_key: String,
    client: reqwest::Client,
}

impl CmsOnlyAdapter {
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
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Remote(format!("{url}: http {status}: {body}")));
        }
        resp.text()
            .await
            .map_err(|e| ProviderError::Request(format!("bad response from {url}: {e}")))
    }
}

#[async_trait]
impl ProviderAdapter for CmsOnlyAdapter {
    fn name(&self) -> &'static str {
        "neo4k"
    }

    async fn check_device(&self, _mac: &str) -> Result<DeviceCheckResult, ProviderError> {
        Err(ProviderError::Unsupported(
            "neo4k: device check not available (M3U only)".to_string(),
        ))
    }

    async fn provision(
        &self,
        ctx: &ProvisionContext,
    ) -> Result<ProvisionedCredential, ProviderError> {
        let pack_id = ctx.external_pack_id.ok_or_else(|| {
            ProviderError::Request("neo4k: product has no external_pack_id".to_string())
        })?;
        let sub = super::duration_to_sub(ctx.duration_months);
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
        let url = build_catalog_url(&self.api_endpoint, &self.api_key)?;
        let body = self.get_text(&url).await?;
        parse_catalog(&body)
    }
}

// Parse-only unit tests: no network involved.
#[cfg(test)]
mod tests {
    use super::*;

    const ENDPOINT: &str = "https://panel.neo4k.example.com/api.php";

    #[test]
    fn catalog_parses_int_and_string_ids() {
        let body = r#"[{"id":19,"message":"Gold - 1 Months - 2 Credits"},{"id":"20","package":"Silver"},{"id":21,"name":"Bronze - 1 Credit"}]"#;
        let catalog = parse_catalog(body).unwrap();
        assert_eq!(catalog.len(), 3);
        assert_eq!(catalog[0].external_pack_id, "19");
        assert_eq!(catalog[0].name, "Gold - 1 Months");
        assert_eq!(catalog[0].price.to_string(), "2");
        assert_eq!(catalog[1].external_pack_id, "20");
        assert_eq!(catalog[1].name, "Silver");
        assert_eq!(catalog[2].name, "Bronze");
        assert_eq!(catalog[2].price.to_string(), "1");
    }

    #[test]
    fn catalog_parses_real_bouquet_shape() {
        let body =
            r#"[{"id":"132","name":"SMALL - ARABIC"},{"id":"152","name":"Canada without adult"}]"#;
        let catalog = parse_catalog(body).unwrap();
        assert_eq!(catalog.len(), 2);
        assert_eq!(catalog[0].external_pack_id, "132");
        assert_eq!(catalog[0].name, "SMALL - ARABIC");
        assert_eq!(catalog[0].price.to_string(), "0");
        assert_eq!(catalog[1].name, "Canada without adult");
    }

    #[test]
    fn new_url_contains_expected_params() {
        let url = build_new_url(ENDPOINT, "KEY", 3, 42, None, Some("uuid-1"))
            .unwrap()
            .to_string();
        assert!(url.starts_with(ENDPOINT));
        // Documented order: action, type, sub, pack, optional country/notes, api_key last.
        assert!(url.contains("action=new&type=m3u&sub=3&pack=42&notes=uuid-1&api_key=KEY"));

        let lifetime = build_new_url(ENDPOINT, "K", 0, 1, Some("US"), None)
            .unwrap()
            .to_string();
        assert!(lifetime.contains("sub=0"));
        assert!(lifetime.contains("country=US"));
        assert!(lifetime.contains("&api_key=K"));
        assert!(!lifetime.contains("notes="));
    }

    #[test]
    fn response_parsed_with_url_credentials() {
        let body = r#"{"status":"success","user_id":"u123","url":"https://stream.example.com/get.php?username=u123&password=p456"}"#;
        let cred = parse_new_response(body, "kmapp.xyz", 8080, None).unwrap();
        assert_eq!(cred.username, "u123");
        assert_eq!(cred.password, "p456");
        assert_eq!(
            cred.m3u_url.as_deref(),
            Some("https://stream.example.com/get.php?username=u123&password=p456")
        );
        assert_eq!(cred.dns.as_deref(), Some("https://stream.example.com"));
        assert!(cred.expires_at.is_none());
    }

    #[test]
    fn response_without_url_synthesizes_m3u() {
        let body = r#"{"status":"success","user_id":"u123","url":""}"#;
        let cred = parse_new_response(body, "kmapp.xyz", 8080, None).unwrap();
        assert_eq!(cred.username, "u123");
        assert_eq!(cred.password, "u123", "password falls back to username");
        assert_eq!(
            cred.m3u_url.as_deref(),
            Some("https://kmapp.xyz:8080/get.php?username=u123&password=u123")
        );
    }

    #[test]
    fn error_status_is_remote_error() {
        let err = parse_new_response(r#"{"status":"error","message":"bad key"}"#, "dns", 1, None)
            .unwrap_err();
        assert!(err.to_string().contains("Provider error: bad key"));

        let err = parse_new_response(r#"{"status":"error","result":"quota"}"#, "dns", 1, None)
            .unwrap_err();
        assert!(err.to_string().contains("quota"));
    }

    #[test]
    fn array_wrapped_response_with_boolean_status_parsed() {
        let body = r#"[{"status":true,"user_id":777001,"url":"https://stream.example.com/get.php?username=u123&password=p456"}]"#;
        let cred = parse_new_response(body, "kmapp.xyz", 8080, None).unwrap();
        assert_eq!(cred.username, "u123");
        assert_eq!(cred.password, "p456");
        assert_eq!(cred.extra["user_id"], "777001");

        let err = parse_new_response(
            r#"[{"status":false,"message":"no credit"}]"#,
            "dns",
            1,
            None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("no credit"));
    }

    #[test]
    fn missing_user_id_is_remote_error() {
        assert!(parse_new_response(r#"{"status":"success"}"#, "dns", 1, None).is_err());
    }

    #[test]
    fn defaults_used_from_extra_config() {
        assert_eq!(default_dns_domain(&serde_json::json!({})), "kmapp.xyz");
        assert_eq!(
            default_dns_domain(&serde_json::json!({"dns_domain": "ott.example.net"})),
            "ott.example.net"
        );
        assert_eq!(default_port(&serde_json::json!({})), 8080);
        assert_eq!(default_port(&serde_json::json!({"port": 8443})), 8443);
    }
}
