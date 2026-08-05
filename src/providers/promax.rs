//! ProMax reseller API — URL building and response parsing.
//! The pure helpers below do NO network calls and are fully unit-tested;
//! only the `PromaxAdapter` at the bottom performs GETs (and it is only
//! reachable when USE_MOCK_PROVIDER=false, see factory.rs).
//! See the locked plan:
//!   create  : GET api.php?action=new&type=m3u&sub=<1|3|6|12|0>&pack=<BOUQUET id>&notes=..&adult=0&country=..&api_key=..
//!   renew   : GET ?action=renew&type=m3u&username=..&password=..&sub=..
//!   delete  : GET ?action=delete&type=m3u&username=..
//!   catalog : GET ?action=bouquet&public=1 -> [{id, name}] (pack ids)
//! The panel returns the full m3u url in the create response; the dns is
//! extracted from that url (scheme://host[:port]).

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
pub struct PromaxNewLine {
    pub status: StatusValue,
    #[serde(default)]
    pub user_id: StringOrNum,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub url: String,
}

/// scheme://host[:port] extracted from a full m3u url like
/// `http://reseller-domain.com/get.php?username=..&password=..&type=m3u_plus&output=ts`.
pub fn dns_from_m3u_url(m3u_url: &str) -> Result<String, ProviderError> {
    let url = Url::parse(m3u_url)
        .map_err(|e| ProviderError::Request(format!("invalid m3u url '{m3u_url}': {e}")))?;
    let scheme = url.scheme();
    let host = url
        .host_str()
        .ok_or_else(|| ProviderError::Request(format!("m3u url '{m3u_url}' has no host")))?;
    let port = url.port().map(|p| format!(":{p}")).unwrap_or_default();
    Ok(format!("{scheme}://{host}{port}"))
}

/// Parse a `action=new` response into a provisioned credential. The m3u url
/// comes straight from the panel; dns is derived from it. The panel returns a
/// bare object or a one-element list and `status` as a string or boolean.
pub fn parse_new_response(
    body: &str,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<ProvisionedCredential, ProviderError> {
    let raw: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| ProviderError::Remote(format!("promax: malformed response ({e}): {body}")))?;
    let result = super::unwrap_response_object(raw, "promax")?;
    let parsed: PromaxNewLine = serde_json::from_value(result)
        .map_err(|e| ProviderError::Remote(format!("promax: malformed line ({e}): {body}")))?;
    if !parsed.status.is_true() {
        return Err(ProviderError::Remote(format!(
            "promax: {}",
            if parsed.message.is_empty() {
                "line creation failed".to_string()
            } else {
                parsed.message
            }
        )));
    }
    if parsed.url.is_empty() {
        return Err(ProviderError::Remote(
            "promax: line created without m3u url".to_string(),
        ));
    }
    let dns = dns_from_m3u_url(&parsed.url)?;
    // The panel's url embeds username=..&password=.. query params.
    let query: std::collections::HashMap<String, String> = Url::parse(&parsed.url)
        .map_err(|e| ProviderError::Remote(format!("promax: bad m3u url: {e}")))?
        .query_pairs()
        .into_owned()
        .collect();
    let username = query.get("username").cloned().unwrap_or_default();
    let password = query.get("password").cloned().unwrap_or_default();
    if username.is_empty() || password.is_empty() {
        return Err(ProviderError::Remote(
            "promax: m3u url missing username/password".to_string(),
        ));
    }
    Ok(ProvisionedCredential {
        username,
        password,
        dns: Some(dns),
        m3u_url: Some(parsed.url.clone()),
        expires_at,
        extra: serde_json::json!({
            "provider": "promax",
            "user_id": parsed.user_id.as_string(),
            "country": parsed.country,
            "notes": parsed.notes,
            "message": parsed.message,
        }),
    })
}

/// action=new URL. `sub` is the variant duration in months (0 = lifetime);
/// `pack` is the bouquet id chosen at checkout (template_id) or the product's
/// external_pack_id as fallback.
pub fn build_new_url(
    api_endpoint: &str,
    api_key: &str,
    sub: i32,
    pack: &str,
    notes: &str,
    country: Option<&str>,
) -> Result<Url, ProviderError> {
    let mut url = Url::parse(api_endpoint).map_err(|e| {
        ProviderError::Request(format!("invalid promax api_endpoint '{api_endpoint}': {e}"))
    })?;
    url.query_pairs_mut()
        .append_pair("action", "new")
        .append_pair("type", "m3u")
        .append_pair("sub", &sub.to_string())
        .append_pair("pack", pack)
        .append_pair("notes", notes)
        .append_pair("adult", "0");
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
    sub: i32,
) -> Result<Url, ProviderError> {
    let mut url = Url::parse(api_endpoint).map_err(|e| {
        ProviderError::Request(format!("invalid promax api_endpoint '{api_endpoint}': {e}"))
    })?;
    url.query_pairs_mut()
        .append_pair("action", "renew")
        .append_pair("type", "m3u")
        .append_pair("username", username)
        .append_pair("password", password)
        .append_pair("sub", &sub.to_string())
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
        ProviderError::Request(format!("invalid promax api_endpoint '{api_endpoint}': {e}"))
    })?;
    url.query_pairs_mut()
        .append_pair("action", "delete")
        .append_pair("type", "m3u")
        .append_pair("username", username)
        .append_pair("api_key", api_key);
    Ok(url)
}

/// action=bouquet&public=1 URL (catalog + /api/promax-bouquets/).
pub fn build_catalog_url(api_endpoint: &str, api_key: &str) -> Result<Url, ProviderError> {
    let mut url = Url::parse(api_endpoint).map_err(|e| {
        ProviderError::Request(format!("invalid promax api_endpoint '{api_endpoint}': {e}"))
    })?;
    url.query_pairs_mut()
        .append_pair("action", "bouquet")
        .append_pair("public", "1")
        .append_pair("api_key", api_key);
    Ok(url)
}

#[derive(Debug, Clone, serde::Serialize, Deserialize)]
pub struct PromaxBouquet {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
}

/// Parse a `action=bouquet` catalog response. Bouquet ids drive both product
/// sync (external_pack_id) and the checkout `pack` param.
pub fn parse_catalog(body: &str) -> Result<Vec<CatalogProduct>, ProviderError> {
    let bouquets: Vec<PromaxBouquet> = serde_json::from_str(body)
        .map_err(|e| ProviderError::Remote(format!("promax: malformed catalog ({e}): {body}")))?;
    Ok(bouquets
        .into_iter()
        .map(|b| CatalogProduct {
            external_pack_id: b.id,
            name: b.name,
            duration_months: 1,
            price: Decimal::ZERO,
            category: None,
        })
        .collect())
}

/// Real HTTP adapter for ProMax reseller panels.
/// Only reachable when USE_MOCK_PROVIDER=false (see factory.rs).
#[derive(Debug, Clone)]
pub struct PromaxAdapter {
    api_endpoint: String,
    api_key: String,
    client: reqwest::Client,
}

impl PromaxAdapter {
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
impl ProviderAdapter for PromaxAdapter {
    fn name(&self) -> &'static str {
        "promax"
    }

    async fn check_device(&self, _mac: &str) -> Result<DeviceCheckResult, ProviderError> {
        Err(ProviderError::Unsupported(
            "promax: device check not available (M3U only)".to_string(),
        ))
    }

    async fn provision(
        &self,
        ctx: &ProvisionContext,
    ) -> Result<ProvisionedCredential, ProviderError> {
        // Bouquet id comes from checkout (template_id); fall back to the
        // product's external_pack_id when the customer did not pick one.
        let pack = ctx
            .template_id
            .clone()
            .or_else(|| ctx.external_pack_id.map(|p| p.to_string()))
            .ok_or_else(|| {
                ProviderError::Request(
                    "promax: no bouquet selected (template_id) and product has no external_pack_id"
                        .to_string(),
                )
            })?;
        let sub = super::duration_to_sub(ctx.duration_months);
        let url = build_new_url(
            &self.api_endpoint,
            &self.api_key,
            sub,
            &pack,
            &ctx.order_id.to_string(),
            None,
        )?;
        let body = self.get_text(&url).await?;
        // Panels do not return an expiry; compute from the variant duration.
        let expires_at = ctx
            .duration_months
            .filter(|m| *m > 0)
            .map(|m| chrono::Utc::now() + chrono::Duration::days(30 * m as i64));
        parse_new_response(&body, expires_at)
    }

    async fn fetch_catalog(&self) -> Result<Vec<CatalogProduct>, ProviderError> {
        let url = build_catalog_url(&self.api_endpoint, &self.api_key)?;
        let body = self.get_text(&url).await?;
        parse_catalog(&body)
    }

    async fn fetch_options(&self, kind: &str) -> Result<serde_json::Value, ProviderError> {
        if kind != "bouquets" {
            return Err(ProviderError::Unsupported(format!(
                "{kind} not supported by promax"
            )));
        }
        let url = build_catalog_url(&self.api_endpoint, &self.api_key)?;
        let body = self.get_text(&url).await?;
        let bouquets: Vec<PromaxBouquet> = serde_json::from_str(&body).map_err(|e| {
            ProviderError::Remote(format!("promax: malformed bouquets ({e}): {body}"))
        })?;
        Ok(serde_json::to_value(bouquets)
            .map_err(|e| ProviderError::Request(format!("promax: bouquets serialization: {e}")))?)
    }
}

// Parse-only unit tests: no network involved.
#[cfg(test)]
mod tests {
    use super::*;

    const ENDPOINT: &str = "https://api.promax-dash.com/api.php";

    #[test]
    fn dns_extracted_from_m3u_url() {
        assert_eq!(
            dns_from_m3u_url(
                "http://reseller-domain.com/get.php?username=a&password=b&type=m3u_plus&output=ts"
            )
            .unwrap(),
            "http://reseller-domain.com"
        );
        assert_eq!(
            dns_from_m3u_url("https://panel.example.com:8443/get.php?username=a").unwrap(),
            "https://panel.example.com:8443"
        );
        assert!(dns_from_m3u_url("nope").is_err());
    }

    #[test]
    fn new_url_contains_expected_params() {
        let url = build_new_url(ENDPOINT, "KEY", 6, "42", "uuid-9", Some("FR"))
            .unwrap()
            .to_string();
        assert!(url.starts_with(ENDPOINT));
        // Documented order: action, type, sub, pack, notes, adult, country, api_key.
        assert!(url.contains("action=new&type=m3u&sub=6&pack=42&notes=uuid-9&adult=0&country=FR&api_key=KEY"));
    }

    #[test]
    fn new_response_parsed_into_credential() {
        let body = r#"{"status":"true","user_id":"123","notes":"n","country":"US","message":"ok","url":"http://reseller-domain.com/get.php?username=u1&password=p1&type=m3u_plus&output=ts"}"#;
        let cred = parse_new_response(body, None).unwrap();
        assert_eq!(cred.username, "u1");
        assert_eq!(cred.password, "p1");
        assert_eq!(cred.dns.as_deref(), Some("http://reseller-domain.com"));
        assert!(cred
            .m3u_url
            .unwrap()
            .starts_with("http://reseller-domain.com/get.php"));
        assert!(cred.expires_at.is_none());
        assert_eq!(cred.extra["user_id"], "123");
    }

    #[test]
    fn failed_response_is_remote_error() {
        let err = parse_new_response(r#"{"status":"false","message":"wrong api key"}"#, None)
            .unwrap_err();
        assert!(err.to_string().contains("wrong api key"));
    }

    #[test]
    fn array_wrapped_response_with_boolean_status_parsed() {
        let body = r#"[{"status":true,"user_id":12178130,"notes":"n1","message":"Add M3U successful","url":"http://reseller-domain.com/get.php?username=u1&password=p1&type=m3u_plus"}]"#;
        let cred = parse_new_response(body, None).unwrap();
        assert_eq!(cred.username, "u1");
        assert_eq!(cred.password, "p1");
        assert_eq!(cred.extra["user_id"], "12178130");
        assert!(parse_new_response(
            r#"[{"status":false,"message":"no credit"}]"#,
            None
        )
        .unwrap_err()
        .to_string()
        .contains("no credit"));
    }

    #[test]
    fn missing_url_is_remote_error() {
        assert!(parse_new_response(r#"{"status":"true","message":"ok"}"#, None).is_err());
    }

    #[test]
    fn catalog_parsed_to_bouquet_products() {
        let body = r#"[{"id":"10","name":"Family Pack"},{"id":"11","name":"Sports Pack"}]"#;
        let catalog = parse_catalog(body).unwrap();
        assert_eq!(catalog.len(), 2);
        assert_eq!(catalog[0].external_pack_id, "10");
        assert_eq!(catalog[1].name, "Sports Pack");
    }
}
