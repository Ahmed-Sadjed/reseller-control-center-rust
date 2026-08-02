use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Category {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub image: Option<String>,
    pub is_active: bool,
    pub sort_order: i32,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Product {
    pub id: Uuid,
    pub name: String,
    pub category_id: Option<Uuid>,
    pub provider_id: Option<Uuid>,
    pub description: String,
    pub external_pack_id: Option<i32>,
    pub duration_months: Option<i32>,
    pub price_in_credits: Option<Decimal>,
    pub image: Option<String>,
    pub is_active: bool,
    pub is_manual: bool,
    pub credential_type: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct ProductVariant {
    pub id: Uuid,
    pub product_id: Uuid,
    pub duration_months: Option<i32>,
    pub is_lifetime: bool,
    pub external_pack_id: Option<i32>,
    pub price_in_credits: Decimal,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Frontend-visible duration label (Django `display_name`); filled in by
    /// catalog handlers, absent from raw FromRow loads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[sqlx(default)]
    pub display_name: Option<String>,
}

/// Django `duration_display`: "Lifetime" or the month/hours label.
pub fn duration_display(duration_months: Option<i32>, is_lifetime: bool) -> String {
    if is_lifetime {
        return "Lifetime".to_string();
    }
    match duration_months {
        Some(100) => "6 Hours".to_string(),
        Some(101) => "12 Hours".to_string(),
        Some(102) => "24 Hours".to_string(),
        Some(103) => "72 Hours".to_string(),
        Some(1) => "1 Month".to_string(),
        Some(n) => format!("{n} Months"),
        None => "Lifetime".to_string(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductListItem {
    pub id: Uuid,
    pub name: String,
    pub category_id: Option<Uuid>,
    pub category_name: Option<String>,
    pub category_slug: Option<String>,
    pub provider_id: Option<Uuid>,
    pub provider_name: Option<String>,
    pub provider_key: Option<String>,
    pub description: String,
    pub image: Option<String>,
    pub is_active: bool,
    pub is_manual: bool,
    pub available_credentials: Option<i64>,
    pub variants: Vec<ProductVariant>,
}
