use std::sync::Arc;

use crate::config::Settings;

use super::{
    cms_only::CmsOnlyAdapter, error::ProviderError, gold_panel::GoldPanelAdapter,
    golden_api::GoldenApiAdapter, hotplayer::HotPlayerAdapter, mock::MockAdapter,
    promax::PromaxAdapter, redfoxx::RedfoxxAdapter, tivipanel::TiviPanelAdapter,
    whatsapp::WhatsAppAdapter, ProviderAdapter,
};

/// SAFETY SWITCH: while USE_MOCK_PROVIDER is enabled (default true), the
/// factory ALWAYS returns the MockAdapter regardless of provider key.
/// Real provider traffic is only possible when USE_MOCK_PROVIDER=false.
/// Never commit code that removes this safety check.
pub fn get_provider(
    key: &str,
    endpoint: Option<&str>,
    token: Option<&str>,
    settings: &Settings,
) -> Result<Arc<dyn ProviderAdapter>, ProviderError> {
    if settings.use_mock_provider {
        tracing::info!(
            provider = key,
            "USE_MOCK_PROVIDER=true: returning MockAdapter"
        );
        let static_key: &'static str = match key {
            "tivipanel" => "tivipanel",
            "promax" => "promax",
            "hotplayer" => "hotplayer",
            "golden_api" => "golden_api",
            "neo4k" => "neo4k",
            "goldpanel" => "goldpanel",
            "redfoxx" => "redfoxx",
            "whatsapp" => "whatsapp",
            _ => "mock",
        };
        return Ok(Arc::new(MockAdapter::named(static_key)));
    }

    let endpoint = endpoint.ok_or_else(|| {
        ProviderError::Unsupported(format!("provider '{key}' has no api_endpoint configured"))
    })?;
    let token = token.unwrap_or_default();

    let client = reqwest::Client::new();

    match key {
        "hotplayer" => Ok(Arc::new(HotPlayerAdapter::new(endpoint, token, client))),
        "tivipanel" => Ok(Arc::new(TiviPanelAdapter::new(endpoint, token, client))),
        "promax" => Ok(Arc::new(PromaxAdapter::new(endpoint, token, client))),
        "golden_api" => Ok(Arc::new(GoldenApiAdapter::new(endpoint, token, client))),
        "neo4k" => Ok(Arc::new(CmsOnlyAdapter::new(endpoint, token, client))),
        "goldpanel" => Ok(Arc::new(GoldPanelAdapter::new(endpoint, token, client))),
        "redfoxx" => Ok(Arc::new(RedfoxxAdapter::new(endpoint, token, client))),
        "whatsapp" => Ok(Arc::new(WhatsAppAdapter::new())),
        other => Err(ProviderError::Unsupported(other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_provider_forced_when_flag_defaults_to_true() {
        // No env override: USE_MOCK_PROVIDER defaults to true => always mock.
        let settings = Settings::from_env();
        assert!(settings.use_mock_provider);

        let adapter = get_provider(
            "hotplayer",
            Some("https://panel.example.com"),
            Some("tok"),
            &settings,
        )
        .expect("factory must succeed");
        assert_eq!(
            adapter.name(),
            "hotplayer",
            "factory must return MockAdapter while USE_MOCK_PROVIDER is on (it reports the real key)"
        );
    }

    #[test]
    fn mock_keeps_real_provider_keys() {
        let settings = Settings::from_env();
        assert_eq!(
            get_provider(
                "tivipanel",
                Some("https://api.tivipanel.net"),
                Some("k"),
                &settings
            )
            .unwrap()
            .name(),
            "tivipanel",
            "mock adapter must report the real provider key so sync paths behave"
        );
        assert_eq!(
            get_provider(
                "promax",
                Some("https://api.promax-dash.com"),
                Some("k"),
                &settings
            )
            .unwrap()
            .name(),
            "promax"
        );
        for key in ["golden_api", "neo4k", "goldpanel", "redfoxx", "whatsapp"] {
            assert_eq!(
                get_provider(key, Some("https://panel.example.com"), Some("k"), &settings)
                    .unwrap()
                    .name(),
                key,
                "mock adapter must keep key '{key}'"
            );
        }
    }
}
