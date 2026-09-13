//! Native Apple Music HTTP API client (`OfficialAppleMusicApi`).
//!
//! Exposes official metadata, catalog, collection, and personal library operations
//! over direct HTTP against `https://api.music.apple.com/v1`.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use malus_protocol::{
    MediaIdWire,
    wire::{
        AlbumWire, ArtistWire, CatalogItemWire, LibraryKindWire, LibraryPageWire, PageWire,
        PlaylistWire, SearchKindWire, SearchResultsWire, TrackWire,
    },
};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT};
use serde_json::Value;
use tracing::{info, warn};

use crate::api::{
    credentials::{AppleCredentials, TokenProvider},
    error::AppleApiError,
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
        }
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

        headers.insert(
            "music-user-token",
            HeaderValue::from_str(&creds.music_user_token).map_err(|e| {
                AppleApiError::AuthRequired(format!("Invalid characters in music user token: {e}"))
            })?,
        );

        headers.insert(
            "origin",
            HeaderValue::from_static("https://music.apple.com"),
        );
        headers.insert(
            "referer",
            HeaderValue::from_static("https://music.apple.com/"),
        );
        headers.insert(USER_AGENT, HeaderValue::from_static(DEFAULT_USER_AGENT));

        Ok(headers)
    }

    /// Execute a single HTTP GET request against Apple Music API.
    async fn execute_single_request(
        &self,
        url: &str,
        query: &[(&str, &str)],
        creds: &AppleCredentials,
    ) -> Result<reqwest::Response, AppleApiError> {
        let headers = self.build_headers(creds)?;
        let mut req = self.client.get(url).headers(headers);
        if !query.is_empty() {
            req = req.query(query);
        }

        // Retry transport network errors up to 2 times with short backoff
        let mut attempts = 0;
        loop {
            attempts += 1;
            match req
                .try_clone()
                .unwrap_or_else(|| self.client.get(url))
                .send()
                .await
            {
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

    /// Canonical request pipeline with 401 token refresh and retry.
    pub async fn send_request(
        &self,
        path_or_url: &str,
        query: &[(&str, &str)],
    ) -> Result<Value, AppleApiError> {
        let mut creds = self.token_provider.get_credentials()?;
        if creds.developer_token.is_empty() || creds.music_user_token.is_empty() {
            return Err(AppleApiError::AuthRequired(
                "Missing Apple Music credentials".to_string(),
            ));
        }

        let url = self.build_url(path_or_url, &creds.storefront);
        let start = Instant::now();

        let mut resp = self.execute_single_request(&url, query, &creds).await?;

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
                        .execute_single_request(&retry_url, query, &creds)
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

        info!(
            method = "GET",
            path = %path_or_url,
            status = %status.as_u16(),
            elapsed_ms = %elapsed.as_millis(),
            "Apple Music API request completed"
        );

        if status.is_success() {
            let body = resp.text().await.map_err(|e| {
                AppleApiError::Network(format!("Failed to read response body: {e}"))
            })?;
            serde_json::from_str::<Value>(&body)
                .map_err(|e| AppleApiError::Parse(format!("Failed to parse response JSON: {e}")))
        } else if status == reqwest::StatusCode::UNAUTHORIZED {
            Err(AppleApiError::AuthRequired(
                "Apple Music session expired or unauthorized".to_string(),
            ))
        } else if status == reqwest::StatusCode::FORBIDDEN {
            let body = resp.text().await.unwrap_or_default();
            Err(AppleApiError::Forbidden(body))
        } else if status == reqwest::StatusCode::NOT_FOUND {
            let body = resp.text().await.unwrap_or_default();
            Err(AppleApiError::NotFound(body))
        } else if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .map(Duration::from_secs);
            Err(AppleApiError::RateLimited { retry_after })
        } else if status.is_server_error() {
            let body = resp.text().await.unwrap_or_default();
            Err(AppleApiError::Server {
                status: status.as_u16(),
                message: body,
            })
        } else {
            let body = resp.text().await.unwrap_or_default();
            Err(AppleApiError::Other(format!(
                "HTTP {}: {}",
                status.as_u16(),
                body
            )))
        }
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

            self.send_request("/v1/catalog/{storefront}/search", &query_params)
                .await?
        };

        let results = res.get("results").unwrap_or(&res);

        let tracks = results.get("songs").map(|sec| {
            let items: Vec<TrackWire> = sec
                .get("data")
                .and_then(|d| d.as_array())
                .map(|arr| arr.iter().filter_map(parse_apple_track).collect())
                .unwrap_or_default();
            let next = sec.get("next").and_then(|v| v.as_str()).map(str::to_string);
            PageWire::new(items, next)
        });

        let albums = results.get("albums").map(|sec| {
            let items: Vec<AlbumWire> = sec
                .get("data")
                .and_then(|d| d.as_array())
                .map(|arr| arr.iter().filter_map(parse_apple_album).collect())
                .unwrap_or_default();
            let next = sec.get("next").and_then(|v| v.as_str()).map(str::to_string);
            PageWire::new(items, next)
        });

        let artists = results.get("artists").map(|sec| {
            let items: Vec<ArtistWire> = sec
                .get("data")
                .and_then(|d| d.as_array())
                .map(|arr| arr.iter().filter_map(parse_apple_artist).collect())
                .unwrap_or_default();
            let next = sec.get("next").and_then(|v| v.as_str()).map(str::to_string);
            PageWire::new(items, next)
        });

        let playlists = results.get("playlists").map(|sec| {
            let items: Vec<PlaylistWire> = sec
                .get("data")
                .and_then(|d| d.as_array())
                .map(|arr| arr.iter().filter_map(parse_apple_playlist).collect())
                .unwrap_or_default();
            let next = sec.get("next").and_then(|v| v.as_str()).map(str::to_string);
            PageWire::new(items, next)
        });

        Ok(SearchResultsWire {
            tracks,
            albums,
            artists,
            playlists,
        })
    }

    /// Fetch a single catalog or library item by MediaId.
    pub async fn get_catalog_item(&self, media_id: &str) -> Result<CatalogItemWire, AppleApiError> {
        let mid = MediaIdWire::parse(media_id)
            .map_err(|e| AppleApiError::Other(format!("Invalid MediaId '{media_id}': {e}")))?;

        let raw_id = mid.id();
        let kind = mid.kind();

        let is_library = raw_id.starts_with("i.")
            || raw_id.starts_with("l.")
            || (raw_id.starts_with("p.") && !raw_id.starts_with("pl."));

        let path = if is_library {
            match kind {
                "track" => format!("/v1/me/library/songs/{raw_id}"),
                "album" => format!("/v1/me/library/albums/{raw_id}"),
                "playlist" => format!("/v1/me/library/playlists/{raw_id}"),
                other => {
                    return Err(AppleApiError::NotFound(format!(
                        "Unsupported library kind '{other}'"
                    )));
                }
            }
        } else {
            match kind {
                "track" => format!("/v1/catalog/{{storefront}}/songs/{raw_id}"),
                "album" => format!("/v1/catalog/{{storefront}}/albums/{raw_id}"),
                "artist" => format!("/v1/catalog/{{storefront}}/artists/{raw_id}"),
                "playlist" => format!("/v1/catalog/{{storefront}}/playlists/{raw_id}"),
                other => {
                    return Err(AppleApiError::NotFound(format!(
                        "Unsupported catalog kind '{other}'"
                    )));
                }
            }
        };

        let res = self.send_request(&path, &[]).await?;
        let item = res
            .get("data")
            .and_then(|d| d.as_array())
            .and_then(|arr| arr.first())
            .ok_or_else(|| AppleApiError::NotFound(format!("Item '{media_id}' not found")))?;

        match kind {
            "track" => {
                let mut track = parse_apple_track(item).ok_or_else(|| {
                    AppleApiError::Parse(format!("Failed to parse track '{media_id}'"))
                })?;
                track.id = media_id.to_string();
                Ok(CatalogItemWire::Track(track))
            }
            "album" => {
                let mut album = parse_apple_album(item).ok_or_else(|| {
                    AppleApiError::Parse(format!("Failed to parse album '{media_id}'"))
                })?;
                album.id = media_id.to_string();
                Ok(CatalogItemWire::Album(album))
            }
            "artist" => {
                let mut artist = parse_apple_artist(item).ok_or_else(|| {
                    AppleApiError::Parse(format!("Failed to parse artist '{media_id}'"))
                })?;
                artist.id = media_id.to_string();
                Ok(CatalogItemWire::Artist(artist))
            }
            "playlist" => {
                let mut playlist = parse_apple_playlist(item).ok_or_else(|| {
                    AppleApiError::Parse(format!("Failed to parse playlist '{media_id}'"))
                })?;
                playlist.id = media_id.to_string();
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
        media_id: &str,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<PageWire<TrackWire>, AppleApiError> {
        let mid = MediaIdWire::parse(media_id)
            .map_err(|e| AppleApiError::Other(format!("Invalid MediaId '{media_id}': {e}")))?;

        let raw_id = mid.id();
        let kind = mid.kind();

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

        let items: Vec<TrackWire> = res
            .get("data")
            .and_then(|d| d.as_array())
            .map(|arr| arr.iter().filter_map(parse_apple_track).collect())
            .unwrap_or_default();
        let next = res.get("next").and_then(|v| v.as_str()).map(str::to_string);
        Ok(PageWire::new(items, next))
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
                let items: Vec<TrackWire> = data
                    .map(|arr| arr.iter().filter_map(parse_apple_track).collect())
                    .unwrap_or_default();
                Ok(LibraryPageWire::Tracks(PageWire::new(items, next)))
            }
            LibraryKindWire::Albums => {
                let items: Vec<AlbumWire> = data
                    .map(|arr| arr.iter().filter_map(parse_apple_album).collect())
                    .unwrap_or_default();
                Ok(LibraryPageWire::Albums(PageWire::new(items, next)))
            }
            LibraryKindWire::Playlists => {
                let items: Vec<PlaylistWire> = data
                    .map(|arr| arr.iter().filter_map(parse_apple_playlist).collect())
                    .unwrap_or_default();
                Ok(LibraryPageWire::Playlists(PageWire::new(items, next)))
            }
        }
    }
}
