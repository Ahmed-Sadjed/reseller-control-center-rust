//! TiviPanel reseller API — URL building and response parsing.
//! The pure helpers below do NO network calls and are fully unit-tested;
//! only the `TiviPanelAdapter` at the bottom performs GETs (and it is only
//! reachable when USE_MOCK_PROVIDER=false, see factory.rs).
//! See the locked plan:
//!   create  : GET panel_api.php?action=new&type=m3u&package=<0|1|3|6|12>&template=..&notes=<order uuid>&country=..&api_key=..
//!   renew   : GET ?action=renew&type=m3u&username=..&password=..&package=..
//!   delete  : GET ?action=delete&type=m3u&username=..
//!   catalog : GET ?action=package (names for display/sync ONLY, never sent at purchase)
//! The M3U streaming endpoint lives on the same domain as the API, so the
//! DNS is derived from the api_endpoint (scheme://host[:port]) and the m3u
//! URL is constructed as {dns}/get.php?username=..&password=..

use async_trait::async_trait;
use reqwest::Url;
use rust_decimal::Decimal;
use serde::Deserialize;

use super::{
    error::ProviderError,
    types::{CatalogProduct, DeviceCheckResult, ProvisionContext, ProvisionedCredential},
    ProviderAdapter,
};

#[derive(Debug, Deserialize)]
pub struct TiviNewLine {
    pub status: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub credits: String,
    #[serde(default)]
    pub cost: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub message: String,
}

/// scheme://host[:port] extracted from an api_endpoint like
/// `https://api.tivipanel.net/reseller/panel_api.php`.
pub fn base_dns_from_api_url(api_endpoint: &str) -> Result<String, ProviderError> {
    let url = Url::parse(api_endpoint)
        .map_err(|e| ProviderError::Request(format!("invalid api_endpoint '{api_endpoint}': {e}")))?;
    let scheme = url.scheme();
    let host = url
        .host_str()
        .ok_or_else(|| ProviderError::Request(format!("api_endpoint '{api_endpoint}' has no host")))?;
    let port = url
        .port()
        .map(|p| format!(":{p}"))
        .unwrap_or_default();
    Ok(format!("{scheme}://{host}{port}"))
}

/// Construct the streaming m3u URL for a line: {dns}/get.php?username=..&password=..
pub fn build_m3u_url(dns: &str, username: &str, password: &str) -> String {
    format!("{dns}/get.php?username={username}&password={password}")
}

/// Parse a `action=new` response into a provisioned credential. The caller
/// fills `expires_at` (computed from the variant duration when absent).
pub fn parse_new_response(
    body: &str,
    dns: &str,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<ProvisionedCredential, ProviderError> {
    let parsed: TiviNewLine = serde_json::from_str(body).map_err(|e| {
        ProviderError::Remote(format!("tivipanel: malformed response ({e}): {body}"))
    })?;
    if parsed.status != "true" {
        return Err(ProviderError::Remote(format!(
            "tivipanel: {}",
            if parsed.message.is_empty() {
                "line creation failed".to_string()
            } else {
                parsed.message
            }
        )));
    }
    if parsed.username.is_empty() || parsed.password.is_empty() {
        return Err(ProviderError::Remote(
            "tivipanel: line created without username/password".to_string(),
        ));
    }
    let m3u_url = build_m3u_url(dns, &parsed.username, &parsed.password);
    Ok(ProvisionedCredential {
        username: parsed.username,
        password: parsed.password,
        dns: Some(dns.to_string()),
        m3u_url: Some(m3u_url),
        expires_at,
        extra: serde_json::json!({
            "provider": "tivipanel",
            "country": parsed.country,
            "credits": parsed.credits,
            "cost": parsed.cost,
            "notes": parsed.notes,
            "message": parsed.message,
        }),
    })
}

/// action=new URL. `package` is the variant duration in months (0 = lifetime).
pub fn build_new_url(
    api_endpoint: &str,
    api_key: &str,
    package: i32,
    template: Option<&str>,
    notes: &str,
    country: Option<&str>,
) -> Result<Url, ProviderError> {
    let mut url = Url::parse(api_endpoint).map_err(|e| {
        ProviderError::Request(format!("invalid tivipanel api_endpoint '{api_endpoint}': {e}"))
    })?;
    url.query_pairs_mut()
        .append_pair("action", "new")
        .append_pair("type", "m3u")
        .append_pair("package", &package.to_string())
        .append_pair("api_key", api_key);
    if let Some(t) = template {
        url.query_pairs_mut().append_pair("template", t);
    }
    url.query_pairs_mut().append_pair("notes", notes);
    if let Some(c) = country {
        url.query_pairs_mut().append_pair("country", c);
    }
    Ok(url)
}

/// action=renew URL (line extension).
pub fn build_renew_url(
    api_endpoint: &str,
    api_key: &str,
    username: &str,
    password: &str,
    package: i32,
) -> Result<Url, ProviderError> {
    let mut url = Url::parse(api_endpoint).map_err(|e| {
        ProviderError::Request(format!("invalid tivipanel api_endpoint '{api_endpoint}': {e}"))
    })?;
    url.query_pairs_mut()
        .append_pair("action", "renew")
        .append_pair("type", "m3u")
        .append_pair("username", username)
        .append_pair("password", password)
        .append_pair("package", &package.to_string())
        .append_pair("api_key", api_key);
    Ok(url)
}

/// action=delete URL.
pub fn build_delete_url(
    api_endpoint: &str,
    api_key: &str,
    username: &str,
) -> Result<Url, ProviderError> {
    let mut url = Url::parse(api_endpoint).map_err(|e| {
        ProviderError::Request(format!("invalid tivipanel api_endpoint '{api_endpoint}': {e}"))
    })?;
    url.query_pairs_mut()
        .append_pair("action", "delete")
        .append_pair("type", "m3u")
        .append_pair("username", username)
        .append_pair("api_key", api_key);
    Ok(url)
}

/// action=package URL (catalog: display names only).
pub fn build_catalog_url(api_endpoint: &str, api_key: &str) -> Result<Url, ProviderError> {
    let mut url = Url::parse(api_endpoint).map_err(|e| {
        ProviderError::Request(format!("invalid tivipanel api_endpoint '{api_endpoint}': {e}"))
    })?;
    url.query_pairs_mut()
        .append_pair("action", "package")
        .append_pair("api_key", api_key);
    Ok(url)
}

#[derive(Debug, Deserialize)]
pub struct TiviPackage {
    pub id: String,
    #[serde(default)]
    pub package: String,
}

/// Parse a `action=package` catalog response. The panel returns [{id, package}];
/// the package name is for display only and durations/prices are unknown, so
/// sync upserts them with a placeholder 1-month/0-price variant (admin edits).
pub fn parse_catalog(body: &str) -> Result<Vec<CatalogProduct>, ProviderError> {
    let packages: Vec<TiviPackage> = serde_json::from_str(body).map_err(|e| {
        ProviderError::Remote(format!("tivipanel: malformed catalog ({e}): {body}"))
    })?;
    Ok(packages
        .into_iter()
        .map(|p| CatalogProduct {
            external_pack_id: p.id,
            name: p.package,
            duration_months: 1,
            price: Decimal::ZERO,
            category: None,
        })
        .collect())
}

/// Real HTTP adapter for TiviPanel reseller panels.
/// Only reachable when USE_MOCK_PROVIDER=false (see factory.rs).
#[derive(Debug, Clone)]
pub struct TiviPanelAdapter {
    api_endpoint: String,
    api_key: String,
    client: reqwest::Client,
}

impl TiviPanelAdapter {
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
            return Err(ProviderError::Remote(format!("{url}: http {}", resp.status())));
        }
        resp.text()
            .await
            .map_err(|e| ProviderError::Request(format!("bad response from {url}: {e}")))
    }
}

#[async_trait]
impl ProviderAdapter for TiviPanelAdapter {
    fn name(&self) -> &'static str {
        "tivipanel"
    }

    async fn check_device(&self, _mac: &str) -> Result<DeviceCheckResult, ProviderError> {
        Err(ProviderError::Unsupported(
            "tivipanel: device check not available (M3U only)".to_string(),
        ))
    }

    async fn provision(
        &self,
        ctx: &ProvisionContext,
    ) -> Result<ProvisionedCredential, ProviderError> {
        let package = ctx.duration_months.unwrap_or(0);
        let url = build_new_url(
            &self.api_endpoint,
            &self.api_key,
            package,
            ctx.template_id.as_deref(),
            &ctx.order_id.to_string(),
            None,
        )?;
        let body = self.get_text(&url).await?;
        let dns = base_dns_from_api_url(&self.api_endpoint)?;
        // Panels do not return an expiry; compute from the variant duration.
        let expires_at = ctx
            .duration_months
            .filter(|m| *m > 0)
            .map(|m| chrono::Utc::now() + chrono::Duration::days(30 * m as i64));
        parse_new_response(&body, &dns, expires_at)
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

    const ENDPOINT: &str = "https://api.tivipanel.net/reseller/panel_api.php";
    #[test]
    fn dns_derived_from_api_endpoint() {
        assert_eq!(
            base_dns_from_api_url(ENDPOINT).unwrap(),
            "https://api.tivipanel.net"
        );
        assert_eq!(
            base_dns_from_api_url("https://panel.example.com:8443/api.php").unwrap(),
            "https://panel.example.com:8443"
        );
        assert!(base_dns_from_api_url("not a url").is_err());
    }

    #[test]
    fn new_url_contains_expected_params() {
        let url = build_new_url(ENDPOINT, "KEY123", 3, Some("tpl42"), "uuid-1", Some("US"))
            .unwrap()
            .to_string();
        assert!(url.starts_with(ENDPOINT));
        assert!(url.contains("action=new"));
        assert!(url.contains("type=m3u"));
        assert!(url.contains("package=3"));
        assert!(url.contains("template=tpl42"));
        assert!(url.contains("notes=uuid-1"));
        assert!(url.contains("country=US"));
        assert!(url.contains("api_key=KEY123"));

        let lifetime = build_new_url(ENDPOINT, "K", 0, None, "n", None).unwrap().to_string();
        assert!(lifetime.contains("package=0"));
        assert!(!lifetime.contains("template"));
        assert!(!lifetime.contains("country"));
    }

    #[test]
    fn new_response_parsed_into_credential() {
        let body = r#"{"status":"true","username":"user_x","password":"pass_y","country":"US","credits":"100","cost":"3.00","notes":"uuid-1","message":"ok"}"#;
        let cred = parse_new_response(body, "https://api.tivipanel.net", None).unwrap();
        assert_eq!(cred.username, "user_x");
        assert_eq!(cred.password, "pass_y");
        assert_eq!(cred.dns.as_deref(), Some("https://api.tivipanel.net"));
        assert_eq!(
            cred.m3u_url.as_deref(),
            Some("https://api.tivipanel.net/get.php?username=user_x&password=pass_y")
        );
        assert!(cred.expires_at.is_none());
        assert_eq!(cred.extra["country"], "US");
    }

    #[test]
    fn failed_response_is_remote_error() {
        let err = parse_new_response(r#"{"status":"false","message":"invalid api key"}"#, "dns", None)
            .unwrap_err();
        assert!(err.to_string().contains("invalid api key"));
    }

    #[test]
    fn malformed_response_is_remote_error() {
        assert!(parse_new_response("not json", "dns", None).is_err());
        assert!(parse_new_response(r#"{"status":"true"}"#, "dns", None).is_err());
    }

    #[test]
    fn catalog_parsed_to_products() {
        let body = r#"[{"id":"5","package":"1 Month"},{"id":"7","package":"12 Months"}]"#;
        let catalog = parse_catalog(body).unwrap();
        assert_eq!(catalog.len(), 2);
        assert_eq!(catalog[0].external_pack_id, "5");
        assert_eq!(catalog[0].name, "1 Month");
    }
}
