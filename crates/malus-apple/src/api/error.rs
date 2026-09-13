use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppleApiError {
    #[error("Authentication required: {0}")]
    AuthRequired(String),

    #[error("Permission denied: {0}")]
    Forbidden(String),

    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Rate limited (retry after {retry_after:?})")]
    RateLimited { retry_after: Option<Duration> },

    #[error("Apple server error {status}: {message}")]
    Server { status: u16, message: String },

    #[error("Network transport error: {0}")]
    Network(String),

    #[error("Malformed response: {0}")]
    Parse(String),

    #[error("API error: {0}")]
    Other(String),
}

impl From<reqwest::Error> for AppleApiError {
    fn from(err: reqwest::Error) -> Self {
        AppleApiError::Network(err.to_string())
    }
}
