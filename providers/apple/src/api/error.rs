//! Error types for the Apple Music official HTTP API.

use malus_provider_sdk::error::ProviderError;
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

impl From<AppleApiError> for ProviderError {
    fn from(err: AppleApiError) -> Self {
        match err {
            AppleApiError::NotFound(msg) => ProviderError::NotFound(msg),
            AppleApiError::AuthRequired(msg) => {
                ProviderError::Other(format!("Authentication required: {msg}"))
            }
            AppleApiError::Forbidden(msg) => ProviderError::Other(format!("Forbidden: {msg}")),
            AppleApiError::RateLimited { retry_after } => {
                if let Some(d) = retry_after {
                    ProviderError::Other(format!(
                        "Rate limited by Apple API. Retry after {}s",
                        d.as_secs()
                    ))
                } else {
                    ProviderError::Other("Rate limited by Apple API".to_string())
                }
            }
            AppleApiError::Server { status, message } => {
                ProviderError::Other(format!("Apple API server error {status}: {message}"))
            }
            AppleApiError::Network(msg) => {
                ProviderError::Other(format!("Network transport error: {msg}"))
            }
            AppleApiError::Parse(msg) => {
                ProviderError::Other(format!("Response parse error: {msg}"))
            }
            AppleApiError::Other(msg) => ProviderError::Other(msg),
        }
    }
}
