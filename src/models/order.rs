use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum OrderStatus {
    Pending,
    Completed,
    Failed,
    Refunded,
}

impl OrderStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderStatus::Pending => "PENDING",
            OrderStatus::Completed => "COMPLETED",
            OrderStatus::Failed => "FAILED",
            OrderStatus::Refunded => "REFUNDED",
        }
    }
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Order {
    pub id: Uuid,
    pub uuid: Uuid,
    pub reseller_id: Uuid,
    pub product_id: Uuid,
    pub variant_id: Option<Uuid>,
    pub quantity: i32,
    pub unit_price_at_purchase: Decimal,
    pub product_name_at_purchase: String,
    pub total_credits: Decimal,
    pub status: String,
    pub failure_reason: Option<String>,
    pub idempotency_key: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub mac: Option<String>,
    pub note: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub template_id: Option<String>,
    pub dns_domain_id: Option<String>,
}
