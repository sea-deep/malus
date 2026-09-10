//! Provider traits for behavioral audio playback and metadata operations.

use crate::error::ProviderError;
use async_trait::async_trait;
use malus_protocol::{MediaIdWire, PlayerStatusWire, QueueWire, TrackWire};

pub mod capability {
    pub const AUTH: &str = "auth";
    pub const AUTH_BROWSER: &str = "auth.browser";
    pub const SEARCH: &str = "search";
    pub const PLAYBACK: &str = "playback";
    pub const PLAYBACK_SEEK: &str = "playback.seek";
    pub const QUEUE_READ: &str = "queue.read";
    pub const QUEUE_EDIT: &str = "queue.edit";
    pub const LIBRARY_ALBUMS: &str = "library.albums";
    pub const LYRICS_SYNCED: &str = "lyrics.synced";
}

/// The primary behavioral interface for a Malus audio provider.
///
/// Providers are authoritative for playback state and their own playback queue.
/// Playback semantics are command-driven (play, pause, resume, seek, next, etc.)
/// rather than stream URL resolution.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Unique identifier for the provider (e.g. "mock", "spotify", "apple").
    fn id(&self) -> &str;

    /// Human-friendly display name.
    fn name(&self) -> &str;

    /// Extensible list of declared capability strings.
    fn capabilities(&self) -> Vec<String>;

    /// Register an event sender for asynchronous unsolicited events (e.g. `StatusChanged`).
    ///
    /// Providers that push events store this channel sender and call `send(...)`
    /// without holding provider locks across event delivery.
    fn register_event_sink(
        &self,
        sink: tokio::sync::mpsc::UnboundedSender<malus_protocol::provider::ProviderEvent>,
    ) {
        let _ = sink;
    }

    /// Return current service authentication status.
    ///
    /// Providers that require no credentials default to `AuthStateWire::Authenticated`.
    async fn auth_status(&self) -> Result<malus_protocol::wire::AuthStatusWire, ProviderError> {
        Ok(malus_protocol::wire::AuthStatusWire::new(
            self.id(),
            malus_protocol::wire::AuthStateWire::Authenticated,
        ))
    }

    /// Initiate service authentication flow.
    async fn auth_begin(&self) -> Result<malus_protocol::wire::AuthStatusWire, ProviderError> {
        Err(ProviderError::NotSupported(
            "Interactive authentication is not supported by this provider".to_string(),
        ))
    }

    /// Log out and invalidate / clear session credentials.
    async fn auth_logout(&self) -> Result<malus_protocol::wire::AuthStatusWire, ProviderError> {
        Err(ProviderError::NotSupported(
            "Logout is not supported by this provider".to_string(),
        ))
    }

    /// Search provider catalog for tracks.
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<TrackWire>, ProviderError> {
        let _ = (query, limit);
        Err(ProviderError::NotSupported(
            "Search is not supported".to_string(),
        ))
    }

    /// Start playback of a specific namespaced media identifier.
    async fn play(&self, media_id: &str) -> Result<(), ProviderError> {
        let _ = media_id;
        Err(ProviderError::NotSupported(
            "Playback is not supported".to_string(),
        ))
    }

    /// Pause current playback.
    async fn pause(&self) -> Result<(), ProviderError> {
        Err(ProviderError::NotSupported(
            "Pause is not supported".to_string(),
        ))
    }

    /// Resume paused playback.
    async fn resume(&self) -> Result<(), ProviderError> {
        Err(ProviderError::NotSupported(
            "Resume is not supported".to_string(),
        ))
    }

    /// Stop playback and reset position.
    async fn stop(&self) -> Result<(), ProviderError> {
        Err(ProviderError::NotSupported(
            "Stop is not supported".to_string(),
        ))
    }

    /// Advance to next track in queue.
    async fn next(&self) -> Result<(), ProviderError> {
        Err(ProviderError::NotSupported(
            "Next track is not supported".to_string(),
        ))
    }

    /// Return to previous track in queue.
    async fn previous(&self) -> Result<(), ProviderError> {
        Err(ProviderError::NotSupported(
            "Previous track is not supported".to_string(),
        ))
    }

    /// Seek to a specific position in milliseconds.
    async fn seek(&self, position_ms: u64) -> Result<(), ProviderError> {
        let _ = position_ms;
        Err(ProviderError::NotSupported(
            "Seek is not supported".to_string(),
        ))
    }

    /// Set playback volume (0 - 100).
    async fn set_volume(&self, volume: u8) -> Result<(), ProviderError> {
        let _ = volume;
        Err(ProviderError::NotSupported(
            "Volume adjustment is not supported".to_string(),
        ))
    }

    /// Return current authoritative player status.
    async fn get_status(&self) -> Result<PlayerStatusWire, ProviderError> {
        Err(ProviderError::NotSupported(
            "Player status is not supported".to_string(),
        ))
    }

    /// Return current authoritative queue snapshot.
    async fn get_queue(&self) -> Result<QueueWire, ProviderError> {
        Err(ProviderError::NotSupported(
            "Queue inspection is not supported".to_string(),
        ))
    }

    /// Enqueue a track into the provider's queue.
    async fn enqueue(&self, track: TrackWire) -> Result<(), ProviderError> {
        let _ = track;
        Err(ProviderError::NotSupported(
            "Queue modification is not supported".to_string(),
        ))
    }

    /// Execute a provider-specific custom action (e.g. `mock.repost`).
    async fn custom_action(
        &self,
        action: &str,
        target: Option<&MediaIdWire>,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, ProviderError> {
        let _ = (target, params);
        Err(ProviderError::NotSupported(format!(
            "Action '{action}' is not supported"
        )))
    }

    /// Cleanly shut down provider background resources and children.
    async fn shutdown(&self) -> Result<(), ProviderError> {
        Ok(())
    }
}
