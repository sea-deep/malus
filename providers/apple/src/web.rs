//! Apple Music web session manager and MusicKit authorization adapter.
//!
//! Exposes `AppleWebSession` trait as a testability seam, decoupling provider
//! unit testing from live browser automation and network calls.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use malus_protocol::{
    PlaybackStateWire, PlayerStatusWire, RepeatModeWire, provider::ProviderEvent, wire::TrackWire,
};
use malus_web_runtime::{LaunchMode, ProfileManager, RuntimeOptions, WebPage, WebRuntime};
use serde_json::Value;
use tokio::sync::{Mutex, mpsc};
use tracing::{info, warn};

use crate::{auth::AuthState, error::AppleError};

pub const APPLE_MUSIC_URL: &str = "https://music.apple.com";
pub const APPLE_PROFILE_NAMESPACE: &str = "apple";
const MUSICKIT_AUTH_SINK: &str = "__malus_apple_auth";
const MUSICKIT_PLAYBACK_SINK: &str = "__malus_apple_playback";

/// Testability seam for Apple Music web session operations.
#[async_trait]
pub trait AppleWebSession: Send + Sync {
    /// Probe the current authorization state without prompting the user.
    async fn probe_auth(&self) -> Result<AuthState, AppleError>;

    /// Begin interactive authentication in a visible browser window and wait until authorized.
    async fn begin_auth(&self, timeout: Duration) -> Result<AuthState, AppleError>;

    /// Log out by wiping the managed Apple profile directory.
    async fn logout(&self) -> Result<(), AppleError>;

    /// Shut down any active web session.
    async fn shutdown(&self) -> Result<(), AppleError>;

    /// Play a track by its Apple Music catalog ID.
    async fn play_track(&self, catalog_id: &str) -> Result<(), AppleError>;

    /// Pause current playback.
    async fn pause(&self) -> Result<(), AppleError>;

    /// Resume current playback.
    async fn resume(&self) -> Result<(), AppleError>;

    /// Stop current playback.
    async fn stop(&self) -> Result<(), AppleError>;

    /// Seek to a specific position in milliseconds.
    async fn seek(&self, position_ms: u64) -> Result<(), AppleError>;

    /// Query current playback status directly from MusicKit.
    async fn get_status(&self) -> Result<PlayerStatusWire, AppleError>;

    /// Register a sink for streaming unsolicited player events.
    fn set_event_sink(&self, sink: mpsc::UnboundedSender<ProviderEvent>);
}

/// Production implementation backed by `malus-web-runtime`.
pub struct ProductionAppleWebSession {
    profile_lock: Arc<Mutex<()>>,
    active_runtime: Arc<Mutex<Option<WebRuntime>>>,
    event_sink: Arc<Mutex<Option<mpsc::UnboundedSender<ProviderEvent>>>>,
}

impl Default for ProductionAppleWebSession {
    fn default() -> Self {
        Self::new()
    }
}

fn maybe_migrate_legacy_profile() {
    if let Some(proj) = directories::ProjectDirs::from("com", "malus", "malus") {
        let legacy_dir = proj.data_local_dir().join("browser-profile");
        let apple_profile_dir = proj
            .data_local_dir()
            .join("profiles")
            .join(APPLE_PROFILE_NAMESPACE);

        let legacy_cookies = legacy_dir.join("Default").join("Cookies");
        let apple_cookies = apple_profile_dir.join("Default").join("Cookies");

        if legacy_cookies.exists()
            && (!apple_cookies.exists()
                || apple_cookies
                    .metadata()
                    .map(|m| m.len() == 0)
                    .unwrap_or(true))
        {
            info!(
                "Migrating existing authenticated Apple Music profile from {} to {}",
                legacy_dir.display(),
                apple_profile_dir.display()
            );
            let _ = std::fs::create_dir_all(&apple_profile_dir);
            let _ = copy_dir_recursive(&legacy_dir, &apple_profile_dir);
        }
    }
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    if !dst.exists() {
        std::fs::create_dir_all(dst)?;
    }
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dest_path = dst.join(entry.file_name());
        if ty.is_dir() {
            let _ = copy_dir_recursive(&entry.path(), &dest_path);
        } else if ty.is_file() {
            let _ = std::fs::copy(entry.path(), dest_path);
        }
    }
    Ok(())
}

impl ProductionAppleWebSession {
    pub fn new() -> Self {
        Self {
            profile_lock: Arc::new(Mutex::new(())),
            active_runtime: Arc::new(Mutex::new(None)),
            event_sink: Arc::new(Mutex::new(None)),
        }
    }

    async fn ensure_playback_session(&self) -> Result<Arc<WebPage>, AppleError> {
        let mut runtime_guard = self.active_runtime.lock().await;
        if let Some(ref runtime) = *runtime_guard
            && let Ok(health) = runtime.check_health().await
            && health.alive
        {
            return Ok(runtime.page_handle());
        }

        // Check single-owner access to profile
        let _guard = match self.profile_lock.try_lock() {
            Ok(g) => g,
            Err(_) => return Err(AppleError::ProfileBusy),
        };

        maybe_migrate_legacy_profile();

        info!("Launching headed browser for Apple Music playback session...");
        let options = RuntimeOptions {
            launch_mode: LaunchMode::Headed,
            initial_url: APPLE_MUSIC_URL.to_string(),
            profile_namespace: Some(APPLE_PROFILE_NAMESPACE.to_string()),
            ..Default::default()
        };

        let runtime = WebRuntime::launch(options).await.map_err(AppleError::Web)?;
        let page = runtime.page_handle();

        // Register playback event sink
        let _ = page.register_event_sink(MUSICKIT_PLAYBACK_SINK).await;
        let mut page_events = page.subscribe_events();

        // Wait up to 15 seconds for MusicKit to be initialized
        let musickit_ready_expr = r#"
            (() => {
                return (window.MusicKit && window.MusicKit.getInstance()) ? true : false;
            })()
        "#;

        if page
            .wait_for_expression(musickit_ready_expr, Duration::from_secs(15))
            .await
            .is_err()
        {
            let _ = runtime.shutdown().await;
            return Err(AppleError::MusicKitUnavailable);
        }

        // Check authorization: session must be authorized to play
        let probe_auth_expr = r#"
            (() => {
                try {
                    const mk = window.MusicKit.getInstance();
                    return mk ? !!mk.isAuthorized : false;
                } catch (_) {
                    return false;
                }
            })()
        "#;
        let is_auth = match page.evaluate(probe_auth_expr).await {
            Ok(Value::Bool(b)) => b,
            _ => false,
        };
        if !is_auth {
            let _ = runtime.shutdown().await;
            return Err(AppleError::NotAuthorized);
        }

        // Inject playback bridge script with deduplication and state change notifications
        let inject_script = format!(
            r#"
            (() => {{
                if (window.__malusPlaybackBridge) return;
                window.__malusPlaybackBridge = true;
                let previousState = "";

                const notify = () => {{
                    try {{
                        const mk = window.MusicKit && window.MusicKit.getInstance();
                        if (!mk) return;
                        const item = mk.nowPlayingItem;
                        const a = item ? (item.attributes || item) : null;
                        const payload = {{
                            isPlaying: !!mk.isPlaying,
                            playbackState: mk.playbackState,
                            positionMs: Math.round((mk.currentPlaybackTime || 0) * 1000),
                            durationMs: Math.round(a?.durationInMillis ? a.durationInMillis : ((item?.playbackDuration || mk.currentPlaybackDuration || 0) * 1000)),
                            volume: Math.round((mk.volume || 1) * 100),
                            muted: !!mk.isMuted,
                            shuffle: mk.shuffleMode === 1,
                            repeat: mk.repeatMode || 0,
                            track: item ? {{
                                id: String(item.id || a?.playParams?.id || ""),
                                title: String(a?.name || item.title || ""),
                                artist: String(a?.artistName || item.artistName || ""),
                                album: String(a?.albumName || item.albumName || "")
                            }} : null
                        }};
                        const json = JSON.stringify(payload);
                        if (json !== previousState) {{
                            previousState = json;
                            if (typeof window.{sink} === "function") {{
                                window.{sink}(json);
                            }}
                        }}
                    }} catch (_) {{}}
                }};

                window.__malusPlaybackNotify = notify;
                const mk = window.MusicKit && window.MusicKit.getInstance();
                if (mk) {{
                    mk.addEventListener("playbackStateDidChange", notify);
                    mk.addEventListener("nowPlayingItemDidChange", notify);
                    mk.addEventListener("playbackVolumeDidChange", notify);
                    mk.addEventListener("playbackTimeDidChange", notify);
                }}
                setInterval(notify, 500);
                notify();
            }})()
            "#,
            sink = MUSICKIT_PLAYBACK_SINK
        );
        let _ = page.evaluate(&inject_script).await;

        // Spawn event listener task
        let sink_holder = self.event_sink.clone();
        tokio::spawn(async move {
            while let Ok(event) = page_events.recv().await {
                if event.name == MUSICKIT_PLAYBACK_SINK
                    && let Some(status) = parse_player_status(&event.payload)
                {
                    let guard = sink_holder.lock().await;
                    if let Some(ref sink) = *guard {
                        let _ = sink.send(ProviderEvent::StatusChanged(status));
                    }
                }
            }
        });

        let handle = page.clone();
        *runtime_guard = Some(runtime);
        Ok(handle)
    }
}

fn parse_player_status(payload: &Value) -> Option<PlayerStatusWire> {
    let is_playing = payload["isPlaying"].as_bool().unwrap_or(false);
    let raw_state = payload["playbackState"].as_i64().unwrap_or(0);
    let position_ms = payload["positionMs"].as_u64().unwrap_or(0);
    let duration_ms = payload["durationMs"].as_u64().unwrap_or(0);
    let volume = payload["volume"].as_u64().unwrap_or(100).min(100) as u8;
    let muted = payload["muted"].as_bool().unwrap_or(false);
    let shuffle = payload["shuffle"].as_bool().unwrap_or(false);
    let repeat = match payload["repeat"].as_i64().unwrap_or(0) {
        1 => RepeatModeWire::Track,
        2 => RepeatModeWire::All,
        _ => RepeatModeWire::Off,
    };

    let state = if is_playing {
        PlaybackStateWire::Playing
    } else {
        match raw_state {
            3 => PlaybackStateWire::Paused,
            0 | 4 | 5 => PlaybackStateWire::Stopped,
            _ => {
                if position_ms > 0 {
                    PlaybackStateWire::Paused
                } else {
                    PlaybackStateWire::Stopped
                }
            }
        }
    };

    let current_track = payload.get("track").and_then(|t| {
        if t.is_null() {
            return None;
        }
        let raw_id = t["id"].as_str()?;
        if raw_id.is_empty() {
            return None;
        }
        let media_id = if raw_id.starts_with("apple:track:") {
            raw_id.to_string()
        } else {
            format!("apple:track:{raw_id}")
        };
        Some(TrackWire {
            id: media_id,
            title: t["title"].as_str().unwrap_or_default().to_string(),
            artist: t["artist"].as_str().unwrap_or_default().to_string(),
            album: t["album"].as_str().map(|s| s.to_string()),
            duration_ms: if duration_ms > 0 {
                Some(duration_ms)
            } else {
                None
            },
            uri: None,
        })
    });

    Some(PlayerStatusWire {
        state,
        current_track,
        position_ms,
        duration_ms,
        volume,
        muted,
        shuffle,
        repeat,
    })
}

#[async_trait]
impl AppleWebSession for ProductionAppleWebSession {
    /// Probe authorization state using a quick headless browser session.
    async fn probe_auth(&self) -> Result<AuthState, AppleError> {
        // Enforce single-owner access to the profile
        let _guard = match self.profile_lock.try_lock() {
            Ok(g) => g,
            Err(_) => return Err(AppleError::ProfileBusy),
        };

        maybe_migrate_legacy_profile();

        info!("Probing Apple Music authorization state...");
        let options = RuntimeOptions {
            launch_mode: LaunchMode::Headless,
            initial_url: APPLE_MUSIC_URL.to_string(),
            profile_namespace: Some(APPLE_PROFILE_NAMESPACE.to_string()),
            ..Default::default()
        };

        let runtime = match WebRuntime::launch(options).await {
            Ok(r) => r,
            Err(e) => {
                warn!("Headless probe launch failed: {e}");
                return Ok(AuthState::Unknown);
            }
        };

        let page = runtime.page_handle();
        let start = tokio::time::Instant::now();
        let timeout = Duration::from_secs(10);
        let mut mk_ready = false;

        let check_expr = r#"
            (() => {
                try {
                    const mk = window.MusicKit && window.MusicKit.getInstance();
                    if (!mk) return null;
                    return { ready: true, isAuthorized: !!mk.isAuthorized };
                } catch (_) {
                    return null;
                }
            })()
        "#;

        let mut final_state = AuthState::Unknown;

        while start.elapsed() < timeout {
            if let Ok(val) = page.evaluate(check_expr).await
                && let Some(obj) = val.as_object()
            {
                mk_ready = true;
                let is_authorized = obj
                    .get("isAuthorized")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if is_authorized {
                    info!("Apple Music probe: session is Authenticated");
                    final_state = AuthState::Authenticated;
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }

        if final_state != AuthState::Authenticated && mk_ready {
            info!("Apple Music probe: session requires authentication");
            final_state = AuthState::NeedsAuth;
        }

        let _ = runtime.shutdown().await;
        Ok(final_state)
    }

    /// Open Apple Music in a headed browser window and wait for the user to authenticate.
    async fn begin_auth(&self, timeout: Duration) -> Result<AuthState, AppleError> {
        // Enforce single-owner access to the profile
        let _guard = match self.profile_lock.try_lock() {
            Ok(g) => g,
            Err(_) => return Err(AppleError::ProfileBusy),
        };

        maybe_migrate_legacy_profile();

        info!("Launching headed browser for Apple Music authentication...");
        let options = RuntimeOptions {
            launch_mode: LaunchMode::Headed,
            initial_url: APPLE_MUSIC_URL.to_string(),
            profile_namespace: Some(APPLE_PROFILE_NAMESPACE.to_string()),
            ..Default::default()
        };

        let runtime = WebRuntime::launch(options).await.map_err(AppleError::Web)?;
        let page = runtime.page_handle();

        // Register event sink before attaching listener
        let _ = page.register_event_sink(MUSICKIT_AUTH_SINK).await;
        let mut event_rx = page.subscribe_events();

        // Wait up to 15 seconds for MusicKit to be ready on the page
        let musickit_ready_expr = r#"
            (() => {
                return (window.MusicKit && window.MusicKit.getInstance()) ? true : false;
            })()
        "#;

        if page
            .wait_for_expression(musickit_ready_expr, Duration::from_secs(15))
            .await
            .is_err()
        {
            let _ = runtime.shutdown().await;
            return Err(AppleError::MusicKitUnavailable);
        }

        // Check if already authorized
        let probe_auth_expr = r#"
            (() => {
                try {
                    const mk = window.MusicKit.getInstance();
                    return mk ? !!mk.isAuthorized : false;
                } catch (_) {
                    return false;
                }
            })()
        "#;

        if let Ok(Value::Bool(true)) = page.evaluate(probe_auth_expr).await {
            info!("User is already authorized in Apple Music.");
            let _ = runtime.shutdown().await;
            return Ok(AuthState::Authenticated);
        }

        // Inject event listener and defensive polling in page context
        let inject_listener_expr = format!(
            r#"
            (() => {{
                try {{
                    const mk = window.MusicKit && window.MusicKit.getInstance();
                    if (!mk) return;
                    const notify = () => {{
                        if (mk.isAuthorized && typeof window.{sink} === 'function') {{
                            window.{sink}(JSON.stringify({{ isAuthorized: true }}));
                        }}
                    }};
                    mk.addEventListener("authorizationStatusDidChange", notify);
                    setInterval(notify, 1000);
                }} catch (_) {{}}
            }})()
            "#,
            sink = MUSICKIT_AUTH_SINK
        );
        let _ = page.evaluate(&inject_listener_expr).await;

        info!("Waiting for user authentication in Apple Music browser window...");
        let start = tokio::time::Instant::now();

        while start.elapsed() < timeout {
            // 1. Check for push event
            if let Ok(Ok(event)) =
                tokio::time::timeout(Duration::from_millis(500), event_rx.recv()).await
                && event.name == MUSICKIT_AUTH_SINK
                && event.payload.get("isAuthorized").and_then(|v| v.as_bool()) == Some(true)
            {
                info!("Received authorization event from MusicKit.");
                // Allow cookies/tokens to flush to profile storage
                tokio::time::sleep(Duration::from_millis(1500)).await;
                let _ = runtime.shutdown().await;
                return Ok(AuthState::Authenticated);
            }

            // 2. Defensive poll on page
            if let Ok(Value::Bool(true)) = page.evaluate(probe_auth_expr).await {
                info!("Defensive poll detected MusicKit authorization.");
                tokio::time::sleep(Duration::from_millis(1500)).await;
                let _ = runtime.shutdown().await;
                return Ok(AuthState::Authenticated);
            }

            // 3. Verify browser health (detect user closing window)
            match runtime.check_health().await {
                Ok(health) if !health.alive => {
                    info!("Browser window was closed by user.");
                    let _ = runtime.shutdown().await;
                    return Err(AppleError::AuthCancelled);
                }
                Err(_) => {
                    let _ = runtime.shutdown().await;
                    return Err(AppleError::BrowserDisconnected);
                }
                _ => {}
            }
        }

        let _ = runtime.shutdown().await;
        Err(AppleError::AuthTimeout)
    }

    /// Logout by wiping the managed Apple profile directory safely.
    async fn logout(&self) -> Result<(), AppleError> {
        let _ = self.shutdown().await;
        let _guard = match self.profile_lock.try_lock() {
            Ok(g) => g,
            Err(_) => return Err(AppleError::ProfileBusy),
        };

        info!("Wiping managed Apple Music profile directory...");
        let pm = ProfileManager::for_namespace(APPLE_PROFILE_NAMESPACE).map_err(AppleError::Web)?;

        pm.wipe_managed().map_err(AppleError::Web)?;
        info!("Apple Music managed profile wiped successfully.");
        Ok(())
    }

    async fn shutdown(&self) -> Result<(), AppleError> {
        let mut runtime_guard = self.active_runtime.lock().await;
        if let Some(runtime) = runtime_guard.take() {
            info!("Shutting down active Apple Music playback runtime...");
            let _ = runtime.shutdown().await;
        }
        Ok(())
    }

    async fn play_track(&self, catalog_id: &str) -> Result<(), AppleError> {
        let page = self.ensure_playback_session().await?;
        let func = r#"
            async function(trackId) {
                const mk = window.MusicKit && window.MusicKit.getInstance();
                if (!mk) throw new Error("MusicKit instance not available");
                let lastErr = null;
                const descriptors = [
                    { songs: [trackId] },
                    { song: trackId },
                    { items: [trackId] }
                ];
                let success = false;
                for (const desc of descriptors) {
                    try {
                        await mk.setQueue(desc);
                        success = true;
                        break;
                    } catch (e) {
                        lastErr = e;
                    }
                }
                if (!success) {
                    throw lastErr || new Error("Failed to set queue for " + trackId);
                }
                await mk.play();
                if (window.__malusPlaybackNotify) {
                    window.__malusPlaybackNotify();
                }
            }
        "#;
        page.call_function(func, &[Value::String(catalog_id.to_string())])
            .await
            .map_err(|e| AppleError::PlaybackFailed(e.to_string()))?;
        Ok(())
    }

    async fn pause(&self) -> Result<(), AppleError> {
        let page = self.ensure_playback_session().await?;
        let func = r#"
            async function() {
                const mk = window.MusicKit && window.MusicKit.getInstance();
                if (!mk) throw new Error("MusicKit instance not available");
                await mk.pause();
                if (window.__malusPlaybackNotify) {
                    window.__malusPlaybackNotify();
                }
            }
        "#;
        page.call_function(func, &[])
            .await
            .map_err(|e| AppleError::PlaybackFailed(e.to_string()))?;
        Ok(())
    }

    async fn resume(&self) -> Result<(), AppleError> {
        let page = self.ensure_playback_session().await?;
        let func = r#"
            async function() {
                const mk = window.MusicKit && window.MusicKit.getInstance();
                if (!mk) throw new Error("MusicKit instance not available");
                await mk.play();
                if (window.__malusPlaybackNotify) {
                    window.__malusPlaybackNotify();
                }
            }
        "#;
        page.call_function(func, &[])
            .await
            .map_err(|e| AppleError::PlaybackFailed(e.to_string()))?;
        Ok(())
    }

    async fn stop(&self) -> Result<(), AppleError> {
        let page = self.ensure_playback_session().await?;
        let func = r#"
            async function() {
                const mk = window.MusicKit && window.MusicKit.getInstance();
                if (!mk) throw new Error("MusicKit instance not available");
                await mk.stop();
                if (window.__malusPlaybackNotify) {
                    window.__malusPlaybackNotify();
                }
            }
        "#;
        page.call_function(func, &[])
            .await
            .map_err(|e| AppleError::PlaybackFailed(e.to_string()))?;
        Ok(())
    }

    async fn seek(&self, position_ms: u64) -> Result<(), AppleError> {
        let page = self.ensure_playback_session().await?;
        let seconds = (position_ms as f64) / 1000.0;
        let func = r#"
            async function(posSeconds) {
                const mk = window.MusicKit && window.MusicKit.getInstance();
                if (!mk) throw new Error("MusicKit instance not available");
                await mk.seekToTime(posSeconds);
                if (window.__malusPlaybackNotify) {
                    window.__malusPlaybackNotify();
                }
            }
        "#;
        page.call_function(func, &[serde_json::json!(seconds)])
            .await
            .map_err(|e| AppleError::PlaybackFailed(e.to_string()))?;
        Ok(())
    }

    async fn get_status(&self) -> Result<PlayerStatusWire, AppleError> {
        let runtime_guard = self.active_runtime.lock().await;
        let page = match *runtime_guard {
            Some(ref r) => {
                if let Ok(health) = r.check_health().await
                    && health.alive
                {
                    r.page_handle()
                } else {
                    return Ok(PlayerStatusWire {
                        state: PlaybackStateWire::Stopped,
                        current_track: None,
                        position_ms: 0,
                        duration_ms: 0,
                        volume: 100,
                        muted: false,
                        shuffle: false,
                        repeat: RepeatModeWire::Off,
                    });
                }
            }
            None => {
                return Ok(PlayerStatusWire {
                    state: PlaybackStateWire::Stopped,
                    current_track: None,
                    position_ms: 0,
                    duration_ms: 0,
                    volume: 100,
                    muted: false,
                    shuffle: false,
                    repeat: RepeatModeWire::Off,
                });
            }
        };

        let expr = r#"
            (() => {
                const mk = window.MusicKit && window.MusicKit.getInstance();
                if (!mk) return null;
                const item = mk.nowPlayingItem;
                const a = item ? (item.attributes || item) : null;
                return {
                    isPlaying: !!mk.isPlaying,
                    playbackState: mk.playbackState,
                    positionMs: Math.round((mk.currentPlaybackTime || 0) * 1000),
                    durationMs: Math.round(a?.durationInMillis ? a.durationInMillis : ((item?.playbackDuration || mk.currentPlaybackDuration || 0) * 1000)),
                    volume: Math.round((mk.volume || 1) * 100),
                    muted: !!mk.isMuted,
                    shuffle: mk.shuffleMode === 1,
                    repeat: mk.repeatMode || 0,
                    track: item ? {
                        id: String(item.id || a?.playParams?.id || ""),
                        title: String(a?.name || item.title || ""),
                        artist: String(a?.artistName || item.artistName || ""),
                        album: String(a?.albumName || item.albumName || "")
                    } : null
                };
            })()
        "#;
        let val = page.evaluate(expr).await.map_err(AppleError::Web)?;
        parse_player_status(&val).ok_or_else(|| {
            AppleError::Internal("Failed to parse player status from MusicKit".to_string())
        })
    }

    fn set_event_sink(&self, sink: mpsc::UnboundedSender<ProviderEvent>) {
        let sink_lock = self.event_sink.clone();
        tokio::spawn(async move {
            *sink_lock.lock().await = Some(sink);
        });
    }
}
