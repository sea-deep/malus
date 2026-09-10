use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use malus_protocol::{
    PlaybackStateWire, PlayerStatusWire, RepeatModeWire,
    provider::ProviderEvent,
    wire::{AuthStateWire, TrackWire},
};
use malus_provider_apple::{AppleError, AppleProvider, AppleWebSession, AuthState};
use malus_provider_sdk::{
    Provider,
    capability::{AUTH, AUTH_BROWSER, PLAYBACK, PLAYBACK_SEEK},
};
use tokio::sync::{Mutex, mpsc};

struct MockAppleWebSession {
    probe_result: Mutex<Result<AuthState, AppleError>>,
    begin_result: Mutex<Result<AuthState, AppleError>>,
    logout_result: Mutex<Result<(), AppleError>>,
    logout_called: Mutex<bool>,
    last_played_track: Mutex<Option<String>>,
    is_paused: Mutex<bool>,
    is_stopped: Mutex<bool>,
    last_seek_ms: Mutex<Option<u64>>,
    event_sink: Mutex<Option<mpsc::UnboundedSender<ProviderEvent>>>,
}

impl MockAppleWebSession {
    fn new() -> Self {
        Self {
            probe_result: Mutex::new(Ok(AuthState::NeedsAuth)),
            begin_result: Mutex::new(Ok(AuthState::Authenticated)),
            logout_result: Mutex::new(Ok(())),
            logout_called: Mutex::new(false),
            last_played_track: Mutex::new(None),
            is_paused: Mutex::new(false),
            is_stopped: Mutex::new(true),
            last_seek_ms: Mutex::new(None),
            event_sink: Mutex::new(None),
        }
    }
}

#[async_trait]
impl AppleWebSession for MockAppleWebSession {
    async fn probe_auth(&self) -> Result<AuthState, AppleError> {
        let guard = self.probe_result.lock().await;
        match &*guard {
            Ok(s) => Ok(*s),
            Err(e) => Err(match e {
                AppleError::ProfileBusy => AppleError::ProfileBusy,
                AppleError::AuthCancelled => AppleError::AuthCancelled,
                AppleError::AuthTimeout => AppleError::AuthTimeout,
                AppleError::MusicKitUnavailable => AppleError::MusicKitUnavailable,
                AppleError::BrowserDisconnected => AppleError::BrowserDisconnected,
                _ => AppleError::AuthTimeout,
            }),
        }
    }

    async fn begin_auth(&self, _timeout: Duration) -> Result<AuthState, AppleError> {
        let guard = self.begin_result.lock().await;
        match &*guard {
            Ok(s) => Ok(*s),
            Err(e) => Err(match e {
                AppleError::ProfileBusy => AppleError::ProfileBusy,
                AppleError::AuthCancelled => AppleError::AuthCancelled,
                AppleError::AuthTimeout => AppleError::AuthTimeout,
                AppleError::MusicKitUnavailable => AppleError::MusicKitUnavailable,
                AppleError::BrowserDisconnected => AppleError::BrowserDisconnected,
                _ => AppleError::AuthTimeout,
            }),
        }
    }

    async fn logout(&self) -> Result<(), AppleError> {
        *self.logout_called.lock().await = true;
        let guard = self.logout_result.lock().await;
        match &*guard {
            Ok(()) => Ok(()),
            Err(_) => Err(AppleError::AuthCancelled),
        }
    }

    async fn shutdown(&self) -> Result<(), AppleError> {
        Ok(())
    }

    async fn play_track(&self, catalog_id: &str) -> Result<(), AppleError> {
        *self.last_played_track.lock().await = Some(catalog_id.to_string());
        *self.is_stopped.lock().await = false;
        *self.is_paused.lock().await = false;

        let guard = self.event_sink.lock().await;
        if let Some(ref sink) = *guard {
            let _ = sink.send(ProviderEvent::StatusChanged(PlayerStatusWire {
                state: PlaybackStateWire::Playing,
                current_track: Some(TrackWire::new(
                    format!("apple:track:{catalog_id}"),
                    "Mock Apple Song",
                    "Mock Artist",
                )),
                position_ms: 0,
                duration_ms: 180_000,
                volume: 100,
                muted: false,
                shuffle: false,
                repeat: RepeatModeWire::Off,
            }));
        }
        Ok(())
    }

    async fn pause(&self) -> Result<(), AppleError> {
        *self.is_paused.lock().await = true;
        Ok(())
    }

    async fn resume(&self) -> Result<(), AppleError> {
        *self.is_paused.lock().await = false;
        *self.is_stopped.lock().await = false;
        Ok(())
    }

    async fn stop(&self) -> Result<(), AppleError> {
        *self.is_stopped.lock().await = true;
        Ok(())
    }

    async fn seek(&self, position_ms: u64) -> Result<(), AppleError> {
        *self.last_seek_ms.lock().await = Some(position_ms);
        Ok(())
    }

    async fn get_status(&self) -> Result<PlayerStatusWire, AppleError> {
        let paused = *self.is_paused.lock().await;
        let stopped = *self.is_stopped.lock().await;
        let track_id = self.last_played_track.lock().await.clone();

        let state = if stopped {
            PlaybackStateWire::Stopped
        } else if paused {
            PlaybackStateWire::Paused
        } else {
            PlaybackStateWire::Playing
        };

        let current_track = track_id.map(|id| {
            TrackWire::new(
                format!("apple:track:{id}"),
                "Mock Apple Song",
                "Mock Artist",
            )
        });

        Ok(PlayerStatusWire {
            state,
            current_track,
            position_ms: self.last_seek_ms.lock().await.unwrap_or(0),
            duration_ms: 180_000,
            volume: 100,
            muted: false,
            shuffle: false,
            repeat: RepeatModeWire::Off,
        })
    }

    fn set_event_sink(&self, sink: mpsc::UnboundedSender<ProviderEvent>) {
        if let Ok(mut guard) = self.event_sink.try_lock() {
            *guard = Some(sink);
        }
    }
}

#[tokio::test]
async fn test_provider_identity_and_capabilities() {
    let mock = Arc::new(MockAppleWebSession::new());
    let provider = AppleProvider::with_session(mock);

    assert_eq!(provider.id(), "apple");
    assert_eq!(provider.name(), "Apple Music");
    assert_eq!(
        provider.capabilities(),
        vec![
            AUTH.to_string(),
            AUTH_BROWSER.to_string(),
            PLAYBACK.to_string(),
            PLAYBACK_SEEK.to_string(),
        ]
    );
}

#[tokio::test]
async fn test_auth_status_probing() {
    let mock = Arc::new(MockAppleWebSession::new());
    *mock.probe_result.lock().await = Ok(AuthState::NeedsAuth);

    let provider = AppleProvider::with_session(mock.clone());
    let status = provider.auth_status().await.expect("probe auth status");

    assert_eq!(status.provider, "apple");
    assert_eq!(status.state, AuthStateWire::NeedsAuth);

    // Simulate returning Authenticated on subsequent probe
    *mock.probe_result.lock().await = Ok(AuthState::Authenticated);
    let status = provider.auth_status().await.expect("probe auth status");
    assert_eq!(status.state, AuthStateWire::Authenticated);
}

#[tokio::test]
async fn test_auth_begin_success() {
    let mock = Arc::new(MockAppleWebSession::new());
    *mock.begin_result.lock().await = Ok(AuthState::Authenticated);

    let provider = AppleProvider::with_session(mock);
    let status = provider.auth_begin().await.expect("auth begin");

    assert_eq!(status.provider, "apple");
    assert_eq!(status.state, AuthStateWire::Authenticated);
}

#[tokio::test]
async fn test_auth_begin_cancellation() {
    let mock = Arc::new(MockAppleWebSession::new());
    *mock.begin_result.lock().await = Err(AppleError::AuthCancelled);

    let provider = AppleProvider::with_session(mock);
    let err = provider.auth_begin().await.expect_err("should cancel");

    assert!(err.to_string().contains("closed by user"));
}

#[tokio::test]
async fn test_auth_begin_timeout() {
    let mock = Arc::new(MockAppleWebSession::new());
    *mock.begin_result.lock().await = Err(AppleError::AuthTimeout);

    let provider = AppleProvider::with_session(mock);
    let err = provider.auth_begin().await.expect_err("should time out");

    assert!(err.to_string().contains("timed out"));
}

#[tokio::test]
async fn test_auth_begin_profile_busy() {
    let mock = Arc::new(MockAppleWebSession::new());
    *mock.begin_result.lock().await = Err(AppleError::ProfileBusy);

    let provider = AppleProvider::with_session(mock);
    let err = provider
        .auth_begin()
        .await
        .expect_err("should fail if busy");

    assert!(err.to_string().contains("already in use"));
}

#[tokio::test]
async fn test_auth_logout() {
    let mock = Arc::new(MockAppleWebSession::new());
    let provider = AppleProvider::with_session(mock.clone());

    let status = provider.auth_logout().await.expect("logout");
    assert_eq!(status.state, AuthStateWire::NeedsAuth);
    assert!(*mock.logout_called.lock().await);
}

#[tokio::test]
async fn test_playback_play_valid_media_id() {
    let mock = Arc::new(MockAppleWebSession::new());
    let provider = AppleProvider::with_session(mock.clone());

    provider
        .play("apple:track:1440857781")
        .await
        .expect("play track");
    assert_eq!(
        mock.last_played_track.lock().await.as_deref(),
        Some("1440857781")
    );

    let status = provider.get_status().await.expect("get status");
    assert_eq!(status.state, PlaybackStateWire::Playing);
    assert_eq!(
        status.current_track.as_ref().map(|t| t.id.as_str()),
        Some("apple:track:1440857781")
    );
}

#[tokio::test]
async fn test_playback_play_invalid_provider_or_kind() {
    let mock = Arc::new(MockAppleWebSession::new());
    let provider = AppleProvider::with_session(mock);

    // Wrong provider
    let err = provider
        .play("mock:track:1")
        .await
        .expect_err("wrong provider");
    assert!(err.to_string().contains("Expected provider 'apple'"));

    // Wrong kind
    let err = provider
        .play("apple:album:12345")
        .await
        .expect_err("wrong kind");
    assert!(err.to_string().contains("Expected item kind 'track'"));
}

#[tokio::test]
async fn test_playback_controls_forwarding() {
    let mock = Arc::new(MockAppleWebSession::new());
    let provider = AppleProvider::with_session(mock.clone());

    provider.play("apple:track:123").await.unwrap();
    assert!(!*mock.is_paused.lock().await);

    provider.pause().await.unwrap();
    assert!(*mock.is_paused.lock().await);

    provider.resume().await.unwrap();
    assert!(!*mock.is_paused.lock().await);

    provider.seek(45_000).await.unwrap();
    assert_eq!(*mock.last_seek_ms.lock().await, Some(45_000));

    provider.stop().await.unwrap();
    assert!(*mock.is_stopped.lock().await);
}
