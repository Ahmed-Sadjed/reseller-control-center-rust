use actix_web::{http::StatusCode, HttpResponse, ResponseError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("redis error: {0}")]
    Redis(#[from] redis::RedisError),

    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("provider error: {0}")]
    Provider(#[from] crate::providers::ProviderError),

    #[error("authentication failed")]
    Unauthorized,

    #[error("forbidden")]
    Forbidden,

    #[error("{0}")]
    BadRequest(String),

    #[error("{0}")]
    NotSupported(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl ApiError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        ApiError::BadRequest(msg.into())
    }
}

impl From<crate::services::FulfillmentError> for ApiError {
    fn from(err: crate::services::FulfillmentError) -> Self {
        match err {
            crate::services::FulfillmentError::Provider(e) => ApiError::Provider(e),
            crate::services::FulfillmentError::Database(e) => ApiError::Database(e),
            crate::services::FulfillmentError::OrderNotFound(uuid) => {
                ApiError::NotFound(format!("order {uuid} not found"))
            }
        }
    }
}

impl ResponseError for ApiError {
    fn status_code(&self) -> StatusCode {
        match self {
            ApiError::Unauthorized => StatusCode::UNAUTHORIZED,
            ApiError::Forbidden => StatusCode::FORBIDDEN,
            ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ApiError::NotSupported(_) => StatusCode::NOT_IMPLEMENTED,
            ApiError::NotFound(_) => StatusCode::NOT_FOUND,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> HttpResponse {
        let status = self.status_code();
        // Django parity: DRF auth errors use {"detail": ...} (LoginPage reads
        // .detail), storefront/admin 4xx use {"error": ...} (ProductCard and
        // the admin pages read .error).
        let body = match self {
            ApiError::Unauthorized | ApiError::Forbidden => {
                serde_json::json!({ "detail": self.to_string() })
            }
            _ => serde_json::json!({ "error": self.to_string() }),
        };
        HttpResponse::build(status).json(body)
    }
}
