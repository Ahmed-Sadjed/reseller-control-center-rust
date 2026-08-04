//! Golden OTT (Golden API) reseller API — JSON REST with `X-API-Key`.
//! The pure helpers below do NO network calls and are fully unit-tested;
//! only the `GoldenApiAdapter` at the bottom performs requests (and it is
//! only reachable when USE_MOCK_PROVIDER=false, see factory.rs).
//! Mirrors backend/api/providers/golden_api.py:
//!   templates : GET /account/templates -> {"data": {"global": [...]}}
//!   domains   : GET /account/domains   -> {"data": [...]}
//!   create    : POST /lines            -> {"data": [line, ...],
//!              "package": {...}, "template": {...}, "qr": {...}, "exp_date": ...}
//! Expiry strings are "%Y-%m-%d %H:%M:%S" or "%Y-%m-%d" (clamped to 23:59:59 UTC).

use async_trait::async_trait;
use serde_json::{json, Value};
use uuid::Uuid;

use super::{
    error::ProviderError,
    types::{CatalogProduct, DeviceCheckResult, ProvisionContext, ProvisionedCredential},
    ProviderAdapter,
};

/// Parse `GET /account/templates` payload: return `data.global` (empty when absent).
pub fn parse_templates(payload: &Value) -> Result<Value, ProviderError> {
    Ok(payload
        .get("data")
        .and_then(|d| d.get("global"))
        .and_then(|g| g.as_array())
        .cloned()
        .map(|a| json!(a))
        .unwrap_or_else(|| json!([])))
}

/// Parse `GET /account/domains` payload: return `data` (empty when absent).
pub fn parse_domains(payload: &Value) -> Result<Value, ProviderError> {
    Ok(payload
        .get("data")
        .and_then(|d| d.as_array())
        .cloned()
        .map(|a| json!(a))
        .unwrap_or_else(|| json!([])))
}

/// Parse an expiry value: "%Y-%m-%d %H:%M:%S" first, then "%Y-%m-%d"
/// (date-only clamped to 23:59:59 UTC, Django parity).
pub fn parse_exp_date(value: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    let v = value.trim();
    if v.is_empty() {
        return None;
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(v, "%Y-%m-%d %H:%M:%S") {
        return Some(chrono::DateTime::from_naive_utc_and_offset(dt, chrono::Utc));
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d") {
        let clamped = d
            .and_hms_opt(23, 59, 59)
            .unwrap_or_else(|| d.and_hms_opt(0, 0, 0).expect("valid hms"));
        return Some(chrono::DateTime::from_naive_utc_and_offset(
            clamped,
            chrono::Utc,
        ));
    }
    None
}

/// Parse a `POST /lines` response into a provisioned credential.
/// `username`/`password` are the values we sent (used as fallbacks), the
/// line itself is `data[0]` (Django: `line_data = data['data'][0]`).
pub fn parse_create_response(
    data: &Value,
    current_username: &str,
    final_password: &str,
) -> Result<ProvisionedCredential, ProviderError> {
    let line = data
        .get("data")
        .and_then(|d| d.as_array())
        .and_then(|arr| arr.first())
        .ok_or_else(|| {
            ProviderError::Remote("golden_api: create response missing data[0]".to_string())
        })?;

    let expires_at = line
        .get("exp_date")
        .and_then(|v| v.as_str())
        .and_then(parse_exp_date)
        .or_else(|| {
            data.get("exp_date")
                .and_then(|v| v.as_str())
                .and_then(parse_exp_date)
        });

    let username = line
        .get("username")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(current_username)
        .to_string();

    Ok(ProvisionedCredential {
        username: username.clone(),
        password: final_password.to_string(),
        dns: None,
        m3u_url: None,
        expires_at,
        extra: json!({
            "provider": "golden_api",
            "username": username,
            "line_id": line.get("id").cloned().unwrap_or(json!(null)),
            "package": data.get("package").and_then(|p| p.get("name")).cloned().unwrap_or(json!("")),
            "template_name": data.get("template").and_then(|t| t.get("name")).cloned().unwrap_or(json!("")),
            "dns_link_samsung": line.get("dns_link_for_samsung_lg").cloned().unwrap_or(json!("")),
            "qr_url": data.get("qr").and_then(|q| q.get("url")).cloned().unwrap_or(json!("")),
            "max_connections": line.get("max_connections").cloned().unwrap_or(json!(null)),
            "is_trial": line.get("is_trial").cloned().unwrap_or(json!(false)),
            "created_at": line.get("created_at").cloned().unwrap_or(json!("")),
            "exp_date": line.get("exp_date").cloned().unwrap_or(json!("")),
        }),
    })
}

/// Real HTTP adapter for Golden OTT panels.
/// Only reachable when USE_MOCK_PROVIDER=false (see factory.rs).
#[derive(Debug, Clone)]
pub struct GoldenApiAdapter {
    api_url: String,
    api_key: String,
    client: reqwest::Client,
}

impl GoldenApiAdapter {
    pub fn new(api_endpoint: &str, api_key: &str, client: reqwest::Client) -> Self {
        Self {
            api_url: api_endpoint.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            client,
        }
    }

    async fn get_json(&self, path: &str) -> Result<Value, ProviderError> {
        let url = format!("{}{}", self.api_url, path);
        let resp = self
            .client
            .get(&url)
            .header("X-API-Key", &self.api_key)
            .send()
            .await
            .map_err(|e| ProviderError::Request(format!("{url}: {e}")))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| ProviderError::Request(format!("bad response from {url}: {e}")))?;
        let parsed: Value = serde_json::from_str(&text).map_err(|e| {
            ProviderError::Remote(format!("golden_api: invalid JSON response: {e}"))
        })?;
        if !status.is_success() {
            let err_msg = parsed
                .get("message")
                .and_then(|m| m.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| text.chars().take(500).collect());
            return Err(ProviderError::Remote(err_msg));
        }
        Ok(parsed)
    }

    async fn post_lines(&self, body: Value) -> Result<Value, ProviderError> {
        let url = format!("{}/lines", self.api_url);
        let resp = self
            .client
            .post(&url)
            .header("X-API-Key", &self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Request(format!("{url}: {e}")))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| ProviderError::Request(format!("bad response from {url}: {e}")))?;
        let parsed: Value = serde_json::from_str(&text).map_err(|e| {
            ProviderError::Remote(format!("golden_api: invalid JSON response: {e}"))
        })?;

        if status.as_u16() == 422 {
            // Validation errors carry message + errors/details; Django retries
            // up to 3 times with a fresh username on 422.
            let msg = parsed
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown validation error")
                .to_string();
            let details = parsed
                .get("errors")
                .or_else(|| parsed.get("details"))
                .map(|d| format!(" | Details: {d}"))
                .unwrap_or_default();
            return Err(ProviderError::Remote(format!(
                "Validation Error (422): {msg}{details}"
            )));
        }
        if !status.is_success() {
            let err_msg = parsed
                .get("message")
                .and_then(|m| m.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| text.chars().take(500).collect());
            return Err(ProviderError::Remote(err_msg));
        }
        Ok(parsed)
    }
}

#[async_trait]
impl ProviderAdapter for GoldenApiAdapter {
    fn name(&self) -> &'static str {
        "golden_api"
    }

    async fn check_device(&self, _mac: &str) -> Result<DeviceCheckResult, ProviderError> {
        Err(ProviderError::Unsupported(
            "golden_api: device check not available (M3U only)".to_string(),
        ))
    }

    async fn provision(
        &self,
        ctx: &ProvisionContext,
    ) -> Result<ProvisionedCredential, ProviderError> {
        let pack_id = ctx.external_pack_id.ok_or_else(|| {
            ProviderError::Request("golden_api: product has no external_pack_id".to_string())
        })?;

        let mut username = format!("g{}", &Uuid::new_v4().simple().to_string()[..7]);
        if let Some(u) = &ctx.preferred_username {
            if !u.is_empty() {
                username = u.clone();
            }
        }
        let mut password = Uuid::new_v4().simple().to_string()[..7].to_uppercase();
        if let Some(p) = &ctx.preferred_password {
            if !p.is_empty() {
                password = p.clone();
            }
        }

        let mut body = json!({
            "package_id": pack_id,
            "max_connections": 1,
            "is_adult": false,
            "username": username,
            "password": password,
        });
        if let Some(t) = &ctx.template_id {
            if let Ok(id) = t.parse::<i64>() {
                body["template_id"] = json!(id);
            }
        }
        if let Some(d) = &ctx.dns_domain_id {
            if let Ok(id) = d.parse::<i64>() {
                body["dns_domain_id"] = json!(id);
            }
        }
        let notes = ctx.extra.get("notes").and_then(|n| n.as_str());
        if let Some(n) = notes {
            if !n.is_empty() {
                body["notes"] = json!(n);
            }
        }

        // Django retries up to 3 attempts on 422 (new random username each time).
        let mut last_err: Option<ProviderError> = None;
        for attempt in 0..3 {
            if attempt > 0 {
                body["username"] = json!(format!("g{}", &Uuid::new_v4().simple().to_string()[..7]));
            }
            match self.post_lines(body.clone()).await {
                Ok(data) => {
                    let username_sent = body["username"].as_str().unwrap_or("").to_string();
                    let password_sent = body["password"].as_str().unwrap_or("").to_string();
                    return parse_create_response(&data, &username_sent, &password_sent);
                }
                Err(e) => {
                    if !e.to_string().starts_with("Validation Error (422)") {
                        return Err(e);
                    }
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| {
            ProviderError::Remote("golden_api: line creation failed after retries".to_string())
        }))
    }

    async fn fetch_catalog(&self) -> Result<Vec<CatalogProduct>, ProviderError> {
        Err(ProviderError::Unsupported(
            "golden_api: no catalog sync (templates/domains only)".to_string(),
        ))
    }

    async fn fetch_options(&self, kind: &str) -> Result<Value, ProviderError> {
        match kind {
            "templates" => {
                let body = self.get_json("/account/templates").await?;
                parse_templates(&body)
            }
            "domains" => {
                let body = self.get_json("/account/domains").await?;
                parse_domains(&body)
            }
            other => Err(ProviderError::Unsupported(format!(
                "{other} not supported by golden_api"
            ))),
        }
    }
}

// Parse-only unit tests: no network involved.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_parsed_from_global_envelope() {
        let body: Value = serde_json::from_str(
            r#"{"data": {"global": [{"id": 7, "name": "Basic"}, {"id": 9, "name": "Pro"}]}}"#,
        )
        .unwrap();
        let t = parse_templates(&body).unwrap();
        assert_eq!(t.as_array().unwrap().len(), 2);
        assert_eq!(t[0]["id"], 7);
    }

    #[test]
    fn templates_missing_global_is_empty_list() {
        assert_eq!(
            parse_templates(&json!({"data": {}}))
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            0
        );
        assert_eq!(
            parse_templates(&json!({"data": {"global": null}}))
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }

    #[test]
    fn domains_parsed_from_envelope() {
        let body: Value =
            serde_json::from_str(r#"{"data": [{"id": 3, "domain": "ott.example.com"}]}"#).unwrap();
        let d = parse_domains(&body).unwrap();
        assert_eq!(d.as_array().unwrap().len(), 1);
        assert_eq!(d[0]["domain"], "ott.example.com");
        assert_eq!(
            parse_domains(&json!({"data": null}))
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }

    #[test]
    fn expiry_formats_parsed() {
        let full = parse_exp_date("2026-12-31 23:59:59").unwrap();
        assert_eq!(full.to_rfc3339(), "2026-12-31T23:59:59+00:00");
        let date_only = parse_exp_date("2026-12-31").unwrap();
        assert_eq!(date_only.to_rfc3339(), "2026-12-31T23:59:59+00:00");
        assert!(parse_exp_date("").is_none());
        assert!(parse_exp_date("garbage").is_none());
    }

    #[test]
    fn create_response_parsed_into_credential() {
        let body: Value = serde_json::from_str(
            r#"{
            "data": [{
                "id": 1234,
                "username": "gabc1234",
                "exp_date": "2027-01-15 10:00:00",
                "dns_link_for_samsung_lg": "http://samsung.example.com/x",
                "max_connections": 1,
                "is_trial": false,
                "created_at": "2026-08-01 00:00:00"
            }],
            "package": {"name": "Golden Pack"},
            "template": {"name": "Basic Template"},
            "qr": {"url": "http://qr.example.com/1234"}
        }"#,
        )
        .unwrap();
        let cred = parse_create_response(&body, "gfallback", "SECRET77").unwrap();
        assert_eq!(cred.username, "gabc1234");
        assert_eq!(cred.password, "SECRET77");
        assert!(cred.dns.is_none(), "golden_api lines have no dns/m3u url");
        assert!(cred.m3u_url.is_none());
        assert_eq!(
            cred.expires_at.unwrap().to_rfc3339(),
            "2027-01-15T10:00:00+00:00"
        );
        assert_eq!(cred.extra["line_id"], 1234);
        assert_eq!(cred.extra["package"], "Golden Pack");
        assert_eq!(cred.extra["template_name"], "Basic Template");
        assert_eq!(
            cred.extra["dns_link_samsung"],
            "http://samsung.example.com/x"
        );
    }

    #[test]
    fn create_response_falls_back_to_sent_username() {
        let body: Value =
            serde_json::from_str(r#"{"data": [{"id": 1, "username": "", "exp_date": null}]}"#)
                .unwrap();
        let cred = parse_create_response(&body, "gsent123", "PASS123").unwrap();
        assert_eq!(cred.username, "gsent123");
        assert!(cred.expires_at.is_none());
    }

    #[test]
    fn create_response_requires_data_array() {
        assert!(parse_create_response(&json!({"data": []}), "u", "p").is_err());
        assert!(parse_create_response(&json!({"data": {}}), "u", "p").is_err());
        assert!(parse_create_response(&json!({"error": "nope"}), "u", "p").is_err());
    }
}
