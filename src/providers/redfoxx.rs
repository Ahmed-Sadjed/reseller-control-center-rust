//! RedFoxx reseller API — JSON REST with `Authorization: Bearer`.
//! The pure helpers below do NO network calls and are fully unit-tested;
//! only the `RedfoxxAdapter` at the bottom performs requests (and it is only
//! reachable when USE_MOCK_PROVIDER=false, see factory.rs).
//! Mirrors backend/api/providers/redfoxx.py:
//!   packages : GET  /packages        -> {"data": [...]} (admin sync)
//!   create   : POST /lines           -> {"data": {username, password, exp_date: <unix>}}
//! Error envelope: {"success": false, "error_code": ..., "message": ...};
//! HTTP 422 + error_code "insufficient_credits" has a dedicated message.

use async_trait::async_trait;
use rust_decimal::Decimal;
use serde_json::{json, Value};

use super::{
    error::ProviderError,
    types::{CatalogProduct, DeviceCheckResult, ProvisionContext, ProvisionedCredential},
    ProviderAdapter,
};

/// Map a package's duration_unit to months (Django DURATION_MAP equivalent).
/// Unknown units fall back to 1 month.
pub fn duration_unit_to_months(duration: i64, unit: &str) -> i32 {
    match unit {
        "day" => ((duration as f64 / 30.0).ceil().max(1.0)) as i32,
        "week" => ((duration as f64 / 4.0).ceil().max(1.0)) as i32,
        "month" => duration as i32,
        "year" => (duration * 12) as i32,
        _ => 1,
    }
}

/// Parse a `GET /packages` response into catalog products. The envelope is
/// `{"data": [...]}`; price comes from `credits`, duration from
/// `duration`+`duration_unit` (trial packages are not filtered here).
pub fn parse_catalog(body: &str) -> Result<Vec<CatalogProduct>, ProviderError> {
    let raw: Value = serde_json::from_str(body)
        .map_err(|e| ProviderError::Remote(format!("redfoxx: malformed packages ({e}): {body}")))?;
    let items: Vec<&Value> = if let Some(arr) = raw.as_array() {
        arr.iter().collect()
    } else {
        raw.get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| {
                ProviderError::Remote("redfoxx: packages response has no data array".to_string())
            })?
            .iter()
            .collect()
    };

    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let id = item.get("id").and_then(|v| v.as_i64()).map(|i| i.to_string()).unwrap_or_default();
        let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let credits = item.get("credits").and_then(|v| v.as_i64()).unwrap_or(0);
        let duration = item.get("duration").and_then(|v| v.as_i64()).unwrap_or(1);
        let unit = item.get("duration_unit").and_then(|v| v.as_str()).unwrap_or("month");
        out.push(CatalogProduct {
            external_pack_id: id,
            name,
            duration_months: duration_unit_to_months(duration, unit),
            price: Decimal::from(credits),
            category: None,
        });
    }
    Ok(out)
}

/// Parse a `POST /lines` response into a provisioned credential. The item
/// carries `username`, `password` and `exp_date` (integer Unix epoch).
pub fn parse_create_response(
    body: &str,
) -> Result<ProvisionedCredential, ProviderError> {
    let raw: Value = serde_json::from_str(body)
        .map_err(|e| ProviderError::Remote(format!("redfoxx: malformed create response ({e}): {body}")))?;
    let item = raw
        .as_object()
        .and_then(|o| o.get("data"))
        .cloned()
        .unwrap_or(raw.clone());

    let username = item.get("username").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let password = item.get("password").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if username.is_empty() || password.is_empty() {
        return Err(ProviderError::Remote(
            "redfoxx: line created without username/password".to_string(),
        ));
    }

    let expires_at = item
        .get("exp_date")
        .and_then(|v| v.as_i64())
        .and_then(|secs| {
            chrono::DateTime::from_timestamp(secs, 0)
        });

    let mut extra = json!({
        "provider": "redfoxx",
        "username": username,
        "password": password,
    });
    for key in ["max_connections", "is_trial", "notes"] {
        if let Some(v) = item.get(key) {
            if !v.is_null() {
                extra[key] = v.clone();
            }
        }
    }

    Ok(ProvisionedCredential {
        username,
        password,
        dns: None,
        m3u_url: None,
        expires_at,
        extra,
    })
}

/// Real HTTP adapter for RedFoxx panels.
/// Only reachable when USE_MOCK_PROVIDER=false (see factory.rs).
#[derive(Debug, Clone)]
pub struct RedfoxxAdapter {
    api_url: String,
    api_key: String,
    client: reqwest::Client,
}

impl RedfoxxAdapter {
    pub fn new(api_endpoint: &str, api_key: &str, client: reqwest::Client) -> Self {
        Self {
            api_url: api_endpoint.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            client,
        }
    }

    async fn request_json(&self, method: reqwest::Method, path: &str, body: Option<Value>) -> Result<Value, ProviderError> {
        let url = format!("{}{}", self.api_url, path);
        let mut req = self
            .client
            .request(method, &url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json");
        if let Some(b) = body {
            req = req.json(&b);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| ProviderError::Request(format!("{url}: {e}")))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| ProviderError::Request(format!("bad response from {url}: {e}")))?;
        let parsed: Value = serde_json::from_str(&text)
            .map_err(|e| ProviderError::Remote(format!("redfoxx: invalid JSON response: {e}")))?;

        if status.as_u16() == 422 {
            let error_code = parsed.get("error_code").and_then(|v| v.as_str()).unwrap_or("");
            if error_code == "insufficient_credits" {
                return Err(ProviderError::Remote(
                    "Insufficient credits on provider side".to_string(),
                ));
            }
            return Err(ProviderError::Remote(format!(
                "Provider error (HTTP 422): {}",
                if error_code.is_empty() {
                    text.chars().take(200).collect::<String>()
                } else {
                    error_code.to_string()
                }
            )));
        }
        if !status.is_success() {
            let error_code = parsed.get("error_code").and_then(|v| v.as_str()).unwrap_or("");
            return Err(ProviderError::Remote(format!(
                "Provider error (HTTP {}): {}",
                status.as_u16(),
                if error_code.is_empty() {
                    text.chars().take(200).collect::<String>()
                } else {
                    error_code.to_string()
                }
            )));
        }
        if parsed.as_object().and_then(|o| o.get("success")).and_then(|v| v.as_bool()) == Some(false) {
            let msg = parsed
                .get("error_code")
                .and_then(|v| v.as_str())
                .or_else(|| parsed.get("message").and_then(|v| v.as_str()))
                .unwrap_or("unknown error");
            return Err(ProviderError::Remote(format!("Provider error: {msg}")));
        }
        Ok(parsed)
    }
}

#[async_trait]
impl ProviderAdapter for RedfoxxAdapter {
    fn name(&self) -> &'static str {
        "redfoxx"
    }

    async fn check_device(&self, _mac: &str) -> Result<DeviceCheckResult, ProviderError> {
        Err(ProviderError::Unsupported(
            "redfoxx: device check not available (M3U only)".to_string(),
        ))
    }

    async fn provision(
        &self,
        ctx: &ProvisionContext,
    ) -> Result<ProvisionedCredential, ProviderError> {
        let pack_id = ctx.external_pack_id.ok_or_else(|| {
            ProviderError::Request("redfoxx: product has no external_pack_id".to_string())
        })?;
        let mut body = json!({ "package_id": pack_id });
        if let Some(u) = &ctx.preferred_username {
            if !u.is_empty() {
                body["username"] = json!(u);
            }
        }
        if let Some(p) = &ctx.preferred_password {
            if !p.is_empty() {
                body["password"] = json!(p);
            }
        }
        let data = self.request_json(reqwest::Method::POST, "/lines", Some(body)).await?;
        parse_create_response(&serde_json::to_string(&data).map_err(|e| {
            ProviderError::Request(format!("redfoxx: serialize: {e}"))
        })?)
    }

    async fn fetch_catalog(&self) -> Result<Vec<CatalogProduct>, ProviderError> {
        let data = self.request_json(reqwest::Method::GET, "/packages", None).await?;
        parse_catalog(&serde_json::to_string(&data).map_err(|e| {
            ProviderError::Request(format!("redfoxx: serialize: {e}"))
        })?)
    }
}

// Parse-only unit tests: no network involved.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_units_mapped_to_months() {
        assert_eq!(duration_unit_to_months(1, "month"), 1);
        assert_eq!(duration_unit_to_months(2, "year"), 24);
        assert_eq!(duration_unit_to_months(30, "day"), 1);
        assert_eq!(duration_unit_to_months(90, "day"), 3);
        assert_eq!(duration_unit_to_months(4, "week"), 1);
        assert_eq!(duration_unit_to_months(1, "fortnight"), 1);
    }

    #[test]
    fn packages_parsed_from_data_envelope() {
        let body = r#"{"data": [
            {"id": 11, "name": "Red 1M", "credits": 5, "duration": 1, "duration_unit": "month", "is_trial": false},
            {"id": 12, "name": "Red 1Y", "credits": 40, "duration": 1, "duration_unit": "year", "is_trial": false}
        ]}"#;
        let catalog = parse_catalog(body).unwrap();
        assert_eq!(catalog.len(), 2);
        assert_eq!(catalog[0].external_pack_id, "11");
        assert_eq!(catalog[0].name, "Red 1M");
        assert_eq!(catalog[0].duration_months, 1);
        assert_eq!(catalog[0].price, Decimal::from(5));
        assert_eq!(catalog[1].duration_months, 12);
        assert_eq!(catalog[1].price, Decimal::from(40));
    }

    #[test]
    fn packages_accepted_as_bare_list() {
        let body = r#"[{"id": 1, "name": "P", "credits": 3, "duration": 1, "duration_unit": "month"}]"#;
        assert_eq!(parse_catalog(body).unwrap().len(), 1);
        assert!(parse_catalog(r#"{"error": "x"}"#).is_err());
    }

    #[test]
    fn create_response_parsed_with_unix_expiry() {
        let body = r#"{"data": {"username": "rf_user", "password": "rf_pass", "exp_date": 1798761600, "max_connections": 2}}"#;
        let cred = parse_create_response(body).unwrap();
        assert_eq!(cred.username, "rf_user");
        assert_eq!(cred.password, "rf_pass");
        assert_eq!(cred.expires_at.unwrap().timestamp(), 1798761600);
        assert_eq!(cred.extra["max_connections"], 2);
        assert_eq!(cred.extra["provider"], "redfoxx");
    }

    #[test]
    fn create_response_without_expiry_is_lifetime() {
        let body = r#"{"data": {"username": "u", "password": "p"}}"#;
        let cred = parse_create_response(body).unwrap();
        assert!(cred.expires_at.is_none());
    }

    #[test]
    fn create_response_requires_credentials() {
        assert!(parse_create_response(r#"{"data": {"username": "u"}}"#).is_err());
        assert!(parse_create_response("not json").is_err());
    }
}
