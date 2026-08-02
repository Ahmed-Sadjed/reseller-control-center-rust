//! WhatsApp provider adapter — deliberately empty, like the Django original.
//! Mirrors backend/api/providers/whatsapp.py: `create()` is a stub that is
//! NEVER called; services intercept `adapter_key == 'whatsapp'` BEFORE any
//! adapter and fulfill the order locally (wa.me link built from the admin's
//! whatsapp_phone setting, credentials stored in data.whatsapp). No network.

use async_trait::async_trait;

use super::{
    error::ProviderError,
    types::{CatalogProduct, DeviceCheckResult, ProvisionContext, ProvisionedCredential},
    ProviderAdapter,
};

/// Real adapter for whatsapp provider rows. `provision()` is unreachable by
/// design: the fulfillment service routes whatsapp orders locally before any
/// adapter call (Django parity). Any direct call is a programming error.
#[derive(Debug, Clone, Default)]
pub struct WhatsAppAdapter;

impl WhatsAppAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ProviderAdapter for WhatsAppAdapter {
    fn name(&self) -> &'static str {
        "whatsapp"
    }

    async fn check_device(&self, _mac: &str) -> Result<DeviceCheckResult, ProviderError> {
        Err(ProviderError::Unsupported(
            "whatsapp: device check not available".to_string(),
        ))
    }

    async fn provision(
        &self,
        _ctx: &ProvisionContext,
    ) -> Result<ProvisionedCredential, ProviderError> {
        Err(ProviderError::Unsupported(
            "whatsapp: orders are fulfilled locally (no provider API)".to_string(),
        ))
    }

    async fn fetch_catalog(&self) -> Result<Vec<CatalogProduct>, ProviderError> {
        Err(ProviderError::Unsupported(
            "whatsapp: no catalog sync".to_string(),
        ))
    }
}
