//! Error types for audio and metadata providers.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Operation not supported: {0}")]
    NotSupported(String),

    #[error("Playback error: {0}")]
    Playback(String),

    #[error("Provider error: {0}")]
    Other(String),
}
