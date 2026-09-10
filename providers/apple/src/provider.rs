//! Apple Music provider implementation for Malus.
//!
//! Exposes the provider over the standard `malus-provider-sdk::Provider` interface,
//! declaring strictly `auth` and `auth.browser` capabilities for M1b.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use malus_protocol::{PlayerStatusWire, provider::ProviderEvent, wire::AuthStatusWire};
use malus_provider_sdk::{
    Provider,
    capability::{AUTH, AUTH_BROWSER, PLAYBACK, PLAYBACK_SEEK},
    error::ProviderError,
};
use tokio::sync::{Mutex, mpsc};
use tracing::info;

use crate::{
    auth::AuthState,
    error::AppleError,
    web::{AppleWebSession, ProductionAppleWebSession},
};

pub struct AppleProvider {
    session: Arc<dyn AppleWebSession>,
    current_auth_state: Arc<Mutex<AuthState>>,
}

impl AppleProvider {
    /// Initialize production Apple provider wrapping real browser runtime.
    pub fn production() -> Self {
        Self::with_session(Arc::new(ProductionAppleWebSession::new()))
    }

    /// Initialize with an arbitrary web session (used for test seam mocking).
    pub fn with_session(session: Arc<dyn AppleWebSession>) -> Self {
        Self {
            session,
            current_auth_state: Arc::new(Mutex::new(AuthState::Unknown)),
        }
    }
}

#[async_trait]
impl Provider for AppleProvider {
    fn id(&self) -> &str {
        "apple"
    }

    fn name(&self) -> &str {
        "Apple Music"
    }

    fn capabilities(&self) -> Vec<String> {
        vec![
            AUTH.to_string(),
            AUTH_BROWSER.to_string(),
            PLAYBACK.to_string(),
            PLAYBACK_SEEK.to_string(),
        ]
    }

    fn register_event_sink(&self, sink: mpsc::UnboundedSender<ProviderEvent>) {
        self.session.set_event_sink(sink);
    }

    async fn auth_status(&self) -> Result<AuthStatusWire, ProviderError> {
        info!("Checking Apple Music authentication status...");
        match self.session.probe_auth().await {
            Ok(state) => {
                *self.current_auth_state.lock().await = state;
                Ok(state.to_status(self.id(), None))
            }
            Err(AppleError::ProfileBusy) => Ok(AuthState::Checking
                .to_status(self.id(), Some("Profile currently in use".to_string()))),
            Err(e) => Err(ProviderError::Other(format!("Failed to probe auth: {e}"))),
        }
    }

    async fn auth_begin(&self) -> Result<AuthStatusWire, ProviderError> {
        info!("Beginning Apple Music interactive login...");
        *self.current_auth_state.lock().await = AuthState::Authenticating;

        // 3-minute timeout for user login
        match self.session.begin_auth(Duration::from_secs(180)).await {
            Ok(state) => {
                *self.current_auth_state.lock().await = state;
                Ok(state.to_status(self.id(), None))
            }
            Err(AppleError::AuthCancelled) => {
                *self.current_auth_state.lock().await = AuthState::NeedsAuth;
                Err(ProviderError::Other(
                    "Authentication window closed by user".to_string(),
                ))
            }
            Err(AppleError::AuthTimeout) => {
                *self.current_auth_state.lock().await = AuthState::NeedsAuth;
                Err(ProviderError::Other("Authentication timed out".to_string()))
            }
            Err(AppleError::ProfileBusy) => Err(ProviderError::Other(
                "Apple profile is already in use by another session".to_string(),
            )),
            Err(e) => {
                *self.current_auth_state.lock().await = AuthState::Failed;
                Err(ProviderError::Other(format!("Authentication failed: {e}")))
            }
        }
    }

    async fn auth_logout(&self) -> Result<AuthStatusWire, ProviderError> {
        info!("Logging out from Apple Music...");
        self.session
            .logout()
            .await
            .map_err(|e| ProviderError::Other(format!("Logout failed: {e}")))?;

        *self.current_auth_state.lock().await = AuthState::NeedsAuth;
        Ok(AuthState::NeedsAuth.to_status(self.id(), Some("Logged out successfully".to_string())))
    }

    async fn shutdown(&self) -> Result<(), ProviderError> {
        info!("Shutting down Apple Music provider...");
        self.session
            .shutdown()
            .await
            .map_err(|e| ProviderError::Other(format!("Shutdown error: {e}")))?;
        Ok(())
    }

    async fn play(&self, media_id: &str) -> Result<(), ProviderError> {
        info!("Apple Music play: {media_id}");
        let mid = malus_protocol::MediaIdWire::parse(media_id)
            .map_err(|e| ProviderError::Other(format!("Invalid MediaId '{media_id}': {e}")))?;

        if mid.provider() != self.id() {
            return Err(ProviderError::Other(format!(
                "Expected provider 'apple', got '{}'",
                mid.provider()
            )));
        }

        if mid.kind() != "track" {
            return Err(ProviderError::Other(format!(
                "Expected item kind 'track', got '{}'",
                mid.kind()
            )));
        }

        let catalog_id = mid.id();
        self.session
            .play_track(catalog_id)
            .await
            .map_err(|e| ProviderError::Other(format!("Play track failed: {e}")))
    }

    async fn resume(&self) -> Result<(), ProviderError> {
        self.session
            .resume()
            .await
            .map_err(|e| ProviderError::Other(format!("Resume failed: {e}")))
    }

    async fn pause(&self) -> Result<(), ProviderError> {
        self.session
            .pause()
            .await
            .map_err(|e| ProviderError::Other(format!("Pause failed: {e}")))
    }

    async fn stop(&self) -> Result<(), ProviderError> {
        self.session
            .stop()
            .await
            .map_err(|e| ProviderError::Other(format!("Stop failed: {e}")))
    }

    async fn seek(&self, position_ms: u64) -> Result<(), ProviderError> {
        self.session
            .seek(position_ms)
            .await
            .map_err(|e| ProviderError::Other(format!("Seek failed: {e}")))
    }

    async fn get_status(&self) -> Result<PlayerStatusWire, ProviderError> {
        self.session
            .get_status()
            .await
            .map_err(|e| ProviderError::Other(format!("Get status failed: {e}")))
    }
}
