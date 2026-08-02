use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    #[serde(skip_serializing, default)]
    pub password_hash: String,
    pub role: String,
    pub credit_balance: Decimal,
    pub is_active: bool,
    pub uuid: Uuid,
    pub date_joined: DateTime<Utc>,
}
