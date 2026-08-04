use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::utils::crypto::decrypt;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Credential {
    pub id: Uuid,
    pub order_id: Uuid,
    pub external_username: String,
    pub streaming_username: Option<String>,
    #[serde(skip_serializing)]
    pub encrypted_password: Vec<u8>,
    pub dns_domain: String,
    pub m3u_url: String,
    pub data: serde_json::Value,
    pub expires_at: Option<DateTime<Utc>>,
    pub is_revoked: bool,
    pub created_at: DateTime<Utc>,
}

/// Full credential as exposed to the reseller on completed orders, including
/// the decrypted streaming password (mirrors Django's CredentialSerializer).
#[derive(Debug, Clone, Serialize)]
pub struct CredentialWithPassword {
    pub id: Uuid,
    pub order_id: Uuid,
    pub external_username: String,
    pub streaming_username: Option<String>,
    pub username: String,
    pub password: String,
    pub dns_domain: String,
    pub m3u_url: String,
    pub data: serde_json::Value,
    pub expires_at: Option<DateTime<Utc>>,
    pub is_revoked: bool,
    pub created_at: DateTime<Utc>,
    pub provider_config: Option<serde_json::Value>,
}

impl Credential {
    /// Decrypt and expose the streaming password for a completed order.
    pub fn with_password(
        &self,
        master_key: &[u8],
        provider_config: Option<serde_json::Value>,
    ) -> CredentialWithPassword {
        let password = decrypt(&self.encrypted_password, master_key)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .unwrap_or_default();
        CredentialWithPassword {
            id: self.id,
            order_id: self.order_id,
            external_username: self.external_username.clone(),
            streaming_username: self.streaming_username.clone(),
            username: self
                .streaming_username
                .clone()
                .unwrap_or_else(|| self.external_username.clone()),
            password,
            dns_domain: self.dns_domain.clone(),
            m3u_url: self.m3u_url.clone(),
            data: self.data.clone(),
            expires_at: self.expires_at,
            is_revoked: self.is_revoked,
            created_at: self.created_at,
            provider_config,
        }
    }
}
