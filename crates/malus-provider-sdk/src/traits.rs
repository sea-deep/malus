//! Provider traits for behavioral audio playback and metadata operations.

use crate::error::ProviderError;
use async_trait::async_trait;
use malus_protocol::{MediaIdWire, PlayerStatusWire, QueueWire, TrackWire};

pub mod capability {
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

    /// Search provider catalog for tracks.
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<TrackWire>, ProviderError>;

    /// Start playback of a specific namespaced media identifier.
    async fn play(&self, media_id: &str) -> Result<(), ProviderError>;

    /// Pause current playback.
    async fn pause(&self) -> Result<(), ProviderError>;

    /// Resume paused playback.
    async fn resume(&self) -> Result<(), ProviderError>;

    /// Stop playback and reset position.
    async fn stop(&self) -> Result<(), ProviderError>;

    /// Advance to next track in queue.
    async fn next(&self) -> Result<(), ProviderError>;

    /// Return to previous track in queue.
    async fn previous(&self) -> Result<(), ProviderError>;

    /// Seek to a specific position in milliseconds.
    async fn seek(&self, position_ms: u64) -> Result<(), ProviderError>;

    /// Set playback volume (0 - 100).
    async fn set_volume(&self, volume: u8) -> Result<(), ProviderError>;

    /// Return current authoritative player status.
    async fn get_status(&self) -> Result<PlayerStatusWire, ProviderError>;

    /// Return current authoritative queue snapshot.
    async fn get_queue(&self) -> Result<QueueWire, ProviderError>;

    /// Enqueue a track into the provider's queue.
    async fn enqueue(&self, track: TrackWire) -> Result<(), ProviderError>;

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
