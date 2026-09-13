//! Apple Music web session manager and MusicKit authorization adapter.
//!
//! Exposes `AppleWebSession` trait as a testability seam, decoupling provider
//! unit testing from live browser automation and network calls.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use malus_protocol::{
    PlaybackStateWire, PlayerStatusWire, RepeatModeWire,
    provider::ProviderEvent,
    wire::{AlbumRefWire, TrackWire},
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

    /// Refresh and return current Apple Music API credentials.
    async fn refresh_tokens(&self) -> Result<crate::api::AppleCredentials, AppleError>;

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

struct ActiveSession {
    runtime: WebRuntime,
}

/// Production implementation backed by `malus-web-runtime`.
pub struct ProductionAppleWebSession {
    profile_lock: Arc<Mutex<()>>,
    active_session: Arc<Mutex<Option<ActiveSession>>>,
    event_sink: Arc<Mutex<Option<mpsc::UnboundedSender<ProviderEvent>>>>,
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

    // 6. Configure MusicKit via call_function with structured CDP arguments
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
    )
}

impl ProductionAppleWebSession {
    pub fn new() -> Self {
        Self {
            profile_lock: Arc::new(Mutex::new(())),
            active_session: Arc::new(Mutex::new(None)),
            event_sink: Arc::new(Mutex::new(None)),
        }
    }

    async fn ensure_playback_session(&self) -> Result<Arc<WebPage>, AppleError> {
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
        let _ = page.register_event_sink(MUSICKIT_PLAYBACK_SINK).await;
        let mut page_events = page.subscribe_events();
        let _ = page.evaluate(&build_inject_playback_bridge_script()).await;

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
        *session_guard = Some(ActiveSession { runtime });
        Ok(handle)
    }
}

fn parse_player_status(payload: &Value) -> Option<PlayerStatusWire> {
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
        let mut track = TrackWire::new(
            media_id,
            t["title"].as_str().unwrap_or_default(),
            t["artist"].as_str().unwrap_or_default(),
        );
        if let Some(alb) = t["album"].as_str()
            && !alb.is_empty()
        {
            track.album = Some(AlbumRefWire::titled(alb));
        }
        if duration_ms > 0 {
            track.duration_ms = Some(duration_ms);
        }
        Some(track)
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

    async fn play_track(&self, catalog_id: &str) -> Result<(), AppleError> {
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
            async function(trackId) {
                const mk = window.MusicKit && window.MusicKit.getInstance();
                if (!mk) throw new Error("MusicKit instance not available");
                if (mk.assertUserStorefront) {
                    mk.assertUserStorefront = () => {};
                }

                let idsToTry = [trackId];
                if (trackId.startsWith("i.") || trackId.startsWith("l.")) {
                    try {
                        const res = await mk.api.music(`/v1/me/library/songs/${trackId}`);
                        const item = res?.data?.data?.[0];
                        const catId = item?.attributes?.playParams?.catalogId;
                        if (catId && !idsToTry.includes(catId)) {
                            idsToTry.unshift(catId);
                        }
                    } catch (_) {}
                }

                let lastErr = null;
                let queued = false;
                for (const id of idsToTry) {
                    const descriptors = [
                        { song: id },
                        { songs: [id] }
                    ];
                    for (const desc of descriptors) {
                        try {
                            await mk.setQueue(desc);
                            queued = true;
                            break;
                        } catch (e) {
                            lastErr = e;
                        }
                    }
                    if (queued) break;
                }
                if (!queued) {
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
                if (mk.assertUserStorefront) {
                    mk.assertUserStorefront = () => {};
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
        let session_guard = self.active_session.lock().await;
        let page = match *session_guard {
            Some(ref s) => {
                if let Ok(health) = s.runtime.check_health().await
                    && health.alive
                {
                    s.runtime.page_handle()
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

pub use crate::api::{
    parse_apple_album, parse_apple_artist, parse_apple_artwork, parse_apple_playlist,
    parse_apple_track,
};
