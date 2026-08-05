pub mod cms_only;
pub mod error;
pub mod factory;
pub mod gold_panel;
pub mod golden_api;
pub mod hotplayer;
pub mod mock;
pub mod promax;
pub mod redfoxx;
pub mod tivipanel;
pub mod types;
pub mod whatsapp;

use serde::Deserialize;

/// Map a variant duration to the panels' `sub`/`package` value. Hour-coded
/// free-trial durations (100=6h, 101=12h, 102=24h, 103=72h) and missing
/// durations map to package 0 (free trial/lifetime); month durations pass
/// through unchanged.
pub fn duration_to_sub(duration_months: Option<i32>) -> i32 {
    match duration_months {
        Some(d) if d >= 100 => 0,
        Some(d) => d,
        None => 0,
    }
}

/// Panels send `status` either as the string `"true"` or the boolean `true`;
/// accept both.
#[derive(Debug, Deserialize, serde::Serialize)]
#[serde(untagged)]
pub enum StatusValue {
    Bool(bool),
    Str(String),
}

impl StatusValue {
    /// True for the boolean `true` or the string `"true"` (case-insensitive).
    pub fn is_true(&self) -> bool {
        match self {
            StatusValue::Bool(b) => *b,
            StatusValue::Str(s) => s.eq_ignore_ascii_case("true"),
        }
    }

    /// True for the boolean `false` or the string `"error"` (case-insensitive).
    /// Used by panels whose success status is a non-`"error"` string.
    pub fn is_error(&self) -> bool {
        match self {
            StatusValue::Bool(b) => !*b,
            StatusValue::Str(s) => s.eq_ignore_ascii_case("error"),
        }
    }
}

/// `credits`/`cost`/`user_id` etc. arrive as JSON numbers or strings; accept
/// both.
#[derive(Debug, Deserialize, serde::Serialize)]
#[serde(untagged)]
pub enum StringOrNum {
    Str(String),
    Num(f64),
}

impl StringOrNum {
    pub fn as_string(&self) -> String {
        match self {
            StringOrNum::Str(s) => s.clone(),
            StringOrNum::Num(n) => n.to_string(),
        }
    }
}

impl Default for StringOrNum {
    fn default() -> Self {
        StringOrNum::Str(String::new())
    }
}

/// Some panels return the new-line response as a bare object `{...}` and
/// others as a one-element list `[{...}]`; unwrap both shapes.
pub fn unwrap_response_object(
    raw: serde_json::Value,
    provider: &str,
) -> Result<serde_json::Value, ProviderError> {
    match raw {
        serde_json::Value::Array(arr) => arr.first().cloned().ok_or_else(|| {
            ProviderError::Remote(format!("{provider}: empty response list"))
        }),
        other => Ok(other),
    }
}

pub use cms_only::CmsOnlyAdapter;
pub use error::ProviderError;
pub use factory::get_provider;
pub use gold_panel::GoldPanelAdapter;
pub use golden_api::GoldenApiAdapter;
pub use mock::MockAdapter;
pub use promax::PromaxAdapter;
pub use redfoxx::RedfoxxAdapter;
pub use tivipanel::TiviPanelAdapter;
pub use types::*;
pub use whatsapp::WhatsAppAdapter;
