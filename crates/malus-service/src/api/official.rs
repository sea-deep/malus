//! Native Apple Music HTTP API client (`OfficialAppleMusicApi`).
//!
//! Exposes official metadata, catalog, collection, and personal library operations
//! over direct HTTP against `https://api.music.apple.com/v1`.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use malus_ipc::wire::{
    CatalogItemWire, LibraryKindWire, LibraryPageWire, PagedListWire, SearchKindWire,
    SearchResultsWire,
};
use malus_model::{
    AccountMediaState, Album, Artist, Credits, Lyrics, MediaRef, Playlist, Rating, Track,
};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT};
use serde_json::Value;
use tracing::{debug, info, warn};

use crate::api::{
    credentials::{AppleCredentials, TokenProvider},
    credits::parse_apple_credits,
    error::AppleApiError,
    lyrics::parse_ttml_lyrics,
    parse::{parse_apple_album, parse_apple_artist, parse_apple_playlist, parse_apple_track},
};

pub const DEFAULT_APPLE_API_BASE: &str = "https://api.music.apple.com/v1";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/133.0.0.0 Safari/537.36";

/// Native client for the official Apple Music API (`api.music.apple.com`).
pub struct OfficialAppleMusicApi {
    client: reqwest::Client,
    base_url: String,
    token_provider: Arc<dyn TokenProvider>,
    catalog_references: tokio::sync::Mutex<std::collections::HashMap<MediaRef, MediaRef>>,
}

impl OfficialAppleMusicApi {
    /// Create a new official Apple Music API client with default base URL and settings.
    pub fn new(token_provider: Arc<dyn TokenProvider>) -> Self {
        Self::with_base_url(token_provider, DEFAULT_APPLE_API_BASE)
    }

    /// Create with a custom base URL (e.g. for testing against a local mock server).
    pub fn with_base_url(
        token_provider: Arc<dyn TokenProvider>,
        base_url: impl Into<String>,
    ) -> Self {
        let client = reqwest::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .build()
            .unwrap_or_default();
        Self::with_client_and_base_url(client, base_url, token_provider)
    }

    /// Create with an explicit reqwest client and base URL.
    pub fn with_client_and_base_url(
        client: reqwest::Client,
        base_url: impl Into<String>,
        token_provider: Arc<dyn TokenProvider>,
    ) -> Self {
        Self {
            client,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            token_provider,
            catalog_references: tokio::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Retrieve the active storefront identifier, preferring environment overrides then user credentials.
    pub fn storefront(&self) -> String {
        if let Ok(env_sf) = std::env::var("MALUS_APPLE_STOREFRONT") {
            let env_sf = env_sf.trim().to_lowercase();
            if !env_sf.is_empty() {
                return env_sf;
            }
        }
        if let Ok(creds) = self.token_provider.get_credentials()
            && !creds.storefront.is_empty()
        {
            return creds.storefront.to_lowercase();
        }
        "us".to_string()
    }

    /// Build the full URL for an API path or cursor.
    fn build_url(&self, path_or_url: &str, storefront: &str) -> String {
        let sf = if let Ok(env_sf) = std::env::var("MALUS_APPLE_STOREFRONT") {
            let env_sf = env_sf.trim().to_lowercase();
            if !env_sf.is_empty() {
                env_sf
            } else if storefront.is_empty() {
                "us".to_string()
            } else {
                storefront.to_string()
            }
        } else if storefront.is_empty() {
            "us".to_string()
        } else {
            storefront.to_string()
        };

        let resolved = path_or_url.replace("{storefront}", &sf);

        // When testing with a mock base URL (e.g. http://127.0.0.1:...), rewrite
        // absolute Apple Music URLs to the mock server to prevent hitting real Apple servers.
        let is_mock_base = !self.base_url.contains("apple.com");
        if is_mock_base {
            let path = if let Some(stripped) =
                resolved.strip_prefix("https://amp-api.music.apple.com/v1")
            {
                stripped
            } else if let Some(stripped) =
                resolved.strip_prefix("https://amp-api-edge.music.apple.com/v1")
            {
                stripped
            } else if let Some(stripped) =
                resolved.strip_prefix("https://amp-api-edge.media.apple.com/v1")
            {
                stripped
            } else if let Some(stripped) = resolved.strip_prefix("https://api.music.apple.com/v1") {
                stripped
            } else if let Some(stripped) = resolved.strip_prefix("http://") {
                stripped.split_once('/').map(|(_, p)| p).unwrap_or("")
            } else if let Some(stripped) = resolved.strip_prefix("https://") {
                stripped.split_once('/').map(|(_, p)| p).unwrap_or("")
            } else {
                resolved.as_str()
            };
            let clean_path = path.strip_prefix("/v1").unwrap_or(path);
            return format!("{}/{}", self.base_url, clean_path.trim_start_matches('/'));
        }

        if resolved.starts_with("http://") || resolved.starts_with("https://") {
            return resolved;
        }

        let clean_path = resolved.strip_prefix("/v1").unwrap_or(&resolved);
        format!("{}/{}", self.base_url, clean_path.trim_start_matches('/'))
    }

    /// Build HTTP headers with strict secret isolation.
    fn build_headers(&self, creds: &AppleCredentials) -> Result<HeaderMap, AppleApiError> {
        let mut headers = HeaderMap::new();

        let auth_val = format!("Bearer {}", creds.developer_token);
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&auth_val).map_err(|e| {
                AppleApiError::AuthRequired(format!("Invalid characters in developer token: {e}"))
            })?,
        );

        if !creds.music_user_token.is_empty() {
            let user_token_hdr = HeaderValue::from_str(&creds.music_user_token).map_err(|e| {
                AppleApiError::AuthRequired(format!("Invalid characters in music user token: {e}"))
            })?;
            headers.insert("music-user-token", user_token_hdr.clone());
            headers.insert("media-user-token", user_token_hdr);
        }

        headers.insert(
            "origin",
            HeaderValue::from_static("https://music.apple.com"),
        );
        headers.insert(
            "referer",
            HeaderValue::from_static("https://music.apple.com/"),
        );
        headers.insert(
            "x-apple-client-version",
            HeaderValue::from_static("2638.7.0-external"),
        );
        headers.insert(USER_AGENT, HeaderValue::from_static(DEFAULT_USER_AGENT));

        Ok(headers)
    }

    /// Execute a single HTTP request against Apple Music API.
    async fn execute_single_request(
        &self,
        method: reqwest::Method,
        url: &str,
        query: &[(&str, &str)],
        body: Option<&Value>,
        creds: &AppleCredentials,
    ) -> Result<reqwest::Response, AppleApiError> {
        let mut attempts = 0;
        loop {
            attempts += 1;
            let headers = self.build_headers(creds)?;
            let mut req = self.client.request(method.clone(), url).headers(headers);
            if !query.is_empty() {
                req = req.query(query);
            }
            if let Some(b) = body {
                req = req.json(b);
            } else if method == reqwest::Method::POST || method == reqwest::Method::PUT {
                req = req.header(reqwest::header::CONTENT_LENGTH, "0");
            }

            match req.send().await {
                Ok(resp) => return Ok(resp),
                Err(err) if attempts <= 2 => {
                    warn!(
                        "Transient network error on request (attempt {attempts}): {err}; retrying..."
                    );
                    tokio::time::sleep(Duration::from_millis(150 * attempts)).await;
                }
                Err(err) => return Err(AppleApiError::from(err)),
            }
        }
    }

    /// Canonical request pipeline with 401 token refresh and retry for arbitrary HTTP methods.
    pub async fn send_request_with_method(
        &self,
        method: reqwest::Method,
        path_or_url: &str,
        query: &[(&str, &str)],
        body: Option<&Value>,
    ) -> Result<Value, AppleApiError> {
        let mut creds = self.token_provider.get_credentials()?;
        if creds.developer_token.is_empty() || creds.music_user_token.is_empty() {
            return Err(AppleApiError::AuthRequired(
                "Missing Apple Music credentials".to_string(),
            ));
        }

        let url = self.build_url(path_or_url, &creds.storefront);
        let start = Instant::now();

        let mut resp = self
            .execute_single_request(method.clone(), &url, query, body, &creds)
            .await?;

        // HTTP 401 Handling: Refresh credentials and retry exactly once
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            warn!(
                path = %path_or_url,
                "Received HTTP 401 Unauthorized from Apple Music API; attempting credential refresh..."
            );

            match self.token_provider.refresh_credentials().await {
                Ok(new_creds) => {
                    creds = new_creds;
                    let retry_url = self.build_url(path_or_url, &creds.storefront);
                    resp = self
                        .execute_single_request(method.clone(), &retry_url, query, body, &creds)
                        .await?;
                }
                Err(refresh_err) => {
                    warn!("Apple token refresh failed: {refresh_err}");
                    return Err(AppleApiError::AuthRequired(format!(
                        "Session expired and token refresh failed: {refresh_err}"
                    )));
                }
            }
        }

        let status = resp.status();
        let elapsed = start.elapsed();

        if status == reqwest::StatusCode::NOT_FOUND && path_or_url.contains("/v1/me/ratings/") {
            debug!(
                method = %method,
                path = %path_or_url,
                status = %status.as_u16(),
                elapsed_ms = %elapsed.as_millis(),
                "Apple Music API request completed (unrated item)"
            );
        } else {
            info!(
                method = %method,
                path = %path_or_url,
                status = %status.as_u16(),
                elapsed_ms = %elapsed.as_millis(),
                "Apple Music API request completed"
            );
        }

        if status.is_success() {
            if status == reqwest::StatusCode::NO_CONTENT {
                return Ok(Value::Null);
            }
            let body_str = resp.text().await.map_err(|e| {
                AppleApiError::Network(format!("Failed to read response body: {e}"))
            })?;
            if body_str.trim().is_empty() {
                return Ok(Value::Null);
            }
            serde_json::from_str::<Value>(&body_str).map_err(|e| {
                AppleApiError::Parse(format!(
                    "Failed to parse response JSON: {e} (status: {status}, body: '{body_str}')"
                ))
            })
        } else if status == reqwest::StatusCode::UNAUTHORIZED {
            let body_str = resp.text().await.unwrap_or_default();
            Err(AppleApiError::AuthRequired(format!(
                "Apple Music session expired or unauthorized (status: {status}, body: '{body_str}')"
            )))
        } else if status == reqwest::StatusCode::FORBIDDEN {
            let body_str = resp.text().await.unwrap_or_default();
            Err(AppleApiError::Forbidden(body_str))
        } else if status == reqwest::StatusCode::NOT_FOUND {
            let body_str = resp.text().await.unwrap_or_default();
            Err(AppleApiError::NotFound(body_str))
        } else if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .map(Duration::from_secs);
            Err(AppleApiError::RateLimited { retry_after })
        } else if status.is_server_error() {
            let body_str = resp.text().await.unwrap_or_default();
            Err(AppleApiError::Server {
                status: status.as_u16(),
                message: body_str,
            })
        } else {
            let body_str = resp.text().await.unwrap_or_default();
            Err(AppleApiError::Other(format!(
                "HTTP {}: {}",
                status.as_u16(),
                body_str
            )))
        }
    }

    /// Canonical GET request pipeline with 401 token refresh and retry.
    pub async fn send_request(
        &self,
        path_or_url: &str,
        query: &[(&str, &str)],
    ) -> Result<Value, AppleApiError> {
        self.send_request_with_method(reqwest::Method::GET, path_or_url, query, None)
            .await
    }

    /// Helper to get Apple plural resource kind string from MediaRef.
    pub fn media_kind_plural(reference: &MediaRef) -> Result<&'static str, AppleApiError> {
        match reference {
            MediaRef::Song(_) => Ok("songs"),
            MediaRef::Album(_) => Ok("albums"),
            MediaRef::Playlist(id) if id.starts_with("p.") => Ok("library-playlists"),
            MediaRef::Playlist(_) => Ok("playlists"),
            MediaRef::Artist(_) => Ok("artists"),
            MediaRef::Station(_) => Err(AppleApiError::Other(
                "Stations do not support account mutations".to_string(),
            )),
        }
    }

    /// Resolve catalog-only operations through Apple's explicit relationship.
    /// Keep the caller's library reference for UI identity and event matching.
    pub async fn catalog_reference(&self, reference: &MediaRef) -> Result<MediaRef, AppleApiError> {
        if !matches!(reference, MediaRef::Song(id) if id.starts_with("i.") || id.starts_with("l."))
        {
            return Ok(reference.clone());
        }
        if let Some(cached) = self.catalog_references.lock().await.get(reference).cloned() {
            return Ok(cached);
        }
        let path = format!("/v1/me/library/songs/{}", reference.id());
        let response = self.send_request(&path, &[("include", "catalog")]).await?;
        let item = response
            .get("data")
            .and_then(Value::as_array)
            .and_then(|items| items.first());
        let catalog_id = item
            .and_then(|item| {
                item.pointer("/relationships/catalog/data/0/id")
                    .or_else(|| item.pointer("/attributes/playParams/catalogId"))
            })
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty());
        let id = catalog_id.ok_or_else(|| {
            AppleApiError::NotFound("This library song has no catalog equivalent".into())
        })?;
        let resolved = MediaRef::Song(id.to_string());
        let mut cache = self.catalog_references.lock().await;
        if cache.len() >= 256 {
            cache.clear();
        }
        cache.insert(reference.clone(), resolved.clone());
        Ok(resolved)
    }

    /// Fetch time-synced or unsynced lyrics for a catalog song.
    pub async fn get_lyrics(&self, song_id: &str) -> Result<Lyrics, AppleApiError> {
        let reference = self
            .catalog_reference(&MediaRef::Song(song_id.to_string()))
            .await?;
        let song_id = reference.id();
        let path = format!(
            "https://amp-api.music.apple.com/v1/catalog/{{storefront}}/songs/{song_id}/lyrics"
        );
        match self.send_request(&path, &[]).await {
            Ok(val) => {
                if let Some(data) = val.get("data").and_then(|d| d.as_array())
                    && let Some(first) = data.first()
                    && let Some(ttml) = first
                        .get("attributes")
                        .and_then(|a| a.get("ttml"))
                        .and_then(|t| t.as_str())
                {
                    return Ok(parse_ttml_lyrics(ttml));
                }
                Err(AppleApiError::NotFound(format!(
                    "Lyrics unavailable for song {song_id}"
                )))
            }
            Err(AppleApiError::NotFound(_)) => {
                // Try syllable lyrics fallback
                let syl_path = format!(
                    "https://amp-api.music.apple.com/v1/catalog/{{storefront}}/songs/{song_id}/syllable-lyrics"
                );
                if let Ok(val) = self.send_request(&syl_path, &[]).await
                    && let Some(data) = val.get("data").and_then(|d| d.as_array())
                    && let Some(first) = data.first()
                    && let Some(ttml) = first
                        .get("attributes")
                        .and_then(|a| a.get("ttml"))
                        .and_then(|t| t.as_str())
                {
                    return Ok(parse_ttml_lyrics(ttml));
                }
                Err(AppleApiError::NotFound(format!(
                    "Lyrics unavailable for song {song_id}"
                )))
            }
            Err(e) => Err(e),
        }
    }

    /// Fetch song credits for a catalog song.
    pub async fn get_credits(&self, song_id: &str) -> Result<Credits, AppleApiError> {
        let reference = self
            .catalog_reference(&MediaRef::Song(song_id.to_string()))
            .await?;
        let song_id = reference.id();
        let path = format!(
            "https://amp-api.music.apple.com/v1/catalog/{{storefront}}/songs/{song_id}/credits"
        );
        let val = self.send_request(&path, &[]).await?;
        let credits = parse_apple_credits(&val);
        if credits.is_empty() {
            return Err(AppleApiError::NotFound(format!(
                "Credits unavailable for song {song_id}"
            )));
        }
        Ok(credits)
    }

    /// Favorite an item (song, album, playlist).
    pub async fn favorite(&self, reference: &MediaRef) -> Result<(), AppleApiError> {
        let resolved = self.catalog_reference(reference).await?;
        let reference = &resolved;
        let kind = Self::media_kind_plural(reference)?;
        let query_key = format!("ids[{kind}]");
        let id_val = reference.id();
        self.send_request_with_method(
            reqwest::Method::POST,
            "/v1/me/favorites",
            &[(&query_key, id_val)],
            None,
        )
        .await?;
        Ok(())
    }

    /// Unfavorite an item (song, album, playlist) using amp-api.
    pub async fn unfavorite(&self, reference: &MediaRef) -> Result<(), AppleApiError> {
        let resolved = self.catalog_reference(reference).await?;
        let reference = &resolved;
        let kind = Self::media_kind_plural(reference)?;
        let query_key = format!("ids[{kind}]");
        let id_val = reference.id();
        self.send_request_with_method(
            reqwest::Method::DELETE,
            "https://amp-api.music.apple.com/v1/me/favorites",
            &[(&query_key, id_val)],
            None,
        )
        .await?;
        Ok(())
    }

    /// Suggest less for an item (negative rating -1).
    pub async fn suggest_less(&self, reference: &MediaRef) -> Result<(), AppleApiError> {
        if matches!(reference, MediaRef::Playlist(id) if id.starts_with("p.")) {
            return Ok(());
        }
        let resolved = self.catalog_reference(reference).await?;
        let reference = &resolved;
        let kind = Self::media_kind_plural(reference)?;
        let path = format!("/v1/me/ratings/{kind}/{}", reference.id());
        let body = serde_json::json!({
            "type": "rating",
            "attributes": { "value": -1 }
        });
        self.send_request_with_method(reqwest::Method::PUT, &path, &[], Some(&body))
            .await?;
        Ok(())
    }

    /// Clear rating (set to neutral).
    pub async fn clear_rating(&self, reference: &MediaRef) -> Result<(), AppleApiError> {
        if matches!(reference, MediaRef::Playlist(id) if id.starts_with("p.")) {
            return Ok(());
        }
        let resolved = self.catalog_reference(reference).await?;
        let reference = &resolved;
        let kind = Self::media_kind_plural(reference)?;
        let path = format!("/v1/me/ratings/{kind}/{}", reference.id());
        self.send_request_with_method(reqwest::Method::DELETE, &path, &[], None)
            .await?;
        Ok(())
    }

    /// Add an item (song, album, playlist) to the user's library.
    pub async fn add_to_library(&self, reference: &MediaRef) -> Result<(), AppleApiError> {
        if matches!(reference, MediaRef::Playlist(id) if id.starts_with("p."))
            || matches!(reference, MediaRef::Song(id) if id.starts_with("i."))
            || matches!(reference, MediaRef::Album(id) if id.starts_with("l."))
        {
            return Ok(());
        }
        let resolved = self.catalog_reference(reference).await?;
        let reference = &resolved;
        let kind = match reference {
            MediaRef::Song(_) => "songs",
            MediaRef::Album(_) => "albums",
            MediaRef::Playlist(_) => "playlists",
            _ => {
                return Err(AppleApiError::Other(format!(
                    "Library add not supported for {}",
                    reference.kind()
                )));
            }
        };
        let query_key = format!("ids[{kind}]");
        let id_val = reference.id();
        self.send_request_with_method(
            reqwest::Method::POST,
            "/v1/me/library",
            &[(&query_key, id_val)],
            None,
        )
        .await?;
        Ok(())
    }

    /// Create a new library playlist.
    /// Supports empty playlists (omitting `relationships.tracks`) or seeded with `initial_tracks`.
    pub async fn create_playlist(
        &self,
        name: &str,
        description: Option<&str>,
        initial_tracks: &[MediaRef],
    ) -> Result<Playlist, AppleApiError> {
        let trimmed_name = name.trim();
        if trimmed_name.is_empty() {
            return Err(AppleApiError::Other(
                "Playlist name cannot be empty".to_string(),
            ));
        }

        let mut attrs = serde_json::json!({
            "name": trimmed_name,
        });
        if let Some(desc) = description.map(str::trim).filter(|d| !d.is_empty()) {
            attrs["description"] = serde_json::Value::String(desc.to_string());
        }

        let mut body = serde_json::json!({
            "attributes": attrs,
        });

        if !initial_tracks.is_empty() {
            let tracks_data: Vec<_> = initial_tracks
                .iter()
                .map(|t| {
                    let raw_id = t.id();
                    let typ = if raw_id.starts_with("i.m") {
                        "library-songs"
                    } else {
                        "songs"
                    };
                    serde_json::json!({
                        "id": raw_id,
                        "type": typ,
                    })
                })
                .collect();
            body["relationships"] = serde_json::json!({
                "tracks": {
                    "data": tracks_data,
                }
            });
        }

        let val = self
            .send_request_with_method(
                reqwest::Method::POST,
                "/v1/me/library/playlists",
                &[],
                Some(&body),
            )
            .await?;

        let first = val
            .get("data")
            .and_then(|d| d.as_array())
            .and_then(|a| a.first())
            .ok_or_else(|| {
                AppleApiError::Parse("Missing data array in create playlist response".to_string())
            })?;

        crate::api::parse::parse_apple_playlist(first).ok_or_else(|| {
            AppleApiError::Parse("Failed to parse newly created playlist".to_string())
        })
    }

    /// Add track(s) to an existing library playlist.
    pub async fn add_tracks_to_playlist(
        &self,
        playlist: &MediaRef,
        tracks: &[MediaRef],
    ) -> Result<(), AppleApiError> {
        if tracks.is_empty() {
            return Ok(());
        }

        let playlist_id = playlist.id();
        let path = format!("/v1/me/library/playlists/{playlist_id}/tracks");

        let tracks_data: Vec<_> = tracks
            .iter()
            .map(|t| {
                let raw_id = t.id();
                let typ = if raw_id.starts_with("i.") {
                    "library-songs"
                } else {
                    "songs"
                };
                serde_json::json!({
                    "id": raw_id,
                    "type": typ,
                })
            })
            .collect();

        let body = serde_json::json!({
            "data": tracks_data,
        });

        self.send_request_with_method(reqwest::Method::POST, &path, &[], Some(&body))
            .await?;

        Ok(())
    }

    pub async fn resolve_library_playlist_id(
        &self,
        catalog_or_lib_id: &str,
    ) -> Result<String, AppleApiError> {
        if catalog_or_lib_id.starts_with("p.") {
            return Ok(catalog_or_lib_id.to_string());
        }
        let mut next_url = Some("/v1/me/library/playlists?limit=100".to_string());
        let mut pages = 0;
        while let Some(url) = next_url {
            if pages >= 5 {
                break;
            }
            pages += 1;
            let res = self.send_request(&url, &[]).await?;
            if let Some(arr) = res.get("data").and_then(|d| d.as_array()) {
                for item in arr {
                    let lib_id = item.get("id").and_then(|i| i.as_str());
                    let global_id = item
                        .pointer("/attributes/playParams/globalId")
                        .or_else(|| item.pointer("/attributes/playParams/catalogId"))
                        .or_else(|| item.pointer("/relationships/catalog/data/0/id"))
                        .and_then(|g| g.as_str());
                    if (global_id == Some(catalog_or_lib_id) || lib_id == Some(catalog_or_lib_id))
                        && let Some(lid) = lib_id
                    {
                        return Ok(lid.to_string());
                    }
                }
            }
            next_url = res.get("next").and_then(|n| n.as_str()).map(str::to_string);
        }
        Err(AppleApiError::NotFound(format!(
            "Playlist {catalog_or_lib_id} is not in library"
        )))
    }

    /// Delete an existing user library playlist.
    pub async fn delete_playlist(&self, playlist: &MediaRef) -> Result<(), AppleApiError> {
        let raw_id = playlist.id();
        let playlist_id = self.resolve_library_playlist_id(raw_id).await?;
        let url = format!("https://amp-api.music.apple.com/v1/me/library/playlists/{playlist_id}");
        self.send_request_with_method(reqwest::Method::DELETE, &url, &[], None)
            .await?;
        Ok(())
    }

    /// Update playlist metadata (name, description).
    pub async fn update_playlist(
        &self,
        playlist: &MediaRef,
        name: &str,
        description: Option<&str>,
    ) -> Result<(), AppleApiError> {
        let playlist_id = playlist.id();
        let url = format!("https://amp-api.music.apple.com/v1/me/library/playlists/{playlist_id}");
        let trimmed_name = name.trim();
        if trimmed_name.is_empty() {
            return Err(AppleApiError::Other(
                "Playlist name cannot be empty".to_string(),
            ));
        }
        let mut attrs = serde_json::json!({
            "name": trimmed_name,
        });
        if let Some(desc) = description.map(str::trim).filter(|d| !d.is_empty()) {
            attrs["description"] = serde_json::Value::String(desc.to_string());
        }
        let body = serde_json::json!({
            "attributes": attrs,
        });
        self.send_request_with_method(reqwest::Method::PATCH, &url, &[], Some(&body))
            .await?;
        Ok(())
    }

    /// Remove a track from an editable playlist at the specified index,
    /// verifying that the item at `track_index` matches `expected_track`.
    pub async fn remove_track_from_playlist(
        &self,
        playlist: &MediaRef,
        track_index: usize,
        expected_track: &MediaRef,
    ) -> Result<(), AppleApiError> {
        let playlist_id = playlist.id();
        let path = format!("/v1/me/library/playlists/{playlist_id}/tracks");
        let val = self.send_request(&path, &[("limit", "100")]).await?;
        let data = val
            .get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| AppleApiError::Parse("Missing tracks data in playlist".to_string()))?;

        if track_index >= data.len() {
            return Err(AppleApiError::Other(format!(
                "Track index {track_index} out of bounds (playlist has {} tracks)",
                data.len()
            )));
        }

        // Verify identity: compare item id or catalogId to expected_track
        let target_item = &data[track_index];
        let item_id = target_item["id"].as_str().unwrap_or_default();
        let catalog_id = target_item["attributes"]["playParams"]["catalogId"]
            .as_str()
            .unwrap_or_default();
        let exp_id = expected_track.id();

        if item_id != exp_id && catalog_id != exp_id {
            return Err(AppleApiError::Other(format!(
                "Track identity mismatch at index {track_index}: expected {exp_id}, found id='{item_id}', catalogId='{catalog_id}'"
            )));
        }

        // Build new track list omitting track_index
        let mut new_tracks = Vec::with_capacity(data.len().saturating_sub(1));
        for (i, item) in data.iter().enumerate() {
            if i == track_index {
                continue;
            }
            let id = item["id"].as_str().unwrap_or_default();
            let typ = item["type"].as_str().unwrap_or("library-songs");
            new_tracks.push(serde_json::json!({
                "id": id,
                "type": typ,
            }));
        }

        let body = serde_json::json!({
            "data": new_tracks,
        });

        let url =
            format!("https://amp-api.music.apple.com/v1/me/library/playlists/{playlist_id}/tracks");
        self.send_request_with_method(reqwest::Method::PUT, &url, &[], Some(&body))
            .await?;
        Ok(())
    }

    /// Remove an item (song, album, playlist) from the user's library.
    pub async fn remove_from_library(&self, reference: &MediaRef) -> Result<(), AppleApiError> {
        match reference {
            MediaRef::Playlist(_) => self.delete_playlist(reference).await,
            MediaRef::Song(id) => {
                let lib_id = if id.starts_with("i.") {
                    id.clone()
                } else {
                    let cat_path = format!("/v1/catalog/{{storefront}}/songs/{id}");
                    let val = self
                        .send_request(&cat_path, &[("relate", "library")])
                        .await?;
                    val.get("data")
                        .and_then(|d| d.as_array())
                        .and_then(|a| a.first())
                        .and_then(|i| i.get("relationships"))
                        .and_then(|r| r.get("library"))
                        .and_then(|l| l.get("data"))
                        .and_then(|d| d.as_array())
                        .and_then(|a| a.first())
                        .and_then(|x| x.get("id"))
                        .and_then(|i| i.as_str())
                        .map(str::to_string)
                        .ok_or_else(|| {
                            AppleApiError::NotFound(format!("Song {id} is not in library"))
                        })?
                };
                let url = format!("https://amp-api.music.apple.com/v1/me/library/songs/{lib_id}");
                self.send_request_with_method(reqwest::Method::DELETE, &url, &[], None)
                    .await?;
                Ok(())
            }
            MediaRef::Album(id) => {
                let lib_id = if id.starts_with("l.") {
                    id.clone()
                } else {
                    let cat_path = format!("/v1/catalog/{{storefront}}/albums/{id}");
                    let val = self
                        .send_request(&cat_path, &[("relate", "library")])
                        .await?;
                    val.get("data")
                        .and_then(|d| d.as_array())
                        .and_then(|a| a.first())
                        .and_then(|i| i.get("relationships"))
                        .and_then(|r| r.get("library"))
                        .and_then(|l| l.get("data"))
                        .and_then(|d| d.as_array())
                        .and_then(|a| a.first())
                        .and_then(|x| x.get("id"))
                        .and_then(|i| i.as_str())
                        .map(str::to_string)
                        .ok_or_else(|| {
                            AppleApiError::NotFound(format!("Album {id} is not in library"))
                        })?
                };
                let url = format!("https://amp-api.music.apple.com/v1/me/library/albums/{lib_id}");
                self.send_request_with_method(reqwest::Method::DELETE, &url, &[], None)
                    .await?;
                Ok(())
            }
            _ => Err(AppleApiError::Other(format!(
                "Removing {} from library is not supported",
                reference.kind()
            ))),
        }
    }

    /// Get current account state (in_library, favorite, rating) for a resource.
    pub async fn get_account_media_state(
        &self,
        reference: &MediaRef,
    ) -> Result<AccountMediaState, AppleApiError> {
        let original_reference = reference;
        let resolved = self.catalog_reference(reference).await?;
        let reference = &resolved;
        let kind = match Self::media_kind_plural(reference) {
            Ok(k) => k,
            Err(_) => {
                return Ok(AccountMediaState::new(
                    reference.clone(),
                    false,
                    false,
                    Rating::Neutral,
                ));
            }
        };

        if kind == "library-playlists" {
            let lib_path = format!("/v1/me/library/playlists/{}", reference.id());
            let mut favorite = false;
            if let Ok(val) = self
                .send_request(
                    &lib_path,
                    &[("fields[library-playlists]", "inFavorites,isFavorite")],
                )
                .await
                && let Some(data) = val.get("data").and_then(|d| d.as_array())
                && let Some(first) = data.first()
            {
                favorite = first
                    .get("attributes")
                    .and_then(|a| a.get("inFavorites").or_else(|| a.get("isFavorite")))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
            }
            return Ok(AccountMediaState::new(
                original_reference.clone(),
                true,
                favorite,
                Rating::Neutral,
            ));
        }

        let cat_path = format!(
            "https://amp-api.music.apple.com/v1/catalog/{{storefront}}/{kind}/{}",
            reference.id()
        );
        let fields_param = format!("fields[{kind}]");
        let cat_query = [
            ("relate", "library"),
            (&fields_param, "inLibrary,personalRating"),
        ];

        let mut in_library = false;
        let mut personal_rating: Option<i64> = None;

        if let Ok(val) = self.send_request(&cat_path, &cat_query).await
            && let Some(data) = val.get("data").and_then(|d| d.as_array())
            && let Some(first) = data.first()
        {
            if let Some(in_lib) = first
                .get("attributes")
                .and_then(|a| a.get("inLibrary"))
                .and_then(|v| v.as_bool())
            {
                in_library = in_lib;
            } else if let Some(lib) = first.get("relationships").and_then(|r| r.get("library"))
                && let Some(lib_data) = lib.get("data").and_then(|d| d.as_array())
            {
                in_library = !lib_data.is_empty();
            }

            personal_rating = first
                .get("attributes")
                .and_then(|a| a.get("personalRating"))
                .and_then(|v| v.as_i64());
        }

        // If personalRating was not available from catalog, consult the ratings endpoint
        if personal_rating.is_none() {
            let rating_path = format!("/v1/me/ratings/{kind}/{}", reference.id());
            if let Ok(val) = self.send_request(&rating_path, &[]).await
                && let Some(data) = val.get("data").and_then(|d| d.as_array())
                && let Some(first) = data.first()
            {
                personal_rating = first
                    .get("attributes")
                    .and_then(|a| a.get("value"))
                    .and_then(|v| v.as_i64());
            }
        }

        let favorite = personal_rating == Some(1);
        let rating = if personal_rating == Some(-1) {
            Rating::SuggestLess
        } else {
            Rating::Neutral
        };

        Ok(AccountMediaState::new(
            original_reference.clone(),
            in_library || original_reference != reference,
            favorite,
            rating,
        ))
    }

    /// Search the official Apple Music catalog.
    pub async fn search(
        &self,
        query: &str,
        kinds: &[SearchKindWire],
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<SearchResultsWire, AppleApiError> {
        let res = if let Some(c) = cursor
            && (c.starts_with("/v1/") || c.starts_with("http://") || c.starts_with("https://"))
        {
            self.send_request(c, &[]).await?
        } else {
            let mut types = Vec::new();
            if kinds.is_empty() || kinds.contains(&SearchKindWire::Track) {
                types.push("songs");
            }
            if kinds.is_empty() || kinds.contains(&SearchKindWire::Album) {
                types.push("albums");
            }
            if kinds.is_empty() || kinds.contains(&SearchKindWire::Artist) {
                types.push("artists");
            }
            if kinds.is_empty() || kinds.contains(&SearchKindWire::Playlist) {
                types.push("playlists");
            }
            if kinds.is_empty() || kinds.contains(&SearchKindWire::Station) {
                types.push("stations");
            }

            let types_str = types.join(",");
            let limit_str = limit.to_string();
            let mut query_params = vec![
                ("term", query),
                ("types", &types_str),
                ("limit", &limit_str),
                ("with", "topResults"),
            ];

            if let Some(offset) = cursor {
                query_params.push(("offset", offset));
            }

            self.send_request("/v1/catalog/{storefront}/search", &query_params)
                .await?
        };

        let results = res.get("results").unwrap_or(&res);

        let top_results = results.get("topResults").and_then(|sec| {
            sec.get("data").and_then(|d| d.as_array()).map(|arr| {
                arr.iter()
                    .filter_map(crate::pages::mapper::map_apple_resource_to_item)
                    .collect::<Vec<_>>()
            })
        });

        let tracks = results.get("songs").map(|sec| {
            let items: Vec<Track> = sec
                .get("data")
                .and_then(|d| d.as_array())
                .map(|arr| arr.iter().filter_map(parse_apple_track).collect())
                .unwrap_or_default();
            let next = sec.get("next").and_then(|v| v.as_str()).map(str::to_string);
            PagedListWire::new(items, next)
        });

        let albums = results.get("albums").map(|sec| {
            let items: Vec<Album> = sec
                .get("data")
                .and_then(|d| d.as_array())
                .map(|arr| arr.iter().filter_map(parse_apple_album).collect())
                .unwrap_or_default();
            let next = sec.get("next").and_then(|v| v.as_str()).map(str::to_string);
            PagedListWire::new(items, next)
        });

        let artists = results.get("artists").map(|sec| {
            let items: Vec<Artist> = sec
                .get("data")
                .and_then(|d| d.as_array())
                .map(|arr| arr.iter().filter_map(parse_apple_artist).collect())
                .unwrap_or_default();
            let next = sec.get("next").and_then(|v| v.as_str()).map(str::to_string);
            PagedListWire::new(items, next)
        });

        let playlists = results.get("playlists").map(|sec| {
            let items: Vec<Playlist> = sec
                .get("data")
                .and_then(|d| d.as_array())
                .map(|arr| arr.iter().filter_map(parse_apple_playlist).collect())
                .unwrap_or_default();
            let next = sec.get("next").and_then(|v| v.as_str()).map(str::to_string);
            PagedListWire::new(items, next)
        });

        let stations = results.get("stations").map(|sec| {
            let items: Vec<_> = sec
                .get("data")
                .and_then(|d| d.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(crate::pages::mapper::map_apple_resource_to_item)
                        .collect()
                })
                .unwrap_or_default();
            let next = sec.get("next").and_then(|v| v.as_str()).map(str::to_string);
            PagedListWire::new(items, next)
        });

        Ok(SearchResultsWire {
            top_results,
            tracks,
            albums,
            artists,
            playlists,
            stations,
        })
    }

    /// Search the user's personal Apple Music library (`/v1/me/library/search`).
    pub async fn search_library(
        &self,
        query: &str,
        kinds: &[SearchKindWire],
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<SearchResultsWire, AppleApiError> {
        let res = if let Some(c) = cursor
            && (c.starts_with("/v1/") || c.starts_with("http://") || c.starts_with("https://"))
        {
            self.send_request(c, &[]).await?
        } else {
            let mut types = Vec::new();
            if kinds.is_empty() || kinds.contains(&SearchKindWire::Track) {
                types.push("library-songs");
            }
            if kinds.is_empty() || kinds.contains(&SearchKindWire::Album) {
                types.push("library-albums");
            }
            if kinds.is_empty() || kinds.contains(&SearchKindWire::Artist) {
                types.push("library-artists");
            }
            if kinds.is_empty() || kinds.contains(&SearchKindWire::Playlist) {
                types.push("library-playlists");
            }

            let types_str = types.join(",");
            let limit_str = limit.to_string();
            let mut query_params = vec![
                ("term", query),
                ("types", &types_str),
                ("limit", &limit_str),
            ];

            if let Some(offset) = cursor {
                query_params.push(("offset", offset));
            }

            self.send_request("/v1/me/library/search", &query_params)
                .await?
        };

        let results = res.get("results").unwrap_or(&res);

        let tracks = results.get("library-songs").map(|sec| {
            let items: Vec<Track> = sec
                .get("data")
                .and_then(|d| d.as_array())
                .map(|arr| arr.iter().filter_map(parse_apple_track).collect())
                .unwrap_or_default();
            let next = sec.get("next").and_then(|v| v.as_str()).map(str::to_string);
            PagedListWire::new(items, next)
        });

        let albums = results.get("library-albums").map(|sec| {
            let items: Vec<Album> = sec
                .get("data")
                .and_then(|d| d.as_array())
                .map(|arr| arr.iter().filter_map(parse_apple_album).collect())
                .unwrap_or_default();
            let next = sec.get("next").and_then(|v| v.as_str()).map(str::to_string);
            PagedListWire::new(items, next)
        });

        let artists = results.get("library-artists").map(|sec| {
            let items: Vec<Artist> = sec
                .get("data")
                .and_then(|d| d.as_array())
                .map(|arr| arr.iter().filter_map(parse_apple_artist).collect())
                .unwrap_or_default();
            let next = sec.get("next").and_then(|v| v.as_str()).map(str::to_string);
            PagedListWire::new(items, next)
        });

        let playlists = results.get("library-playlists").map(|sec| {
            let items: Vec<Playlist> = sec
                .get("data")
                .and_then(|d| d.as_array())
                .map(|arr| arr.iter().filter_map(parse_apple_playlist).collect())
                .unwrap_or_default();
            let next = sec.get("next").and_then(|v| v.as_str()).map(str::to_string);
            PagedListWire::new(items, next)
        });

        Ok(SearchResultsWire {
            top_results: None,
            tracks,
            albums,
            artists,
            playlists,
            stations: None,
        })
    }

    /// Fetch official curator details and editorial grouping for an Apple Curator.
    pub async fn get_curator_grouping(&self, curator_id: &str) -> Result<Value, AppleApiError> {
        let params = [
            ("ids[apple-curators]", curator_id),
            ("ids[curators]", curator_id),
            ("include", "grouping,playlists"),
            ("format[resources]", "map"),
            ("art[url]", "f"),
            ("extend", "editorialArtwork,seoDescription,seoTitle"),
            ("extend[apple-curators]", "playlistCount"),
            ("extend[curators]", "playlistCount"),
            (
                "fields[albums]",
                "artistName,artistUrl,artwork,contentRating,editorialArtwork,name,playParams,releaseDate,url",
            ),
            ("platform", "web"),
        ];
        self.send_request(
            "https://amp-api.music.apple.com/v1/catalog/{storefront}",
            &params,
        )
        .await
    }

    /// Fetch a single catalog or library item by MediaRef.
    pub async fn get_catalog_item(
        &self,
        reference: &MediaRef,
    ) -> Result<CatalogItemWire, AppleApiError> {
        let raw_id = reference.id();
        let kind = reference.kind();

        let is_library = raw_id.starts_with("i.")
            || raw_id.starts_with("l.")
            || (raw_id.starts_with("p.") && !raw_id.starts_with("pl."));

        let (path, query_params) = if is_library {
            match kind {
                "song" | "track" => (
                    format!("/v1/me/library/songs/{raw_id}"),
                    vec![("include", "catalog,artists,albums")],
                ),
                "album" => (
                    format!("/v1/me/library/albums/{raw_id}"),
                    vec![("include", "artists,tracks")],
                ),
                "playlist" => (
                    format!("/v1/me/library/playlists/{raw_id}"),
                    vec![("include", "tracks")],
                ),
                other => {
                    return Err(AppleApiError::NotFound(format!(
                        "Unsupported library kind '{other}'"
                    )));
                }
            }
        } else {
            match kind {
                "song" | "track" => (
                    format!("/v1/catalog/{{storefront}}/songs/{raw_id}"),
                    vec![("include", "artists,albums")],
                ),
                "album" => (
                    format!("/v1/catalog/{{storefront}}/albums/{raw_id}"),
                    vec![("include", "artists,tracks")],
                ),
                "artist" => (
                    format!("/v1/catalog/{{storefront}}/artists/{raw_id}"),
                    vec![],
                ),
                "playlist" => (
                    format!("/v1/catalog/{{storefront}}/playlists/{raw_id}"),
                    vec![("include", "tracks")],
                ),
                other => {
                    return Err(AppleApiError::NotFound(format!(
                        "Unsupported catalog kind '{other}'"
                    )));
                }
            }
        };

        let res = self.send_request(&path, &query_params).await?;
        let item = res
            .get("data")
            .and_then(|d| d.as_array())
            .and_then(|arr| arr.first())
            .ok_or_else(|| AppleApiError::NotFound(format!("Item '{reference}' not found")))?;

        match kind {
            "song" | "track" => {
                let mut track = parse_apple_track(item).ok_or_else(|| {
                    AppleApiError::Parse(format!("Failed to parse track '{reference}'"))
                })?;
                track.id = reference.clone();
                Ok(CatalogItemWire::Track(track))
            }
            "album" => {
                let mut album = parse_apple_album(item).ok_or_else(|| {
                    AppleApiError::Parse(format!("Failed to parse album '{reference}'"))
                })?;
                album.id = reference.clone();
                Ok(CatalogItemWire::Album(album))
            }
            "artist" => {
                let mut artist = parse_apple_artist(item).ok_or_else(|| {
                    AppleApiError::Parse(format!("Failed to parse artist '{reference}'"))
                })?;
                artist.id = reference.clone();
                Ok(CatalogItemWire::Artist(artist))
            }
            "playlist" => {
                let mut playlist = parse_apple_playlist(item).ok_or_else(|| {
                    AppleApiError::Parse(format!("Failed to parse playlist '{reference}'"))
                })?;
                playlist.id = reference.clone();
                Ok(CatalogItemWire::Playlist(playlist))
            }
            other => Err(AppleApiError::NotFound(format!(
                "Unknown media kind '{other}'"
            ))),
        }
    }

    /// Fetch collection tracks (album or playlist) with pagination.
    pub async fn get_collection_items(
        &self,
        reference: &MediaRef,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<PagedListWire<Track>, AppleApiError> {
        let raw_id = reference.id();
        let kind = reference.kind();

        let res = if let Some(c) = cursor
            && (c.starts_with("/v1/") || c.starts_with("http://") || c.starts_with("https://"))
        {
            self.send_request(c, &[]).await?
        } else {
            let is_library = raw_id.starts_with("l.")
                || (raw_id.starts_with("p.") && !raw_id.starts_with("pl."));
            let path = match (kind, is_library) {
                ("album", false) => format!("/v1/catalog/{{storefront}}/albums/{raw_id}/tracks"),
                ("album", true) => format!("/v1/me/library/albums/{raw_id}/tracks"),
                ("playlist", false) => {
                    format!("/v1/catalog/{{storefront}}/playlists/{raw_id}/tracks")
                }
                ("playlist", true) => format!("/v1/me/library/playlists/{raw_id}/tracks"),
                _ => {
                    return Err(AppleApiError::NotFound(format!(
                        "Kind '{kind}' cannot have collection items"
                    )));
                }
            };

            let limit_str = limit.to_string();
            let mut query_params = vec![("limit", limit_str.as_str())];
            if let Some(offset) = cursor {
                query_params.push(("offset", offset));
            }

            self.send_request(&path, &query_params).await?
        };

        let items: Vec<Track> = res
            .get("data")
            .and_then(|d| d.as_array())
            .map(|arr| arr.iter().filter_map(parse_apple_track).collect())
            .unwrap_or_default();
        let next = res.get("next").and_then(|v| v.as_str()).map(str::to_string);
        Ok(PagedListWire::new(items, next))
    }

    /// Fetch a page of user's personal library items.
    pub async fn get_library(
        &self,
        kind: LibraryKindWire,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<LibraryPageWire, AppleApiError> {
        let res = if let Some(c) = cursor
            && (c.starts_with("/v1/") || c.starts_with("http://") || c.starts_with("https://"))
        {
            self.send_request(c, &[]).await?
        } else {
            let path = match kind {
                LibraryKindWire::Tracks => "/v1/me/library/songs",
                LibraryKindWire::Albums => "/v1/me/library/albums",
                LibraryKindWire::Playlists => "/v1/me/library/playlists",
            };

            let limit_str = limit.to_string();
            let mut query_params = vec![("limit", limit_str.as_str())];
            if let Some(offset) = cursor {
                query_params.push(("offset", offset));
            }

            self.send_request(path, &query_params).await?
        };

        let next = res.get("next").and_then(|v| v.as_str()).map(str::to_string);
        let data = res.get("data").and_then(|d| d.as_array());

        match kind {
            LibraryKindWire::Tracks => {
                let items: Vec<Track> = data
                    .map(|arr| arr.iter().filter_map(parse_apple_track).collect())
                    .unwrap_or_default();
                Ok(LibraryPageWire::Tracks(PagedListWire::new(items, next)))
            }
            LibraryKindWire::Albums => {
                let items: Vec<Album> = data
                    .map(|arr| arr.iter().filter_map(parse_apple_album).collect())
                    .unwrap_or_default();
                Ok(LibraryPageWire::Albums(PagedListWire::new(items, next)))
            }
            LibraryKindWire::Playlists => {
                let items: Vec<Playlist> = data
                    .map(|arr| arr.iter().filter_map(parse_apple_playlist).collect())
                    .unwrap_or_default();
                Ok(LibraryPageWire::Playlists(PagedListWire::new(items, next)))
            }
        }
    }

    /// Fetch the user's recently played tracks from `/v1/me/recent/played/tracks`.
    ///
    /// If no tracks are returned (e.g. tracks history was cleared or not populated),
    /// falls back to inspecting `/v1/me/recent/played` for recently played containers (albums/playlists).
    pub async fn get_recently_played_tracks(
        &self,
        limit: usize,
    ) -> Result<Vec<Track>, AppleApiError> {
        let limit_str = limit.max(1).to_string();
        let res = self
            .send_request("/v1/me/recent/played/tracks", &[("limit", &limit_str)])
            .await;

        if let Ok(val) = res
            && let Some(arr) = val.get("data").and_then(|d| d.as_array())
        {
            let tracks: Vec<Track> = arr.iter().filter_map(parse_apple_track).collect();
            if !tracks.is_empty() {
                return Ok(tracks);
            }
        }

        // Fallback to recently played containers (albums/playlists)
        let container_res = self
            .send_request("/v1/me/recent/played", &[("limit", "1")])
            .await?;

        if let Some(data) = container_res.get("data").and_then(|d| d.as_array())
            && let Some(first) = data.first()
        {
            let typ = first.get("type").and_then(|t| t.as_str()).unwrap_or("");
            let id = first.get("id").and_then(|i| i.as_str()).unwrap_or("");
            if typ == "albums" || typ == "library-albums" {
                let mref = MediaRef::Album(id.to_string());
                if let Ok(PagedListWire { items, .. }) =
                    self.get_collection_items(&mref, 1, None).await
                    && !items.is_empty()
                {
                    return Ok(items);
                }
            } else if typ == "playlists" || typ == "library-playlists" {
                let mref = MediaRef::Playlist(id.to_string());
                if let Ok(PagedListWire { items, .. }) =
                    self.get_collection_items(&mref, 1, None).await
                    && !items.is_empty()
                {
                    return Ok(items);
                }
            }
        }

        Ok(Vec::new())
    }
}
