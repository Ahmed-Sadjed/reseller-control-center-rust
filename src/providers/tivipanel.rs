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
    ProviderAdapter, StatusValue, StringOrNum,
};

#[derive(Debug, Deserialize)]
pub struct TiviNewLine {
    pub status: StatusValue,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub credits: Option<StringOrNum>,
    #[serde(default)]
    pub cost: Option<StringOrNum>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub message: String,
}

/// scheme://host[:port] extracted from an api_endpoint like
/// `https://api.tivipanel.net/reseller/panel_api.php`.
pub fn base_dns_from_api_url(api_endpoint: &str) -> Result<String, ProviderError> {
    let url = Url::parse(api_endpoint).map_err(|e| {
        ProviderError::Request(format!("invalid api_endpoint '{api_endpoint}': {e}"))
    })?;
    let scheme = url.scheme();
    let host = url.host_str().ok_or_else(|| {
        ProviderError::Request(format!("api_endpoint '{api_endpoint}' has no host"))
    })?;
    let port = url.port().map(|p| format!(":{p}")).unwrap_or_default();
    Ok(format!("{scheme}://{host}{port}"))
}

/// Construct the streaming m3u URL for a line: {dns}/get.php?username=..&password=..
pub fn build_m3u_url(dns: &str, username: &str, password: &str) -> String {
    format!("{dns}/get.php?username={username}&password={password}")
}

/// Parse a `action=new` response into a provisioned credential. The panel
/// returns a bare object or a one-element list; `status` may be a string or
/// boolean and `credits`/`cost` strings or numbers. The caller fills
/// `expires_at` (computed from the variant duration when absent).
pub fn parse_new_response(
    body: &str,
    dns: &str,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<ProvisionedCredential, ProviderError> {
    let raw: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        ProviderError::Remote(format!("tivipanel: malformed response ({e}): {body}"))
    })?;
    let result = super::unwrap_response_object(raw, "tivipanel")?;
    let parsed: TiviNewLine = serde_json::from_value(result)
        .map_err(|e| ProviderError::Remote(format!("tivipanel: malformed line ({e}): {body}")))?;
    if !parsed.status.is_true() {
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
            "credits": parsed.credits.as_ref().map(StringOrNum::as_string).unwrap_or_default(),
            "cost": parsed.cost.as_ref().map(StringOrNum::as_string).unwrap_or_default(),
            "notes": parsed.notes,
            "message": parsed.message,
        }),
    })
}

/// action=new URL. `package` is the variant duration in months (0 = trial/lifetime).
pub fn build_new_url(
    api_endpoint: &str,
    api_key: &str,
    package: i32,
    template: Option<&str>,
    notes: &str,
    country: Option<&str>,
) -> Result<Url, ProviderError> {
    let mut url = Url::parse(api_endpoint).map_err(|e| {
        ProviderError::Request(format!(
            "invalid tivipanel api_endpoint '{api_endpoint}': {e}"
        ))
    })?;
    url.query_pairs_mut()
        .append_pair("notes", notes)
        .append_pair("action", "new")
        .append_pair("type", "m3u");
    if let Some(t) = template {
        url.query_pairs_mut().append_pair("template", t);
    }
    url.query_pairs_mut().append_pair("package", &package.to_string());
    if let Some(c) = country {
        url.query_pairs_mut().append_pair("country", c);
    }
    url.query_pairs_mut().append_pair("api_key", api_key);
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
        ProviderError::Request(format!(
            "invalid tivipanel api_endpoint '{api_endpoint}': {e}"
        ))
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
        ProviderError::Request(format!(
            "invalid tivipanel api_endpoint '{api_endpoint}': {e}"
        ))
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
        ProviderError::Request(format!(
            "invalid tivipanel api_endpoint '{api_endpoint}': {e}"
        ))
    })?;
    url.query_pairs_mut()
        .append_pair("action", "package")
        .append_pair("api_key", api_key);
    Ok(url)
}

#[derive(Debug, Deserialize)]
pub struct TiviPackage {
    pub id: i64,
    #[serde(default)]
    pub message: String,
}

fn split_message(message: &str) -> (String, Decimal) {
    if let Some(idx) = message.rfind(" - ") {
        let tail = &message[idx + 3..];
        let tail = tail
            .strip_suffix(" Credits")
            .or_else(|| tail.strip_suffix(" Credit"));
        if let Some(credits) = tail {
            if let Ok(price) = credits.trim().parse::<Decimal>() {
                return (message[..idx].trim().to_string(), price);
            }
        }
    }
    (message.to_string(), Decimal::ZERO)
}

fn duration_from_name(name: &str) -> Option<i32> {
    let lower = name.to_lowercase();
    for unit in ["year", "month", "day"] {
        if let Some(pos) = lower.find(unit) {
            let prefix = &lower[..pos];
            if let Some(end) = prefix.rfind(|c: char| c.is_ascii_digit()) {
                let mut start = end;
                while start > 0 && prefix.as_bytes()[start - 1].is_ascii_digit() {
                    start -= 1;
                }
                let n: i32 = prefix[start..=end].parse().ok()?;
                return match unit {
                    "year" => Some(n * 12),
                    "month" => Some(n),
                    _ => Some(1),
                };
            }
        }
    }
    None
}

/// Parse a `action=package` catalog response. The panel returns
/// [{id, message: "Name - N Credits"}]; name, price and duration are
/// extracted from the message text.
pub fn parse_catalog(body: &str) -> Result<Vec<CatalogProduct>, ProviderError> {
    let packages: Vec<TiviPackage> = serde_json::from_str(body).map_err(|e| {
        ProviderError::Remote(format!("tivipanel: malformed catalog ({e}): {body}"))
    })?;
    Ok(packages
        .into_iter()
        .map(|p| {
            let (name, price) = split_message(&p.message);
            CatalogProduct {
                external_pack_id: p.id.to_string(),
                name,
                duration_months: duration_from_name(&p.message).unwrap_or(1),
                price,
                category: None,
            }
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
        let status = resp.status();
        if !status.is_success() {
            // The panel's 400 body carries the real reason (missing/unknown
            // package, template, api_key, ...); surface it instead of a bare
            // status code so failures are diagnosable.
            let body = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Remote(format!(
                "{url}: http {status}: {body}"
            )));
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
        let package = super::duration_to_sub(ctx.duration_months);
        let url = build_new_url(
            &self.api_endpoint,
            &self.api_key,
            package,
            ctx.template_id.as_deref(),
            &ctx.order_id.to_string(),
            // The panel rejects the request when `country` is absent (the docs
            // mark it optional but the panel requires it); "ALL" is the
            // documented value meaning no geo-lock/VPN.
            Some("ALL"),
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
    fn catalog_parses_real_panel_shape() {
        let body = r#"[{"id":19,"message":"Official (1 Year) - 1 Credits"},{"id":20,"message":"Official (6 Months) - 0.5 Credits"},{"id":21,"message":"Official (3 Months) - 0.25 Credits"},{"id":34,"message":"Official (1 Months) - 0.10 Credits"},{"id":35,"message":"TIVI ONE - Trial (1 Days) - 0 Credits"}]"#;
        let catalog = parse_catalog(body).unwrap();
        assert_eq!(catalog.len(), 5);
        assert_eq!(catalog[0].external_pack_id, "19");
        assert_eq!(catalog[0].name, "Official (1 Year)");
        assert_eq!(catalog[0].duration_months, 12);
        assert_eq!(catalog[0].price, rust_decimal::Decimal::ONE);
        assert_eq!(catalog[1].duration_months, 6);
        assert_eq!(catalog[2].price.to_string(), "0.25");
        assert_eq!(catalog[4].name, "TIVI ONE - Trial (1 Days)");
        assert_eq!(catalog[4].price, rust_decimal::Decimal::ZERO);
    }

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
    fn hour_codes_map_to_trial_package() {
        assert_eq!(super::super::duration_to_sub(Some(102)), 0);
        assert_eq!(super::super::duration_to_sub(Some(100)), 0);
        assert_eq!(super::super::duration_to_sub(Some(103)), 0);
        assert_eq!(super::super::duration_to_sub(None), 0);
        assert_eq!(super::super::duration_to_sub(Some(0)), 0);
        assert_eq!(super::super::duration_to_sub(Some(1)), 1);
        assert_eq!(super::super::duration_to_sub(Some(3)), 3);
        assert_eq!(super::super::duration_to_sub(Some(6)), 6);
        assert_eq!(super::super::duration_to_sub(Some(12)), 12);
    }

    #[test]
    fn new_url_contains_expected_params() {
        let url = build_new_url(ENDPOINT, "KEY123", 3, Some("tpl42"), "uuid-1", Some("US"))
            .unwrap()
            .to_string();
        assert!(url.starts_with(ENDPOINT));
        // Parameter order must match the panel's documented example:
        // notes, action, type, template, package, country, api_key.
        assert!(url.contains("panel_api.php?notes=uuid-1&action=new&type=m3u&template=tpl42&package=3&country=US&api_key=KEY123"));
        assert!(url.contains("action=new"));
        assert!(url.contains("type=m3u"));
        assert!(url.contains("package=3"));
        assert!(url.contains("template=tpl42"));
        assert!(url.contains("notes=uuid-1"));
        assert!(url.contains("country=US"));
        assert!(url.contains("api_key=KEY123"));

        let lifetime = build_new_url(ENDPOINT, "K", 0, None, "n", None)
            .unwrap()
            .to_string();
        assert!(lifetime.contains("panel_api.php?notes=n&action=new&type=m3u&package=0&api_key=K"));
        assert!(!lifetime.contains("template"));
        assert!(!lifetime.contains("country"));
    }

    #[test]
    fn array_wrapped_response_with_typed_fields_parsed() {
        let body = r#"[{"status":true,"username":"23M2ZX","password":"2T3B9A","country":"[\"ALL\"]","credits":6.9,"cost":0,"message":"M3U Added Successfully !"}]"#;
        let cred = parse_new_response(body, "https://api.tivipanel.net", None).unwrap();
        assert_eq!(cred.username, "23M2ZX");
        assert_eq!(cred.password, "2T3B9A");
        assert_eq!(cred.dns.as_deref(), Some("https://api.tivipanel.net"));
        assert_eq!(cred.extra["credits"], "6.9");
        assert_eq!(cred.extra["cost"], "0");
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
        let err = parse_new_response(
            r#"{"status":"false","message":"invalid api key"}"#,
            "dns",
            None,
        )
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
        let body = r#"[{"id":5,"message":"1 Month"},{"id":7,"message":"12 Months - 3 Credits"}]"#;
        let catalog = parse_catalog(body).unwrap();
        assert_eq!(catalog.len(), 2);
        assert_eq!(catalog[0].external_pack_id, "5");
        assert_eq!(catalog[0].name, "1 Month");
        assert_eq!(catalog[0].duration_months, 1);
        assert_eq!(catalog[1].name, "12 Months");
        assert_eq!(catalog[1].duration_months, 12);
        assert_eq!(catalog[1].price.to_string(), "3");
    }
}
