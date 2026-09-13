//! Error types for the Apple Music provider.

use malus_web_runtime::WebError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppleError {
    #[error("Web runtime error: {0}")]
    Web(#[from] WebError),

    #[error("MusicKit is unavailable on the loaded page")]
    MusicKitUnavailable,

    #[error("Authentication timed out")]
    AuthTimeout,

    #[error("Authentication was cancelled or page closed")]
    AuthCancelled,

    #[error("Apple browser profile is currently locked or in use by another session")]
    ProfileBusy,

    #[error("Browser disconnected unexpectedly")]
    BrowserDisconnected,

    #[error("Playback failed: {0}")]
    PlaybackFailed(String),

    #[error("Apple Music session is not authorized (run 'malus provider login apple')")]
    NotAuthorized,

    #[error("Apple Music storefront unavailable: {0}")]
    StorefrontUnavailable(String),

    #[error("Apple Music resource not found: {0}")]
    NotFound(String),

    #[error("API error: {0}")]
    Api(#[from] crate::api::AppleApiError),

    #[error("Internal error: {0}")]
    Internal(String),
}
