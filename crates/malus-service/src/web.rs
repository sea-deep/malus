//! Apple Music web session manager and MusicKit authorization adapter.
//!
//! Exposes `AppleWebSession` trait as a testability seam, decoupling provider
//! unit testing from live browser automation and network calls.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use malus_model::{
    AlbumRef, ArtistRef, MediaRef, PlaybackState, PlayerStatus, Queue, RepeatMode, Track,
};
use malus_wpe::{LaunchMode, ProfileManager, RuntimeOptions, WebPage, WebRuntime};
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

    /// Refresh and return current Apple Music API credentials.
    async fn refresh_tokens(&self) -> Result<crate::api::AppleCredentials, AppleError>;

    /// Log out by wiping the managed Apple profile directory.
    async fn logout(&self) -> Result<(), AppleError>;

    /// Shut down any active web session.
    async fn shutdown(&self) -> Result<(), AppleError>;

    /// Set MusicKit queue to a media item and begin playback.
    async fn set_queue(&self, kind: &str, id: &str) -> Result<(), AppleError>;

    /// Set MusicKit queue to a media item and begin playback at a specific index.
    async fn set_queue_at_index(
        &self,
        kind: &str,
        id: &str,
        start_index: usize,
    ) -> Result<(), AppleError>;

    /// Set MusicKit queue to a collection with optional shuffle.
    async fn set_queue_with_shuffle(
        &self,
        kind: &str,
        id: &str,
        shuffle: bool,
    ) -> Result<(), AppleError>;

    /// Rewind already-loaded current playable item to the beginning and ensure playback.
    /// Used only when the user explicitly re-selects the currently active item (product intent).
    /// Does not reconstruct the queue, tear down the media pipeline, or reacquire DRM licenses.
    async fn restart_current_item(&self) -> Result<(), AppleError>;

    /// Pause current playback.
    async fn pause(&self) -> Result<(), AppleError>;

    /// Resume current playback.
    async fn resume(&self) -> Result<(), AppleError>;

    /// Stop current playback.
    async fn stop(&self) -> Result<(), AppleError>;

    /// Seek to a specific position in milliseconds.
    async fn seek(&self, position_ms: u64) -> Result<(), AppleError>;

    /// Set playback volume (0..=100).
    async fn set_volume(&self, volume: u8) -> Result<(), AppleError>;

    /// Set playback shuffle state.
    async fn set_shuffle(&self, shuffle: bool) -> Result<(), AppleError>;

    /// Set playback repeat mode.
    async fn set_repeat(&self, repeat: RepeatMode) -> Result<(), AppleError>;

    /// Set native autoplay mode.
    async fn set_autoplay(&self, autoplay: bool) -> Result<(), AppleError>;

    /// Skip to next track in queue.
    async fn skip_to_next(&self) -> Result<(), AppleError>;

    /// Skip to previous track in queue.
    async fn skip_to_previous(&self) -> Result<(), AppleError>;

    /// Query current playback status directly from MusicKit.
    async fn get_status(&self) -> Result<PlayerStatus, AppleError>;

    /// Read the authoritative queue snapshot from MusicKit.
    async fn get_queue(&self) -> Result<Queue, AppleError>;

    /// Insert a media item to play next in the queue.
    async fn play_next(&self, kind: &str, id: &str) -> Result<(), AppleError>;

    /// Append a media item to the end of the queue.
    async fn play_later(&self, kind: &str, id: &str) -> Result<(), AppleError>;

    /// Jump to a specific index in the queue.
    async fn queue_jump(&self, index: usize) -> Result<(), AppleError>;

    /// Remove an item at the specified index from the queue.
    async fn queue_remove(&self, index: usize) -> Result<(), AppleError>;

    /// Move a queue item from one index to another.
    async fn queue_move(&self, from: usize, to: usize) -> Result<(), AppleError>;

    /// Clear all upcoming items in the queue (preserve current).
    async fn queue_clear_upcoming(&self) -> Result<(), AppleError>;

    /// Register a sink for streaming unsolicited player events.
    fn set_event_sink(&self, sink: mpsc::UnboundedSender<PlaybackEvent>);
}

struct ActiveSession {
    runtime: WebRuntime,
}

/// Production implementation backed by `malus-web-runtime`.
pub struct ProductionAppleWebSession {
    profile_lock: Arc<Mutex<()>>,
    active_session: Arc<Mutex<Option<ActiveSession>>>,
    event_sink: Arc<Mutex<Option<mpsc::UnboundedSender<PlaybackEvent>>>>,
}

impl Default for ProductionAppleWebSession {
    fn default() -> Self {
        Self::new()
    }
}

const MINIMAL_MUSICKIT_HTML: &str = r#"<!doctype html>
<html>
  <head>
    <meta charset="utf-8">
    <title>Apple Music</title>
    <script src="https://js-cdn.music.apple.com/musickit/v3/musickit.js"></script>
  </head>
  <body></body>
</html>"#;

pub type AppleHarvestedTokens = crate::api::AppleCredentials;

fn load_cached_tokens(profile: &ProfileManager) -> Option<AppleHarvestedTokens> {
    let token_file = profile.profile_dir().join("tokens.json");
    if let Ok(content) = std::fs::read_to_string(&token_file)
        && let Ok(tokens) = serde_json::from_str::<AppleHarvestedTokens>(&content)
        && !tokens.developer_token.is_empty()
        && !tokens.music_user_token.is_empty()
    {
        return Some(tokens);
    }
    None
}

fn resolve_apple_music_url(profile: Option<&ProfileManager>) -> String {
    if let Ok(sf) = std::env::var("MALUS_APPLE_STOREFRONT") {
        let sf = sf.trim().to_lowercase();
        if !sf.is_empty() {
            return format!("https://music.apple.com/{sf}");
        }
    }
    if let Some(profile) = profile
        && let Some(tokens) = load_cached_tokens(profile)
    {
        let sf = tokens.storefront.trim().to_lowercase();
        if !sf.is_empty() {
            return format!("https://music.apple.com/{sf}");
        }
    }
    APPLE_MUSIC_URL.to_string()
}

fn save_cached_tokens(profile: &ProfileManager, tokens: &AppleHarvestedTokens) {
    let token_file = profile.profile_dir().join("tokens.json");
    if let Ok(json) = serde_json::to_string(tokens) {
        let _ = std::fs::write(&token_file, json);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            let _ = std::fs::set_permissions(&token_file, perms);
        }
    }
}

async fn harvest_tokens(page: &WebPage) -> Result<AppleHarvestedTokens, AppleError> {
    let harvest_script = r#"
        (() => {
            try {
                const mk = window.MusicKit && window.MusicKit.getInstance();
                if (!mk) return null;

                const devToken = mk.developerToken ||
                    (mk.api && mk.api.developerToken) ||
                    (mk._api && mk._api.developerToken) ||
                    (window.MusicKit && window.MusicKit._instance && window.MusicKit._instance.developerToken);

                const userToken = mk.musicUserToken ||
                    (mk.api && mk.api.userToken) ||
                    (mk._api && mk._api.userToken);

                const storefront = mk.storefrontId ||
                    (mk.api && mk.api.storefrontId) ||
                    mk.storefrontCountryCode || "";

                if (!devToken || !userToken) return null;

                return {
                    devToken: String(devToken),
                    userToken: String(userToken),
                    storefront: String(storefront),
                    isAuthorized: !!mk.isAuthorized
                };
            } catch (_) {
                return null;
            }
        })()
    "#;

    let start = tokio::time::Instant::now();
    let timeout = Duration::from_secs(25);

    while start.elapsed() < timeout {
        if let Ok(val) = page.evaluate(harvest_script).await
            && let Some(obj) = val.as_object()
        {
            let is_auth = obj
                .get("isAuthorized")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if !is_auth {
                return Err(AppleError::NotAuthorized);
            }

            let dev = obj
                .get("devToken")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let user = obj
                .get("userToken")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let sf = obj
                .get("storefront")
                .and_then(|v| v.as_str())
                .unwrap_or_default();

            if !dev.is_empty() && !user.is_empty() {
                return Ok(AppleHarvestedTokens {
                    developer_token: dev.to_string(),
                    music_user_token: user.to_string(),
                    storefront: sf.to_string(),
                });
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    Err(AppleError::MusicKitUnavailable)
}

async fn bootstrap_minimal_page(
    page: &WebPage,
    tokens: &AppleHarvestedTokens,
) -> Result<(), AppleError> {
    info!("Transitioning to minimal Apple Music document...");

    // 1. Load minimal document via unified capability API
    page.load_document(APPLE_MUSIC_URL, MINIMAL_MUSICKIT_HTML)
        .await
        .map_err(AppleError::Web)?;

    // 2. Verify origin
    let origin_val = page
        .wait_for_expression("window.location.origin", Duration::from_secs(5))
        .await
        .map_err(AppleError::Web)?;
    if origin_val.as_str() != Some("https://music.apple.com") {
        return Err(AppleError::Internal(format!(
            "Loaded page origin mismatch: {:?}",
            origin_val
        )));
    }

    // 5. Wait for MusicKit script to load
    page.wait_for_expression(
        "typeof window.MusicKit !== 'undefined'",
        Duration::from_secs(15),
    )
    .await
    .map_err(AppleError::Web)?;

    // 6. Configure MusicKit via call_function with structured runtime host arguments
    let config_func = r#"
        async function(devToken, userToken, expectedStorefront, appName, appBuild) {
            try {
                const configOpts = {
                    developerToken: devToken,
                    app: {
                        name: appName,
                        build: appBuild
                    },
                    persist: "cookie"
                };
                if (expectedStorefront && expectedStorefront.length > 0) {
                    configOpts.storefrontId = expectedStorefront;
                    configOpts.storefrontCountryCode = expectedStorefront;
                }
                const music = await window.MusicKit.configure(configOpts);
                window.music = music;
                music.musicUserToken = userToken;
                music.assertUserStorefront = () => {};
                return { ok: true, version: window.MusicKit.version || "unknown" };
            } catch (e) {
                return { ok: false, error: e.message || String(e) };
            }
        }
    "#;

    let res = page
        .call_function(
            config_func,
            &[
                serde_json::Value::String(tokens.developer_token.clone()),
                serde_json::Value::String(tokens.music_user_token.clone()),
                serde_json::Value::String(tokens.storefront.clone()),
                serde_json::Value::String("Malus".to_string()),
                serde_json::Value::String(env!("CARGO_PKG_VERSION").to_string()),
            ],
        )
        .await
        .map_err(AppleError::Web)?;

    if res.get("ok").and_then(|v| v.as_bool()) != Some(true) {
        let err = res
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown configuration error");
        return Err(AppleError::Internal(format!(
            "MusicKit.configure failed: {}",
            err
        )));
    }

    // 7. Verify authorization
    let auth_check = r#"
        (() => {
            const mk = window.MusicKit && window.MusicKit.getInstance();
            return mk ? !!mk.isAuthorized : false;
        })()
    "#;
    let is_auth = page
        .wait_for_expression(auth_check, Duration::from_secs(10))
        .await
        .map_err(AppleError::Web)?;
    if !is_auth.as_bool().unwrap_or(false) {
        return Err(AppleError::NotAuthorized);
    }

    // 8. STOREFRONT HARD GATE: Verify storefront matches harvested expected_storefront
    let sf_check = r#"
        (() => {
            const mk = window.MusicKit && window.MusicKit.getInstance();
            if (!mk) return null;
            return mk.storefrontId || mk.storefrontCountryCode || null;
        })()
    "#;

    let mut active_sf = String::new();
    let sf_start = tokio::time::Instant::now();
    let sf_timeout = Duration::from_secs(10);
    while sf_start.elapsed() < sf_timeout {
        if let Ok(Value::String(s)) = page.evaluate(sf_check).await
            && !s.is_empty()
        {
            active_sf = s.clone();
            if s.eq_ignore_ascii_case(&tokens.storefront) {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }

    if !active_sf.eq_ignore_ascii_case(&tokens.storefront) {
        return Err(AppleError::StorefrontUnavailable(format!(
            "Storefront mismatch: minimal page reported '{}', expected '{}'",
            active_sf, tokens.storefront
        )));
    }

    info!(
        "Minimal Apple Music document bootstrapped successfully (authorized: true, storefront: '{}')",
        active_sf
    );

    Ok(())
}

fn build_inject_playback_bridge_script() -> String {
    format!(
        "(() => {{\n{}\n{}\n}})()",
        include_str!("playback/snapshot.js"),
        include_str!("playback/events.js")
    )
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum PlaybackEvent {
    Status(PlayerStatus),
    Queue(Queue),
    Error { source: String, message: String },
}

impl ProductionAppleWebSession {
    pub fn new() -> Self {
        Self {
            profile_lock: Arc::new(Mutex::new(())),
            active_session: Arc::new(Mutex::new(None)),
            event_sink: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn ensure_playback_session(&self) -> Result<Arc<WebPage>, AppleError> {
        let mut session_guard = self.active_session.lock().await;
        if let Some(ref session) = *session_guard
            && let Ok(health) = session.runtime.check_health().await
            && health.alive
        {
            return Ok(session.runtime.page_handle());
        }

        // Shut down any stale session before launching
        if let Some(session) = session_guard.take() {
            let _ = session.runtime.shutdown().await;
        }

        // Check single-owner access to profile
        let _guard = match self.profile_lock.try_lock() {
            Ok(g) => g,
            Err(_) => return Err(AppleError::ProfileBusy),
        };

        // Browser presentation: default Headless for playback, Headed if MALUS_HEADED is set.
        let launch_mode = if std::env::var("MALUS_HEADED")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
        {
            LaunchMode::Headed
        } else {
            LaunchMode::Headless
        };

        // Apple bootstrap mode: default minimal intercepted document, or full_page fallback override.
        let try_minimal = !std::env::var("MALUS_APPLE_PLAYBACK_MODE")
            .map(|v| v.eq_ignore_ascii_case("full_page"))
            .unwrap_or(false);

        let profile =
            ProfileManager::for_namespace(APPLE_PROFILE_NAMESPACE).map_err(AppleError::Web)?;
        let cached_tokens = load_cached_tokens(&profile);

        let initial_url = if cached_tokens.is_some() && try_minimal {
            "about:blank".to_string()
        } else {
            resolve_apple_music_url(Some(&profile))
        };

        info!(
            "Launching browser for Apple Music session (launch_mode: {:?}, try_minimal: {}, cached_tokens: {})...",
            launch_mode,
            try_minimal,
            cached_tokens.is_some()
        );

        let options = RuntimeOptions {
            launch_mode,
            initial_url,
            profile_namespace: Some(APPLE_PROFILE_NAMESPACE.to_string()),
            disable_background_throttling: true,
            ..Default::default()
        };

        let runtime = WebRuntime::launch(options).await.map_err(AppleError::Web)?;
        let page = runtime.page_handle();

        // Wait expression for full-page MusicKit
        let musickit_ready_expr = r#"
            (() => {
                return (window.MusicKit && window.MusicKit.getInstance()) ? true : false;
            })()
        "#;

        let mut minimal_active = false;

        if let Some(tokens) = cached_tokens
            && try_minimal
        {
            info!("Bootstrapping minimal Apple Music document with cached tokens...");
            match bootstrap_minimal_page(&page, &tokens).await {
                Ok(()) => {
                    minimal_active = true;
                }
                Err(e) => {
                    warn!(
                        "Minimal bootstrap with cached tokens failed: {e}; falling back to full music.apple.com..."
                    );
                    if let Err(nav_err) = page
                        .navigate(&resolve_apple_music_url(Some(&profile)))
                        .await
                    {
                        let _ = runtime.shutdown().await;
                        return Err(AppleError::Web(nav_err));
                    }
                }
            }
        }

        if !minimal_active {
            if page
                .wait_for_expression(musickit_ready_expr, Duration::from_secs(25))
                .await
                .is_err()
            {
                let _ = runtime.shutdown().await;
                return Err(AppleError::MusicKitUnavailable);
            }

            if try_minimal {
                let minimal_res: Result<(), AppleError> = async {
                    let tokens = harvest_tokens(&page).await?;
                    save_cached_tokens(&profile, &tokens);
                    bootstrap_minimal_page(&page, &tokens).await?;
                    Ok(())
                }
                .await;

                match minimal_res {
                    Ok(()) => {
                        minimal_active = true;
                    }
                    Err(AppleError::NotAuthorized) => {
                        let _ = runtime.shutdown().await;
                        return Err(AppleError::NotAuthorized);
                    }
                    Err(e) => {
                        warn!(
                            "Minimal MusicKit document bootstrap failed ({e}), falling back to full music.apple.com web application..."
                        );
                        if let Err(nav_err) = page
                            .navigate(&resolve_apple_music_url(Some(&profile)))
                            .await
                        {
                            let _ = runtime.shutdown().await;
                            return Err(AppleError::Web(nav_err));
                        }
                        if page
                            .wait_for_expression(musickit_ready_expr, Duration::from_secs(25))
                            .await
                            .is_err()
                        {
                            let _ = runtime.shutdown().await;
                            return Err(AppleError::MusicKitUnavailable);
                        }
                    }
                }
            }
        }

        // Check authorization on active document (whether minimal or full-page fallback)
        let probe_auth_expr = r#"
            (() => {
                try {
                    const mk = window.MusicKit && window.MusicKit.getInstance();
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

        // Register and inject exactly ONE active playback bridge
        page.register_event_sink(MUSICKIT_PLAYBACK_SINK)
            .await
            .map_err(AppleError::Web)?;
        let mut page_events = page.subscribe_events();
        page.evaluate(&build_inject_playback_bridge_script())
            .await
            .map_err(AppleError::Web)?;

        // Apply visual suppression only if full-page fallback was used in headless mode
        if !minimal_active && launch_mode == LaunchMode::Headless {
            let opt_script = r#"
                (() => {
                    try {
                        const style = document.createElement('style');
                        style.id = '__malus_suppress_styles';
                        style.textContent = `
                            *, *::before, *::after {
                                animation: none !important;
                                transition: none !important;
                            }
                            body {
                                display: none !important;
                            }
                        `;
                        document.head.appendChild(style);
                    } catch (_) {}
                })()
            "#;
            let _ = page.evaluate(opt_script).await;
        }

        // Spawn event listener task
        let sink_holder = self.event_sink.clone();
        tokio::spawn(async move {
            loop {
                let event = match page_events.recv().await {
                    Ok(event) => event,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                };
                if event.name != MUSICKIT_PLAYBACK_SINK {
                    continue;
                }
                let payload = match event.payload.as_str() {
                    Some(json) => serde_json::from_str::<Value>(json).unwrap_or(Value::Null),
                    None => event.payload,
                };
                let event = match payload["kind"].as_str() {
                    Some("status") => {
                        parse_player_status(&payload["data"]).map(PlaybackEvent::Status)
                    }
                    Some("queue") => parse_queue(&payload["data"]).ok().map(PlaybackEvent::Queue),
                    Some("error") => Some(PlaybackEvent::Error {
                        source: payload["source"].as_str().unwrap_or("MusicKit").to_string(),
                        message: payload["message"]
                            .as_str()
                            .unwrap_or("Playback error")
                            .to_string(),
                    }),
                    _ => None,
                };
                if let Some(event) = event {
                    let guard = sink_holder.lock().await;
                    if let Some(sink) = &*guard {
                        let _ = sink.send(event);
                    }
                }
            }
        });

        let handle = page.clone();
        *session_guard = Some(ActiveSession { runtime });
        Ok(handle)
    }
}

fn track_from_snapshot(t: &Value) -> Option<Track> {
    let raw_id = t["id"].as_str()?;
    if raw_id.is_empty() {
        return None;
    }
    let mref = if let Ok(parsed) = MediaRef::parse(raw_id) {
        parsed
    } else if raw_id.starts_with("ra.") {
        MediaRef::Station(raw_id.to_string())
    } else {
        let id_clean = raw_id.strip_prefix("song:").unwrap_or(raw_id);
        MediaRef::Song(id_clean.to_string())
    };

    let title = t["title"].as_str().unwrap_or_default();
    let mut artists = Vec::new();
    if let Some(arr) = t["artists"].as_array() {
        for art in arr {
            let name = art["name"].as_str().unwrap_or_default();
            let id = art["id"].as_str().unwrap_or_default();
            if !name.is_empty() {
                let mref_art = if !id.is_empty() {
                    Some(MediaRef::Artist(id.to_string()))
                } else {
                    None
                };
                artists.push(ArtistRef::new(mref_art, name));
            }
        }
    }
    if artists.is_empty() {
        let name = t["artist"].as_str().unwrap_or_default();
        let id = t["artistId"].as_str().unwrap_or_default();
        let mref_art = if !id.is_empty() {
            Some(MediaRef::Artist(id.to_string()))
        } else {
            None
        };
        artists.push(ArtistRef::new(mref_art, name));
    }

    let mut track = Track::with_artists(mref, title, artists);
    if let Some(alb) = t["album"].as_str()
        && !alb.is_empty()
    {
        let alb_id = t["albumId"]
            .as_str()
            .filter(|id| !id.is_empty())
            .map(|id| MediaRef::Album(id.to_string()));
        track = track.with_album_ref(AlbumRef::new(alb_id, alb));
    }
    if let Some(art_val) = t.get("artwork")
        && let Some(artwork) = parse_apple_artwork(art_val)
    {
        track = track.with_artwork(artwork);
    }
    if let Some(dur) = t["durationMs"].as_u64()
        && dur > 0
    {
        track = track.with_duration_ms(dur);
    }
    if let Some(tn) = t["trackNumber"].as_u64()
        && tn > 0
    {
        track = track.with_track_number(tn as u32);
    }
    if let Some(dn) = t["discNumber"].as_u64()
        && dn > 0
    {
        track = track.with_disc_number(dn as u32);
    }
    Some(track)
}

fn parse_player_status(payload: &Value) -> Option<PlayerStatus> {
    let payload_parsed: Value;
    let payload = if let Some(s) = payload.as_str() {
        if let Ok(p) = serde_json::from_str::<Value>(s) {
            payload_parsed = p;
            &payload_parsed
        } else {
            payload
        }
    } else {
        payload
    };
    let raw_state = payload["playbackState"].as_i64()?;
    let position_ms = payload["positionMs"].as_u64().unwrap_or(0);
    let duration_ms = payload["durationMs"].as_u64().unwrap_or(0);
    let volume = payload["volume"].as_u64().unwrap_or(100).min(100) as u8;
    let muted = payload["muted"].as_bool().unwrap_or(false);
    let shuffle = payload["shuffle"].as_bool().unwrap_or(false);
    let repeat = match payload["repeat"].as_i64().unwrap_or(0) {
        1 => RepeatMode::Track,
        2 => RepeatMode::All,
        _ => RepeatMode::Off,
    };

    let state = match raw_state {
        2 => PlaybackState::Playing,
        1 | 3 | 6 | 7 | 8 | 9 => PlaybackState::Paused,
        _ => PlaybackState::Stopped,
    };

    let current_track = payload.get("track").and_then(|t| {
        if t.is_null() {
            return None;
        }
        let mut track = track_from_snapshot(t)?;
        if duration_ms > 0 && track.duration_ms.is_none() {
            track = track.with_duration_ms(duration_ms);
        }
        Some(track)
    });

    let timeline_id = payload["timelineId"].as_u64().unwrap_or(0);
    let sequence = payload["sequence"].as_u64().unwrap_or(0);
    let autoplay = payload["autoplay"].as_bool().unwrap_or(false);

    Some(PlayerStatus {
        state,
        current_track,
        position_ms,
        duration_ms,
        volume,
        muted,
        shuffle,
        repeat,
        autoplay,
        timeline_id,
        sequence,
    })
}

fn parse_queue(val: &Value) -> Result<Queue, AppleError> {
    if !val["items"].is_array() {
        return Err(AppleError::Internal(
            "Invalid MusicKit queue snapshot".into(),
        ));
    }
    let items_val = &val["items"];
    let current_index = val["currentIndex"]
        .as_i64()
        .and_then(|i| if i >= 0 { Some(i as usize) } else { None });
    let autoplay_start_index = val["autoplayStartIndex"].as_u64().map(|i| i as usize);

    let mut tracks = Vec::new();
    if let Some(arr) = items_val.as_array() {
        for item in arr {
            if let Some(track) = track_from_snapshot(item) {
                tracks.push(track);
            } else {
                return Err(AppleError::Internal("Queue item has no identity".into()));
            }
        }
    }

    if current_index.is_some_and(|index| index >= tracks.len()) {
        return Err(AppleError::Internal(
            "Queue position is outside its items".into(),
        ));
    }
    Ok(Queue::with_autoplay(
        tracks,
        current_index,
        autoplay_start_index,
    ))
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

        let profile =
            ProfileManager::for_namespace(APPLE_PROFILE_NAMESPACE).map_err(AppleError::Web)?;
        if let Some(tokens) = load_cached_tokens(&profile)
            && !tokens.music_user_token.is_empty()
        {
            info!("Apple Music probe: session is Authenticated (cached tokens present)");
            return Ok(AuthState::Authenticated);
        }

        info!("Probing Apple Music authorization state...");
        let options = RuntimeOptions {
            launch_mode: LaunchMode::Headless,
            initial_url: resolve_apple_music_url(Some(&profile)),
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
                    if let Ok(tokens) = harvest_tokens(&page).await {
                        save_cached_tokens(&profile, &tokens);
                    }
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

        let profile =
            ProfileManager::for_namespace(APPLE_PROFILE_NAMESPACE).map_err(AppleError::Web)?;
        let initial_url = resolve_apple_music_url(Some(&profile));
        info!("Launching headed browser for Apple Music authentication at {initial_url}...");
        let options = RuntimeOptions {
            launch_mode: LaunchMode::Headed,
            initial_url,
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

        // Directly open the Apple Music sign-in modal
        info!("Directly opening Apple Music sign-in modal...");
        let trigger_signin_modal_expr = r#"
            (() => {
                const clickSignIn = () => {
                    const btn = document.querySelector('button[data-testid="sign-in-button"]')
                        || document.querySelector('.commerce-button.signin')
                        || document.querySelector('.signin')
                        || document.querySelector('[data-testid="auth-content"] button');
                    if (btn) {
                        btn.click();
                        return true;
                    }
                    return false;
                };

                if (clickSignIn()) {
                    return "clicked";
                }

                const observer = new MutationObserver((_, obs) => {
                    if (clickSignIn()) {
                        obs.disconnect();
                    }
                });
                observer.observe(document.body || document.documentElement, {
                    childList: true,
                    subtree: true
                });

                setTimeout(() => {
                    observer.disconnect();
                    if (!clickSignIn()) {
                        try {
                            const mk = window.MusicKit && window.MusicKit.getInstance();
                            if (mk && typeof mk.authorize === 'function') {
                                mk.authorize().catch(() => {});
                            }
                        } catch (_) {}
                    }
                }, 3000);

                return "observer_attached";
            })()
        "#;
        let _ = page.evaluate(trigger_signin_modal_expr).await;

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
                if let Ok(tokens) = harvest_tokens(&page).await
                    && let Ok(profile) = ProfileManager::for_namespace(APPLE_PROFILE_NAMESPACE)
                {
                    save_cached_tokens(&profile, &tokens);
                }
                // Allow cookies/tokens to flush to profile storage
                tokio::time::sleep(Duration::from_millis(1500)).await;
                let _ = runtime.shutdown().await;
                return Ok(AuthState::Authenticated);
            }

            // 2. Defensive poll on page
            if let Ok(Value::Bool(true)) = page.evaluate(probe_auth_expr).await {
                info!("Defensive poll detected MusicKit authorization.");
                if let Ok(tokens) = harvest_tokens(&page).await
                    && let Ok(profile) = ProfileManager::for_namespace(APPLE_PROFILE_NAMESPACE)
                {
                    save_cached_tokens(&profile, &tokens);
                }
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

    /// Refresh and return current Apple Music API credentials.
    async fn refresh_tokens(&self) -> Result<crate::api::AppleCredentials, AppleError> {
        // 1. If an active session is running, harvest directly from its page handle
        {
            let guard = self.active_session.lock().await;
            if let Some(ref session) = *guard
                && let Ok(health) = session.runtime.check_health().await
                && health.alive
            {
                let page = session.runtime.page_handle();
                if let Ok(tokens) = harvest_tokens(&page).await {
                    if let Ok(profile) = ProfileManager::for_namespace(APPLE_PROFILE_NAMESPACE) {
                        save_cached_tokens(&profile, &tokens);
                    }
                    return Ok(tokens);
                }
            }
        }

        // 2. Otherwise launch a headless browser session to harvest fresh tokens
        let _guard = match self.profile_lock.try_lock() {
            Ok(g) => g,
            Err(_) => return Err(AppleError::ProfileBusy),
        };

        let profile =
            ProfileManager::for_namespace(APPLE_PROFILE_NAMESPACE).map_err(AppleError::Web)?;

        info!("Launching headless session to harvest fresh Apple Music tokens...");
        let options = RuntimeOptions {
            launch_mode: LaunchMode::Headless,
            initial_url: resolve_apple_music_url(Some(&profile)),
            profile_namespace: Some(APPLE_PROFILE_NAMESPACE.to_string()),
            ..Default::default()
        };

        let runtime = WebRuntime::launch(options).await.map_err(AppleError::Web)?;
        let page = runtime.page_handle();
        let start = tokio::time::Instant::now();
        let timeout = Duration::from_secs(12);

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

        let mut harvested = None;
        while start.elapsed() < timeout {
            if let Ok(val) = page.evaluate(check_expr).await
                && let Some(obj) = val.as_object()
            {
                let is_auth = obj
                    .get("isAuthorized")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if is_auth && let Ok(tokens) = harvest_tokens(&page).await {
                    save_cached_tokens(&profile, &tokens);
                    harvested = Some(tokens);
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }

        let _ = runtime.shutdown().await;

        harvested.ok_or(AppleError::NotAuthorized)
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

        let _ = std::fs::remove_file(pm.profile_dir().join("tokens.json"));
        pm.wipe_managed().map_err(AppleError::Web)?;
        info!("Apple Music managed profile wiped successfully.");
        Ok(())
    }

    async fn shutdown(&self) -> Result<(), AppleError> {
        let mut session_guard = self.active_session.lock().await;
        if let Some(session) = session_guard.take() {
            info!("Shutting down active Apple Music runtime...");
            let _ = session.runtime.shutdown().await;
        }
        Ok(())
    }

    async fn set_queue(&self, kind: &str, id: &str) -> Result<(), AppleError> {
        self.set_queue_at_index(kind, id, 0).await
    }

    async fn set_queue_at_index(
        &self,
        kind: &str,
        id: &str,
        start_index: usize,
    ) -> Result<(), AppleError> {
        let page = self.ensure_playback_session().await?;
        {
            let guard = self.active_session.lock().await;
            if let Some(session) = guard.as_ref() {
                session
                    .runtime
                    .ensure_drm_supported()
                    .map_err(AppleError::Web)?;
            }
        }
        let func = r#"
            async function(kind, itemId, startIndex) {
                const mk = window.MusicKit && window.MusicKit.getInstance();
                if (!mk) throw new Error("MusicKit instance not available");

                // For library song IDs (i.xxx / l.xxx), resolve catalog ID first
                let resolvedId = itemId;
                if (kind === "song" && (itemId.startsWith("i.") || itemId.startsWith("l."))) {
                    try {
                        const res = await mk.api.music(`/v1/me/library/songs/${itemId}`);
                        const item = res?.data?.data?.[0];
                        const catId = item?.attributes?.playParams?.catalogId;
                        if (catId) resolvedId = catId;
                    } catch (_) {}
                }

                // Cleanly pause if currently playing to prevent MusicKit "The play() method was called without a previous stop() or pause() call" error
                if (mk.isPlaying || mk.playbackState === 2) {
                    try {
                        await mk.pause();
                    } catch (_) {}
                }

                // Build MusicKit setQueue descriptor based on kind
                let descriptor;
                switch (kind) {
                    case "song":
                        descriptor = { song: resolvedId, startPlaying: true };
                        break;
                    case "album":
                        descriptor = { album: resolvedId, startPlaying: true };
                        break;
                    case "playlist":
                        descriptor = { playlist: resolvedId, startPlaying: true };
                        break;
                    case "station":
                        descriptor = { station: resolvedId, startPlaying: true };
                        break;
                    default:
                        throw new Error("Unsupported media kind: " + kind);
                }
                if (typeof startIndex === "number" && startIndex > 0) {
                    descriptor.startPosition = startIndex;
                }

                await mk.setQueue(descriptor);

                if (!mk.isPlaying) {
                    try {
                        await mk.play();
                    } catch (e) {
                        console.warn("mk.play error after setQueue:", e);
                    }
                }

                if (mk.autoplayEnabled) {
                    try {
                        const pc = mk.getPlaybackController && mk.getPlaybackController();
                        if (pc && !pc.autoplayStation && !pc.loadingAutoplayStation) {
                            await pc.startAutoplay();
                        }
                    } catch (_) {}
                }

                if (window.__malusPlaybackNotify) {
                    window.__malusPlaybackNotify();
                }
            }
        "#;
        page.call_function(
            func,
            &[
                Value::String(kind.to_string()),
                Value::String(id.to_string()),
                serde_json::json!(start_index),
            ],
        )
        .await
        .map_err(|e| AppleError::PlaybackFailed(e.to_string()))?;
        Ok(())
    }

    async fn set_queue_with_shuffle(
        &self,
        kind: &str,
        id: &str,
        shuffle: bool,
    ) -> Result<(), AppleError> {
        let page = self.ensure_playback_session().await?;
        let func = r#"
            async function(shuf) {
                const mk = window.MusicKit && window.MusicKit.getInstance();
                if (!mk) throw new Error("MusicKit instance not available");
                mk.shuffleMode = shuf ? 1 : 0;
            }
        "#;
        page.call_function(func, &[serde_json::json!(shuffle)])
            .await
            .map_err(|e| AppleError::PlaybackFailed(e.to_string()))?;
        self.set_queue_at_index(kind, id, 0).await
    }

    async fn restart_current_item(&self) -> Result<(), AppleError> {
        let page = self.ensure_playback_session().await?;
        let func = r#"
            async function() {
                const mk = window.MusicKit && window.MusicKit.getInstance();
                if (!mk) throw new Error("MusicKit instance not available");
                const items = Array.from(mk.queue?.items || []);
                const position = mk.queue?.position;
                const currentIndex = Number.isInteger(position) && position >= 0 && position < items.length ? position : -1;
                const item = mk.nowPlayingItem || items[currentIndex] || null;
                if (!item) {
                    throw new Error("No item currently loaded in player to restart");
                }
                if (window.__malusPlaybackDiscontinuity) {
                    window.__malusPlaybackDiscontinuity('restart', 0);
                }
                const isPlaying = !!mk.isPlaying;
                await mk.seekToTime(0);
                if (!isPlaying) {
                    await mk.play();
                }
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

    async fn set_volume(&self, volume: u8) -> Result<(), AppleError> {
        let page = self.ensure_playback_session().await?;
        let func = r#"
            async function(vol) {
                const mk = window.MusicKit && window.MusicKit.getInstance();
                if (!mk) throw new Error("MusicKit instance not available");
                mk.volume = vol / 100.0;
                if (window.__malusPlaybackNotify) {
                    window.__malusPlaybackNotify();
                }
            }
        "#;
        page.call_function(func, &[serde_json::json!(volume.min(100))])
            .await
            .map_err(|e| AppleError::PlaybackFailed(e.to_string()))?;
        Ok(())
    }

    async fn set_shuffle(&self, shuffle: bool) -> Result<(), AppleError> {
        let page = self.ensure_playback_session().await?;
        let func = r#"
            async function(shuf) {
                const mk = window.MusicKit && window.MusicKit.getInstance();
                if (!mk) throw new Error("MusicKit instance not available");
                mk.shuffleMode = shuf ? 1 : 0;
                if (window.__malusPlaybackNotify) {
                    window.__malusPlaybackNotify();
                }
            }
        "#;
        page.call_function(func, &[serde_json::json!(shuffle)])
            .await
            .map_err(|e| AppleError::PlaybackFailed(e.to_string()))?;
        Ok(())
    }

    async fn set_repeat(&self, repeat: RepeatMode) -> Result<(), AppleError> {
        let page = self.ensure_playback_session().await?;
        let rep_code: i64 = match repeat {
            RepeatMode::Track => 1,
            RepeatMode::All => 2,
            RepeatMode::Off => 0,
        };
        let func = r#"
            async function(rep) {
                const mk = window.MusicKit && window.MusicKit.getInstance();
                if (!mk) throw new Error("MusicKit instance not available");
                mk.repeatMode = rep;
                if (window.__malusPlaybackNotify) {
                    window.__malusPlaybackNotify();
                }
            }
        "#;
        page.call_function(func, &[serde_json::json!(rep_code)])
            .await
            .map_err(|e| AppleError::PlaybackFailed(e.to_string()))?;
        Ok(())
    }

    async fn set_autoplay(&self, autoplay: bool) -> Result<(), AppleError> {
        let page = self.ensure_playback_session().await?;
        let func = r#"
            async function(auto) {
                const mk = window.MusicKit && window.MusicKit.getInstance();
                if (!mk) throw new Error("MusicKit instance not available");
                mk.autoplayEnabled = !!auto;
                try {
                    const pc = mk.getPlaybackController && mk.getPlaybackController();
                    if (pc) {
                        pc.autoplayEnabled = !!auto;
                        if (auto) {
                            if (!pc.autoplayStation && !pc.loadingAutoplayStation) {
                                await pc.startAutoplay();
                            } else if (pc.autoplayStation) {
                                await pc.queueAutoplayTracks();
                            }
                        } else {
                            await pc.stopAutoplay();
                        }
                    }
                } catch (_) {}
                if (window.__malusPlaybackNotify) {
                    window.__malusPlaybackNotify();
                }
            }
        "#;
        page.call_function(func, &[serde_json::json!(autoplay)])
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
        {
            let guard = self.active_session.lock().await;
            if let Some(session) = guard.as_ref() {
                session
                    .runtime
                    .ensure_drm_supported()
                    .map_err(AppleError::Web)?;
            }
        }
        let func = r#"
            async function() {
                const mk = window.MusicKit && window.MusicKit.getInstance();
                if (!mk) throw new Error("MusicKit instance not available");
                const items = Array.from(mk.queue?.items || []);
                const position = mk.queue?.position;
                const currentIndex = Number.isInteger(position) && position >= 0 && position < items.length ? position : -1;
                const item = mk.nowPlayingItem || items[currentIndex] || null;
                if (!item) {
                    throw new Error("No item currently loaded in player to resume");
                }
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
                const targetMs = Math.round(posSeconds * 1000);
                if (window.__malusPlaybackDiscontinuity) {
                    window.__malusPlaybackDiscontinuity('seek', targetMs);
                }
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

    async fn get_status(&self) -> Result<PlayerStatus, AppleError> {
        let session_guard = self.active_session.lock().await;
        let page = match *session_guard {
            Some(ref s) => {
                if let Ok(health) = s.runtime.check_health().await
                    && health.alive
                {
                    s.runtime.page_handle()
                } else {
                    return Ok(PlayerStatus::default());
                }
            }
            None => {
                return Ok(PlayerStatus::default());
            }
        };

        drop(session_guard);
        let val = page
            .evaluate("window.__malusPlaybackSnapshot()?.status")
            .await
            .map_err(AppleError::Web)?;
        parse_player_status(&val).ok_or_else(|| {
            AppleError::Internal("Failed to parse player status from MusicKit".to_string())
        })
    }

    async fn get_queue(&self) -> Result<Queue, AppleError> {
        let session_guard = self.active_session.lock().await;
        let page = match *session_guard {
            Some(ref s) => {
                if let Ok(health) = s.runtime.check_health().await
                    && health.alive
                {
                    s.runtime.page_handle()
                } else {
                    return Ok(Queue::new());
                }
            }
            None => {
                return Ok(Queue::new());
            }
        };

        drop(session_guard);
        let val = page
            .evaluate("window.__malusPlaybackSnapshot()?.queue")
            .await
            .map_err(AppleError::Web)?;
        parse_queue(&val)
    }

    async fn play_next(&self, kind: &str, id: &str) -> Result<(), AppleError> {
        let page = self.ensure_playback_session().await?;
        let func = r#"
            async function(kind, itemId) {
                const mk = window.MusicKit && window.MusicKit.getInstance();
                if (!mk) throw new Error("MusicKit instance not available");
                let descriptor = {};
                descriptor[kind] = itemId;
                await mk.playNext(descriptor);
            }
        "#;
        page.call_function(
            func,
            &[
                Value::String(kind.to_string()),
                Value::String(id.to_string()),
            ],
        )
        .await
        .map_err(|e| AppleError::PlaybackFailed(e.to_string()))?;
        Ok(())
    }

    async fn play_later(&self, kind: &str, id: &str) -> Result<(), AppleError> {
        let page = self.ensure_playback_session().await?;
        let func = r#"
            async function(kind, itemId) {
                const mk = window.MusicKit && window.MusicKit.getInstance();
                if (!mk) throw new Error("MusicKit instance not available");
                let descriptor = {};
                descriptor[kind] = itemId;
                await mk.playLater(descriptor);
                if (window.__malusPlaybackNotify) {
                    window.__malusPlaybackNotify();
                }
            }
        "#;
        page.call_function(
            func,
            &[
                Value::String(kind.to_string()),
                Value::String(id.to_string()),
            ],
        )
        .await
        .map_err(|e| AppleError::PlaybackFailed(e.to_string()))?;
        Ok(())
    }

    async fn queue_jump(&self, index: usize) -> Result<(), AppleError> {
        let page = self.ensure_playback_session().await?;
        let func = r#"
            async function(idx) {
                const mk = window.MusicKit && window.MusicKit.getInstance();
                if (!mk) throw new Error("MusicKit instance not available");
                const q = mk.queue;
                if (!q || idx < 0 || idx >= q.items.length) {
                    throw new Error("Queue index out of bounds: " + idx);
                }
                await mk.changeToMediaAtIndex(idx);
                if (window.__malusPlaybackNotify) {
                    window.__malusPlaybackNotify();
                }
            }
        "#;
        page.call_function(func, &[serde_json::json!(index)])
            .await
            .map_err(|e| AppleError::PlaybackFailed(e.to_string()))?;
        Ok(())
    }

    async fn queue_remove(&self, index: usize) -> Result<(), AppleError> {
        let page = self.ensure_playback_session().await?;
        let func = r#"
            async function(idx) {
                const mk = window.MusicKit && window.MusicKit.getInstance();
                if (!mk) throw new Error("MusicKit instance not available");
                const q = mk.queue;
                if (!q || idx < 0 || idx >= q.items.length) {
                    throw new Error("Queue index out of bounds: " + idx);
                }
                q.splice(idx, 1);
            }
        "#;
        page.call_function(func, &[serde_json::json!(index)])
            .await
            .map_err(|e| AppleError::PlaybackFailed(e.to_string()))?;
        Ok(())
    }

    async fn queue_move(&self, from: usize, to: usize) -> Result<(), AppleError> {
        let page = self.ensure_playback_session().await?;
        let func = r#"
            async function(fromIdx, toIdx) {
                const mk = window.MusicKit && window.MusicKit.getInstance();
                if (!mk) throw new Error("MusicKit instance not available");
                const q = mk.queue;
                if (!q) throw new Error("No active queue");
                const len = q.items.length;
                if (fromIdx < 0 || fromIdx >= len || toIdx < 0 || toIdx >= len) {
                    throw new Error("Queue index out of bounds: from=" + fromIdx + " to=" + toIdx);
                }
                const item = q.items[fromIdx];
                q.splice(fromIdx, 1);
                q.splice(toIdx, 0, [item]);
            }
        "#;
        page.call_function(func, &[serde_json::json!(from), serde_json::json!(to)])
            .await
            .map_err(|e| AppleError::PlaybackFailed(e.to_string()))?;
        Ok(())
    }

    async fn queue_clear_upcoming(&self) -> Result<(), AppleError> {
        let page = self.ensure_playback_session().await?;
        let func = r#"
            async function() {
                const mk = window.MusicKit && window.MusicKit.getInstance();
                if (!mk) throw new Error("MusicKit instance not available");
                const q = mk.queue;
                if (!q || q.items.length === 0) return;
                const pos = typeof q.position === "number" ? q.position : 0;
                // Remove everything after the current position
                if (pos + 1 < q.items.length) {
                    q.splice(pos + 1, q.items.length - pos - 1);
                }
            }
        "#;
        page.call_function(func, &[])
            .await
            .map_err(|e| AppleError::PlaybackFailed(e.to_string()))?;
        Ok(())
    }

    async fn skip_to_next(&self) -> Result<(), AppleError> {
        let page = self.ensure_playback_session().await?;
        let func = r#"
            async function() {
                const mk = window.MusicKit && window.MusicKit.getInstance();
                if (!mk) throw new Error("MusicKit instance not available");
                if (window.__malusPlaybackDiscontinuity) {
                    window.__malusPlaybackDiscontinuity('next');
                }
                const currentRepeat = mk.repeatMode;
                if (currentRepeat === 1) {
                    mk.repeatMode = 0;
                }
                try {
                    await mk.skipToNextItem();
                } finally {
                    if (currentRepeat === 1) {
                        mk.repeatMode = 1;
                    }
                }
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

    async fn skip_to_previous(&self) -> Result<(), AppleError> {
        let page = self.ensure_playback_session().await?;
        let func = r#"
            async function() {
                const mk = window.MusicKit && window.MusicKit.getInstance();
                if (!mk) throw new Error("MusicKit instance not available");
                if (window.__malusPlaybackDiscontinuity) {
                    window.__malusPlaybackDiscontinuity('previous');
                }
                const currentRepeat = mk.repeatMode;
                if (currentRepeat === 1) {
                    mk.repeatMode = 0;
                }
                try {
                    await mk.skipToPreviousItem();
                } finally {
                    if (currentRepeat === 1) {
                        mk.repeatMode = 1;
                    }
                }
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

    fn set_event_sink(&self, sink: mpsc::UnboundedSender<PlaybackEvent>) {
        let sink_lock = self.event_sink.clone();
        tokio::spawn(async move {
            *sink_lock.lock().await = Some(sink);
        });
    }
}

pub use crate::api::{
    parse_apple_album, parse_apple_artist, parse_apple_artwork, parse_apple_playlist,
    parse_apple_track,
};

#[cfg(test)]
mod playback_normalization_tests {
    use super::*;
    #[test]
    fn only_raw_playing_state_allows_progress() {
        for raw in 0..=10 {
            let status = parse_player_status(&serde_json::json!({
                "playbackState": raw, "isPlaying": true, "positionMs": 2500, "volume": 0
            }))
            .unwrap();
            assert_eq!(
                status.state == PlaybackState::Playing,
                raw == 2,
                "raw state {raw}"
            );
            assert_eq!(status.volume, 0);
        }
        assert!(parse_player_status(&Value::Null).is_none());
    }
    #[test]
    fn malformed_queue_cannot_shift_indexes_silently() {
        assert!(
            parse_queue(&serde_json::json!({"items":[{"id":"a"},{}],"currentIndex":1})).is_err()
        );
        assert!(parse_queue(&serde_json::json!({"items":[{"id":"a"}],"currentIndex":5})).is_err());
        let queue =
            parse_queue(&serde_json::json!({"items":[{"id":"a"},{"id":"a"}],"currentIndex":1}))
                .unwrap();
        assert_eq!(queue.current_index, Some(1));
    }
}
