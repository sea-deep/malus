//! Apple Music page and surface implementation.
//!
//! Generates structured `ApplePageWire` documents for:
//! - Home (recommendations, recently played, heavy rotation)
//! - New (charts for songs, albums, playlists)
//! - Radio (recent stations, live stations)
//! - Library (recently added, songs, albums, artists, playlists)
//! - Album detail (header, metadata, track list)
//! - Artist detail (header, top songs, albums, singles, similar artists)
//! - Playlist detail (header, curator, track list)
//! - Replay (year in review, top songs, albums, artists)

pub mod manifest;
pub mod mapper;
pub mod routes;

use malus_protocol::wire::{
    ApplePageWire, PageBadgeWire, PageContinuationWire, PageCursorWire, PageHeaderWire,
    PageSectionWire,
};
use serde_json::Value;

use crate::{api::OfficialAppleMusicApi, error::AppleError};
use routes::ApplePageRoute;

/// Resolve an Apple page route or route string to an ApplePageWire response.
pub async fn get_apple_page(
    api: &OfficialAppleMusicApi,
    route_or_id: &str,
) -> Result<ApplePageWire, AppleError> {
    let route = ApplePageRoute::parse(route_or_id)
        .ok_or_else(|| AppleError::NotFound(format!("Unknown page route: {route_or_id}")))?;

    match route {
        ApplePageRoute::Home => build_home(api).await,
        ApplePageRoute::New => build_new(api).await,
        ApplePageRoute::Radio => build_radio(api).await,
        ApplePageRoute::LibraryRecentlyAdded => build_library_recently_added(api).await,
        ApplePageRoute::LibrarySongs => build_library_songs(api).await,
        ApplePageRoute::LibraryAlbums => build_library_albums(api).await,
        ApplePageRoute::LibraryArtists => build_library_artists(api).await,
        ApplePageRoute::LibraryPlaylists => build_library_playlists(api).await,
        ApplePageRoute::Album(id) => build_album_detail(api, &id).await,
        ApplePageRoute::Artist(id) => build_artist_detail(api, &id).await,
        ApplePageRoute::Playlist(id) => build_playlist_detail(api, &id).await,
        ApplePageRoute::Replay(year) => build_replay(api, &year).await,
    }
}

/// Continue pagination for an Apple page.
pub async fn continue_apple_page(
    api: &OfficialAppleMusicApi,
    route_or_id: &str,
    cursor: &PageCursorWire,
) -> Result<PageContinuationWire, AppleError> {
    let _route = ApplePageRoute::parse(route_or_id)
        .ok_or_else(|| AppleError::NotFound(format!("Unknown page route: {route_or_id}")))?;

    let next_url = decode_cursor(&cursor.token)?;
    let resp = api.send_request(&next_url, &[]).await?;

    if let Some(section_id) = &cursor.section_id {
        let items = mapper::map_apple_data_to_items(&resp);
        let next = extract_next_cursor(&resp, Some(section_id));
        Ok(PageContinuationWire::Section {
            section_id: section_id.clone(),
            items,
            continuation: next,
        })
    } else {
        let sections = mapper::map_recommendations_to_sections(&resp);
        let next = extract_next_cursor(&resp, None);
        Ok(PageContinuationWire::Sections {
            sections,
            continuation: next,
        })
    }
}

// Backwards-compatible aliases
pub use continue_apple_page as continue_apple_surface;
pub use get_apple_page as get_apple_surface;

// ──────────────────────── HOME ────────────────────────

async fn build_home(api: &OfficialAppleMusicApi) -> Result<ApplePageWire, AppleError> {
    let mut page = ApplePageWire::new("apple:page:home", "Listen Now");
    page.subtitle = Some("Personal recommendations and radio".to_string());

    // Recommendations
    match api
        .send_request("/v1/me/recommendations", &[("limit", "10")])
        .await
    {
        Ok(rec_data) => {
            let mut sections = mapper::map_recommendations_to_sections(&rec_data);
            page.sections.append(&mut sections);
            if let Some(next) = rec_data.get("next").and_then(|n| n.as_str()) {
                page.continuation = Some(PageCursorWire::new(encode_cursor(next)));
            }
        }
        Err(e) => {
            tracing::warn!("Failed to fetch recommendations for home page: {e}");
        }
    }

    // Recently Played
    match api
        .send_request("/v1/me/recent/played", &[("limit", "10")])
        .await
    {
        Ok(rp_data) => {
            let items = mapper::map_apple_data_to_items(&rp_data);
            if !items.is_empty() {
                let mut section = PageSectionWire::new(
                    "recently-played",
                    Some("Recently Played".to_string()),
                    items,
                )
                .with_hint("shelf");
                if let Some(next) = rp_data.get("next").and_then(|n| n.as_str()) {
                    section.continuation = Some(PageCursorWire::section(
                        "recently-played",
                        encode_cursor(next),
                    ));
                }
                page.sections.push(section);
            }
        }
        Err(e) => tracing::warn!("Failed to fetch recently played: {e}"),
    }

    // Heavy Rotation
    match api
        .send_request("/v1/me/history/heavy-rotation", &[("limit", "10")])
        .await
    {
        Ok(hr_data) => {
            let items = mapper::map_apple_data_to_items(&hr_data);
            if !items.is_empty() {
                let section = PageSectionWire::new(
                    "heavy-rotation",
                    Some("Heavy Rotation".to_string()),
                    items,
                )
                .with_hint("shelf");
                page.sections.push(section);
            }
        }
        Err(e) => tracing::warn!("Failed to fetch heavy rotation: {e}"),
    }

    Ok(page)
}

// ──────────────────────── NEW (CHARTS) ────────────────────────

async fn build_new(api: &OfficialAppleMusicApi) -> Result<ApplePageWire, AppleError> {
    let mut page = ApplePageWire::new("apple:page:new", "Browse");
    page.subtitle = Some("Charts and new releases".to_string());

    let charts_data = api
        .send_request(
            "/v1/catalog/{storefront}/charts",
            &[("types", "songs,albums,playlists"), ("limit", "20")],
        )
        .await?;

    if let Some(results) = charts_data.get("results").and_then(|r| r.as_object()) {
        for chart_type in &["songs", "albums", "playlists"] {
            if let Some(chart_arr) = results.get(*chart_type).and_then(|c| c.as_array()) {
                for chart in chart_arr {
                    let chart_name = chart
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("Chart");
                    let section_id = format!("chart:{chart_type}");
                    let items = if let Some(data) = chart.get("data").and_then(|d| d.as_array()) {
                        data.iter()
                            .filter_map(mapper::map_apple_resource_to_item)
                            .collect()
                    } else {
                        Vec::new()
                    };
                    if !items.is_empty() {
                        let hint = if *chart_type == "songs" {
                            "track-list"
                        } else {
                            "grid"
                        };
                        let mut section =
                            PageSectionWire::new(&section_id, Some(chart_name.to_string()), items)
                                .with_hint(hint);
                        if let Some(next) = chart.get("next").and_then(|n| n.as_str()) {
                            section.continuation =
                                Some(PageCursorWire::section(&section_id, encode_cursor(next)));
                        }
                        page.sections.push(section);
                    }
                }
            }
        }
    }

    Ok(page)
}

// ──────────────────────── RADIO ────────────────────────

async fn build_radio(api: &OfficialAppleMusicApi) -> Result<ApplePageWire, AppleError> {
    let mut page = ApplePageWire::new("apple:page:radio", "Radio");

    // Recent Radio Stations
    match api
        .send_request("/v1/me/recent/radio-stations", &[("limit", "20")])
        .await
    {
        Ok(data) => {
            let items = mapper::map_apple_data_to_items(&data);
            if !items.is_empty() {
                let mut section = PageSectionWire::new(
                    "recent-stations",
                    Some("Recent Stations".to_string()),
                    items,
                )
                .with_hint("shelf");
                if let Some(next) = data.get("next").and_then(|n| n.as_str()) {
                    section.continuation = Some(PageCursorWire::section(
                        "recent-stations",
                        encode_cursor(next),
                    ));
                }
                page.sections.push(section);
            }
        }
        Err(e) => tracing::warn!("Failed to fetch recent radio stations: {e}"),
    }

    // Live Stations (Apple Music 1, Hits, Country, etc.)
    match api
        .send_request(
            "/v1/catalog/{storefront}/search",
            &[
                ("term", "Apple Music"),
                ("types", "stations"),
                ("limit", "10"),
            ],
        )
        .await
    {
        Ok(data) => {
            if let Some(stations) = data
                .get("results")
                .and_then(|r| r.get("stations"))
                .and_then(|s| s.get("data"))
                .and_then(|d| d.as_array())
            {
                let live_items: Vec<_> = stations
                    .iter()
                    .filter(|s| {
                        s.get("attributes")
                            .and_then(|a| a.get("isLive"))
                            .and_then(|l| l.as_bool())
                            .unwrap_or(false)
                    })
                    .filter_map(mapper::map_apple_resource_to_item)
                    .collect();
                if !live_items.is_empty() {
                    let section = PageSectionWire::new(
                        "live-stations",
                        Some("Live Stations".to_string()),
                        live_items,
                    )
                    .with_hint("shelf");
                    page.sections.push(section);
                }
            }
        }
        Err(e) => tracing::warn!("Failed to search for live stations: {e}"),
    }

    Ok(page)
}

// ──────────────────────── LIBRARY ────────────────────────

async fn build_library_recently_added(
    api: &OfficialAppleMusicApi,
) -> Result<ApplePageWire, AppleError> {
    build_library_list(
        api,
        "apple:page:library:recently-added",
        "Recently Added",
        "/v1/me/library/recently-added",
        "grid",
    )
    .await
}

async fn build_library_songs(api: &OfficialAppleMusicApi) -> Result<ApplePageWire, AppleError> {
    build_library_list(
        api,
        "apple:page:library:songs",
        "Songs",
        "/v1/me/library/songs",
        "track-list",
    )
    .await
}

async fn build_library_albums(api: &OfficialAppleMusicApi) -> Result<ApplePageWire, AppleError> {
    build_library_list(
        api,
        "apple:page:library:albums",
        "Albums",
        "/v1/me/library/albums",
        "grid",
    )
    .await
}

async fn build_library_artists(api: &OfficialAppleMusicApi) -> Result<ApplePageWire, AppleError> {
    build_library_list(
        api,
        "apple:page:library:artists",
        "Artists",
        "/v1/me/library/artists",
        "grid",
    )
    .await
}

async fn build_library_playlists(api: &OfficialAppleMusicApi) -> Result<ApplePageWire, AppleError> {
    build_library_list(
        api,
        "apple:page:library:playlists",
        "Playlists",
        "/v1/me/library/playlists",
        "grid",
    )
    .await
}

async fn build_library_list(
    api: &OfficialAppleMusicApi,
    page_id: &str,
    title: &str,
    endpoint: &str,
    hint: &str,
) -> Result<ApplePageWire, AppleError> {
    let data = api.send_request(endpoint, &[("limit", "25")]).await?;
    let items = mapper::map_apple_data_to_items(&data);
    let mut page = ApplePageWire::new(page_id, title);
    let mut section = PageSectionWire::new("all", None, items).with_hint(hint);
    if let Some(next) = data.get("next").and_then(|n| n.as_str()) {
        section.continuation = Some(PageCursorWire::section("all", encode_cursor(next)));
    }
    page.sections.push(section);
    Ok(page)
}

// ──────────────────────── ALBUM DETAIL ────────────────────────

async fn build_album_detail(
    api: &OfficialAppleMusicApi,
    album_id: &str,
) -> Result<ApplePageWire, AppleError> {
    let path = format!("/v1/catalog/{{storefront}}/albums/{album_id}");
    let data = api.send_request(&path, &[("include", "tracks")]).await?;

    let album = data
        .get("data")
        .and_then(|d| d.as_array())
        .and_then(|a| a.first())
        .ok_or_else(|| AppleError::NotFound(format!("Album {album_id} not found")))?;

    let attrs = album.get("attributes").unwrap_or(album);
    let name = attrs
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("Album");
    let artist = attrs
        .get("artistName")
        .and_then(|a| a.as_str())
        .unwrap_or("");
    let page_id = format!("apple:page:album:{album_id}");

    let mut page = ApplePageWire::new(&page_id, name);
    let mut header = PageHeaderWire::new(name);
    header.subtitle = if artist.is_empty() {
        None
    } else {
        Some(artist.to_string())
    };
    header.artwork = attrs
        .get("artwork")
        .and_then(crate::api::parse::parse_apple_artwork);
    if let Some(release) = attrs.get("releaseDate").and_then(|r| r.as_str()) {
        header.metadata.push(release.to_string());
    }
    if let Some(genre_arr) = attrs.get("genreNames").and_then(|g| g.as_array()) {
        for g in genre_arr.iter().take(2) {
            if let Some(s) = g.as_str() {
                header.metadata.push(s.to_string());
            }
        }
    }
    if attrs.get("contentRating").and_then(|r| r.as_str()) == Some("explicit") {
        header
            .badges
            .push(PageBadgeWire::with_style("E", "explicit"));
    }
    page.header = Some(header);

    // Track list
    if let Some(tracks) = album
        .get("relationships")
        .and_then(|r| r.get("tracks"))
        .and_then(|t| t.get("data"))
        .and_then(|d| d.as_array())
    {
        let items: Vec<_> = tracks
            .iter()
            .filter_map(mapper::map_apple_resource_to_item)
            .collect();
        let mut section = PageSectionWire::new("tracks", None, items).with_hint("track-list");
        if let Some(next) = album
            .get("relationships")
            .and_then(|r| r.get("tracks"))
            .and_then(|t| t.get("next"))
            .and_then(|n| n.as_str())
        {
            section.continuation = Some(PageCursorWire::section("tracks", encode_cursor(next)));
        }
        page.sections.push(section);
    }

    Ok(page)
}

// ──────────────────────── ARTIST DETAIL ────────────────────────

async fn build_artist_detail(
    api: &OfficialAppleMusicApi,
    artist_id: &str,
) -> Result<ApplePageWire, AppleError> {
    let path = format!("/v1/catalog/{{storefront}}/artists/{artist_id}");
    let data = api
        .send_request(
            &path,
            &[(
                "views",
                "top-songs,full-albums,singles,similar-artists,latest-release",
            )],
        )
        .await?;

    let artist = data
        .get("data")
        .and_then(|d| d.as_array())
        .and_then(|a| a.first())
        .ok_or_else(|| AppleError::NotFound(format!("Artist {artist_id} not found")))?;

    let attrs = artist.get("attributes").unwrap_or(artist);
    let name = attrs
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("Artist");
    let page_id = format!("apple:page:artist:{artist_id}");

    let mut page = ApplePageWire::new(&page_id, name);
    let mut header = PageHeaderWire::new(name);
    header.artwork = attrs
        .get("artwork")
        .and_then(crate::api::parse::parse_apple_artwork);
    if let Some(genres) = attrs.get("genreNames").and_then(|g| g.as_array()) {
        for g in genres.iter().take(3) {
            if let Some(s) = g.as_str() {
                header.metadata.push(s.to_string());
            }
        }
    }
    page.header = Some(header);

    // Views
    if let Some(views) = artist.get("views").and_then(|v| v.as_object()) {
        let view_order = [
            "top-songs",
            "latest-release",
            "full-albums",
            "singles",
            "similar-artists",
        ];
        for view_key in &view_order {
            if let Some(view) = views.get(*view_key) {
                let view_title = view
                    .get("attributes")
                    .and_then(|a| a.get("title").and_then(|t| t.as_str()))
                    .unwrap_or(view_key);

                if let Some(view_data) = view.get("data").and_then(|d| d.as_array()) {
                    if view_data.is_empty() {
                        continue;
                    }
                    let items: Vec<_> = view_data
                        .iter()
                        .filter_map(mapper::map_apple_resource_to_item)
                        .collect();
                    let hint = if *view_key == "top-songs" {
                        "track-list"
                    } else {
                        "shelf"
                    };
                    let section_id = format!("view:{view_key}");
                    let mut section =
                        PageSectionWire::new(&section_id, Some(view_title.to_string()), items)
                            .with_hint(hint);
                    if let Some(next) = view.get("next").and_then(|n| n.as_str()) {
                        section.continuation =
                            Some(PageCursorWire::section(&section_id, encode_cursor(next)));
                    }
                    page.sections.push(section);
                }
            }
        }
    }

    Ok(page)
}

// ──────────────────────── PLAYLIST DETAIL ────────────────────────

async fn build_playlist_detail(
    api: &OfficialAppleMusicApi,
    playlist_id: &str,
) -> Result<ApplePageWire, AppleError> {
    let path = format!("/v1/catalog/{{storefront}}/playlists/{playlist_id}");
    let data = api.send_request(&path, &[("include", "tracks")]).await?;

    let playlist = data
        .get("data")
        .and_then(|d| d.as_array())
        .and_then(|a| a.first())
        .ok_or_else(|| AppleError::NotFound(format!("Playlist {playlist_id} not found")))?;

    let attrs = playlist.get("attributes").unwrap_or(playlist);
    let name = attrs
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("Playlist");
    let page_id = format!("apple:page:playlist:{playlist_id}");

    let mut page = ApplePageWire::new(&page_id, name);
    let mut header = PageHeaderWire::new(name);
    header.artwork = attrs
        .get("artwork")
        .and_then(crate::api::parse::parse_apple_artwork);
    if let Some(curator) = attrs.get("curatorName").and_then(|c| c.as_str()) {
        header.subtitle = Some(curator.to_string());
    }
    if let Some(desc) = attrs
        .get("description")
        .and_then(|d| d.get("short").or_else(|| d.get("standard")))
        .and_then(|s| s.as_str())
    {
        header.metadata.push(desc.to_string());
    }
    page.header = Some(header);

    // Track list
    if let Some(tracks) = playlist
        .get("relationships")
        .and_then(|r| r.get("tracks"))
        .and_then(|t| t.get("data"))
        .and_then(|d| d.as_array())
    {
        let items: Vec<_> = tracks
            .iter()
            .filter_map(mapper::map_apple_resource_to_item)
            .collect();
        let mut section = PageSectionWire::new("tracks", None, items).with_hint("track-list");
        if let Some(next) = playlist
            .get("relationships")
            .and_then(|r| r.get("tracks"))
            .and_then(|t| t.get("next"))
            .and_then(|n| n.as_str())
        {
            section.continuation = Some(PageCursorWire::section("tracks", encode_cursor(next)));
        }
        page.sections.push(section);
    }

    Ok(page)
}

// ──────────────────────── REPLAY ────────────────────────

async fn build_replay(
    api: &OfficialAppleMusicApi,
    year: &str,
) -> Result<ApplePageWire, AppleError> {
    let filter_val = if year == "latest" { "latest" } else { year };
    let data = api
        .send_request(
            "/v1/me/music-summaries",
            &[
                ("filter[year]", filter_val),
                ("views", "top-artists,top-albums,top-songs"),
            ],
        )
        .await?;

    let summary = data
        .get("data")
        .and_then(|d| d.as_array())
        .and_then(|a| a.first())
        .ok_or_else(|| AppleError::NotFound(format!("Replay for year {year} not found")))?;

    let actual_year = summary
        .get("attributes")
        .and_then(|a| a.get("year"))
        .and_then(|y| y.as_u64())
        .map(|y| y.to_string())
        .unwrap_or_else(|| year.to_string());

    let page_id = format!("apple:page:replay:{actual_year}");
    let mut page = ApplePageWire::new(&page_id, format!("Replay {actual_year}"));

    if let Some(views) = summary.get("views").and_then(|v| v.as_object()) {
        let view_order = ["top-songs", "top-albums", "top-artists"];
        let view_labels = ["Top Songs", "Top Albums", "Top Artists"];
        for (view_key, label) in view_order.iter().zip(view_labels.iter()) {
            if let Some(view) = views.get(*view_key)
                && let Some(view_data) = view.get("data").and_then(|d| d.as_array())
            {
                if view_data.is_empty() {
                    continue;
                }
                let items: Vec<_> = view_data
                    .iter()
                    .filter_map(mapper::map_period_summary_to_item)
                    .collect();
                if !items.is_empty() {
                    let hint = if *view_key == "top-songs" {
                        "track-list"
                    } else {
                        "grid"
                    };
                    let section = PageSectionWire::new(*view_key, Some(label.to_string()), items)
                        .with_hint(hint);
                    page.sections.push(section);
                }
            }
        }
    }

    Ok(page)
}

// ──────────────────────── Cursor Encoding ────────────────────────

fn encode_cursor(url: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(url.as_bytes())
}

fn decode_cursor(token: &str) -> Result<String, AppleError> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(token)
        .map_err(|e| AppleError::Internal(format!("Invalid cursor token: {e}")))?;
    String::from_utf8(bytes)
        .map_err(|e| AppleError::Internal(format!("Invalid cursor encoding: {e}")))
}

fn extract_next_cursor(val: &Value, section_id: Option<&str>) -> Option<PageCursorWire> {
    let next = val.get("next").and_then(|n| n.as_str())?;
    let token = encode_cursor(next);
    Some(if let Some(sid) = section_id {
        PageCursorWire::section(sid, token)
    } else {
        PageCursorWire::new(token)
    })
}
