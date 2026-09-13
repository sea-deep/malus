//! Native Apple Music service implementation for Malus.
//!
//! Provides in-process Apple Music operations directly to `malusd`:
//! - Fast catalog, search, library, and page generation via `OfficialAppleMusicApi` over HTTP.
//! - Lazy playback, authentication, DRM, and queue execution via `AppleWebSession` (WPE MusicKit).

use std::{sync::Arc, time::Duration};

use malus_ipc::wire::{
    AuthStatusWire, CatalogItemWire, LibraryKindWire, LibraryPageWire, NavigationWire,
    PageContinuationWire, PageCursorWire, PageWire, PagedListWire, SearchKindWire,
    SearchResultsWire,
};
use malus_model::{MediaRef, PageRoute, PlayerStatus, Queue, Track};
use tokio::sync::{Mutex, mpsc};
use tracing::info;

use crate::{
    api::{OfficialAppleMusicApi, ProfileTokenProvider},
    auth::AuthState,
    error::AppleError,
    pages::{self, manifest::apple_navigation},
    web::{AppleWebSession, ProductionAppleWebSession},
};

/// First-class in-process Apple Music service.
pub struct AppleService {
    session: Arc<dyn AppleWebSession>,
    api: Arc<OfficialAppleMusicApi>,
    current_auth_state: Arc<Mutex<AuthState>>,
}

impl AppleService {
    /// Initialize production Apple service wrapping real browser runtime and native HTTP engine.
    pub fn production() -> Self {
        let session = Arc::new(ProductionAppleWebSession::new());
        let token_provider = Arc::new(ProfileTokenProvider::new(session.clone()));
        let api = Arc::new(OfficialAppleMusicApi::new(token_provider));
        Self::with_session_and_api(session, api)
    }

    /// Initialize with an explicit web session and native API client (test seam).
    pub fn with_session_and_api(
        session: Arc<dyn AppleWebSession>,
        api: Arc<OfficialAppleMusicApi>,
    ) -> Self {
        Self {
            session,
            api,
            current_auth_state: Arc::new(Mutex::new(AuthState::Unknown)),
        }
    }

    /// Service identifier ("apple").
    pub fn id(&self) -> &'static str {
        "apple"
    }

    /// Service display name ("Apple Music").
    pub fn name(&self) -> &'static str {
        "Apple Music"
    }

    /// Capability list.
    pub fn capabilities(&self) -> Vec<String> {
        vec![
            "auth".to_string(),
            "auth.browser".to_string(),
            "playback".to_string(),
            "playback.seek".to_string(),
            "search".to_string(),
            "catalog.track".to_string(),
            "catalog.album".to_string(),
            "catalog.artist".to_string(),
            "catalog.playlist".to_string(),
            "library.tracks".to_string(),
            "library.albums".to_string(),
            "library.playlists".to_string(),
        ]
    }

    /// Test seam constructor initializing native API with ProfileTokenProvider.
    pub fn with_session(session: Arc<dyn AppleWebSession>) -> Self {
        let token_provider = Arc::new(ProfileTokenProvider::new(session.clone()));
        let api = Arc::new(OfficialAppleMusicApi::new(token_provider));
        Self::with_session_and_api(session, api)
    }

    /// Register a sink for playback status changes from MusicKit runtime.
    pub fn set_event_sink(&self, sink: mpsc::UnboundedSender<PlayerStatus>) {
        self.session.set_event_sink(sink);
    }

    /// Check current authentication status against cached tokens and session.
    pub async fn auth_status(&self) -> Result<AuthStatusWire, AppleError> {
        info!("Checking Apple Music authentication status...");
        match self.session.probe_auth().await {
            Ok(state) => {
                *self.current_auth_state.lock().await = state;
                Ok(state.to_status(None))
            }
            Err(AppleError::ProfileBusy) => {
                Ok(AuthState::Checking.to_status(Some("Profile currently in use".to_string())))
            }
            Err(e) => Err(e),
        }
    }

    /// Begin interactive login flow in browser window (up to 3 minutes).
    pub async fn auth_begin(&self) -> Result<AuthStatusWire, AppleError> {
        info!("Beginning Apple Music interactive login...");
        *self.current_auth_state.lock().await = AuthState::Authenticating;

        match self.session.begin_auth(Duration::from_secs(180)).await {
            Ok(state) => {
                *self.current_auth_state.lock().await = state;
                Ok(state.to_status(None))
            }
            Err(AppleError::AuthCancelled) => {
                *self.current_auth_state.lock().await = AuthState::NeedsAuth;
                Err(AppleError::Internal(
                    "Authentication window closed by user".to_string(),
                ))
            }
            Err(AppleError::AuthTimeout) => {
                *self.current_auth_state.lock().await = AuthState::NeedsAuth;
                Err(AppleError::Internal("Authentication timed out".to_string()))
            }
            Err(AppleError::ProfileBusy) => Err(AppleError::Internal(
                "Apple profile is already in use by another session".to_string(),
            )),
            Err(e) => {
                *self.current_auth_state.lock().await = AuthState::Failed;
                Err(AppleError::Internal(format!("Authentication failed: {e}")))
            }
        }
    }

    /// Log out by clearing active profile tokens.
    pub async fn auth_logout(&self) -> Result<AuthStatusWire, AppleError> {
        info!("Logging out from Apple Music...");
        self.session.logout().await?;
        *self.current_auth_state.lock().await = AuthState::NeedsAuth;
        Ok(AuthState::NeedsAuth.to_status(Some("Logged out successfully".to_string())))
    }

    /// Cleanly terminate WPE browser runtime and background tasks.
    pub async fn shutdown(&self) -> Result<(), AppleError> {
        info!("Shutting down Apple Music service...");
        self.session.shutdown().await?;
        Ok(())
    }

    /// Play a media item by MediaRef (e.g. `song:617154362`, `album:1440833098`).
    pub async fn play(&self, reference: &MediaRef) -> Result<(), AppleError> {
        info!("Apple Music play: {reference}");
        self.session
            .set_queue(reference.kind(), reference.id())
            .await
    }

    /// Read the authoritative queue snapshot from MusicKit.
    pub async fn get_queue(&self) -> Result<Queue, AppleError> {
        self.session.get_queue().await
    }

    /// Resume playback.
    pub async fn resume(&self) -> Result<(), AppleError> {
        self.session.resume().await
    }

    /// Pause playback.
    pub async fn pause(&self) -> Result<(), AppleError> {
        self.session.pause().await
    }

    /// Stop playback.
    pub async fn stop(&self) -> Result<(), AppleError> {
        self.session.stop().await
    }

    /// Seek playback to specified millisecond position.
    pub async fn seek(&self, position_ms: u64) -> Result<(), AppleError> {
        self.session.seek(position_ms).await
    }

    /// Get current player status from MusicKit runtime.
    pub async fn get_status(&self) -> Result<PlayerStatus, AppleError> {
        self.session.get_status().await
    }

    /// Fast catalog search over native HTTP.
    pub async fn search(
        &self,
        query: &str,
        kinds: &[SearchKindWire],
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<SearchResultsWire, AppleError> {
        self.api
            .search(query, kinds, limit, cursor)
            .await
            .map_err(AppleError::from)
    }

    /// Single catalog item lookup over native HTTP.
    pub async fn get_catalog_item(
        &self,
        reference: &MediaRef,
    ) -> Result<CatalogItemWire, AppleError> {
        self.api
            .get_catalog_item(reference)
            .await
            .map_err(AppleError::from)
    }

    /// Collection (album/playlist) tracks lookup over native HTTP.
    pub async fn get_collection_items(
        &self,
        reference: &MediaRef,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<PagedListWire<Track>, AppleError> {
        self.api
            .get_collection_items(reference, limit, cursor)
            .await
            .map_err(AppleError::from)
    }

    /// Personal library items read over native HTTP.
    pub async fn get_library(
        &self,
        kind: LibraryKindWire,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<LibraryPageWire, AppleError> {
        self.api
            .get_library(kind, limit, cursor)
            .await
            .map_err(AppleError::from)
    }

    /// Return native navigation hierarchy for Apple Music.
    pub fn get_navigation(&self) -> NavigationWire {
        apple_navigation()
    }

    /// Return a structured Apple page (Home, New, Radio, Library, Album/Artist/Playlist detail, Replay).
    pub async fn get_page(&self, route: &PageRoute) -> Result<PageWire, AppleError> {
        pages::get_apple_page(&self.api, route).await
    }

    /// Continue pagination for an Apple page or section.
    pub async fn continue_page(
        &self,
        route: &PageRoute,
        cursor: &PageCursorWire,
    ) -> Result<PageContinuationWire, AppleError> {
        pages::continue_apple_page(&self.api, route, cursor).await
    }
}
