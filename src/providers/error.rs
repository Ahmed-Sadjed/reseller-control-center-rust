use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("provider request failed: {0}")]
    Request(String),

    #[error("provider returned an error: {0}")]
    Remote(String),

    #[error("unsupported provider adapter: {0}")]
    Unsupported(String),
}
