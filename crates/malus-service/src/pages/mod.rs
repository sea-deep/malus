//! Apple Music page implementation.
//!
//! Generates structured `PageWire` documents for:
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

use malus_ipc::wire::{
    PageActionWire, PageBadgeWire, PageContinuationWire, PageCursorWire, PageHeaderWire,
    PageItemWire, PageSectionWire, PageWire,
};
use malus_model::{MediaRef, PageRoute};
use serde_json::Value;

use crate::{api::OfficialAppleMusicApi, error::AppleError};

/// Resolve a page route to a PageWire response.
pub async fn get_apple_page(
    api: &OfficialAppleMusicApi,
    route: &PageRoute,
) -> Result<PageWire, AppleError> {
    match route {
        PageRoute::Home => build_home(api).await,
        PageRoute::New => build_new(api).await,
        PageRoute::Radio => build_radio(api).await,
        PageRoute::LibraryRecentlyAdded => build_library_recently_added(api).await,
        PageRoute::LibrarySongs => build_library_songs(api).await,
        PageRoute::LibraryAlbums => build_library_albums(api).await,
        PageRoute::LibraryArtists => build_library_artists(api).await,
        PageRoute::LibraryPlaylists => build_library_playlists(api).await,
        PageRoute::LibraryMadeForYou => build_library_made_for_you(api).await,
        PageRoute::Album(id) => build_album_detail(api, id).await,
        PageRoute::Artist(id) => build_artist_detail(api, id).await,
        PageRoute::Playlist(id) => build_playlist_detail(api, id).await,
        PageRoute::Replay(year) => build_replay(api, *year).await,
    }
}

/// Continue pagination for an Apple page.
pub async fn continue_apple_page(
    api: &OfficialAppleMusicApi,
    route: &PageRoute,
    cursor: &PageCursorWire,
) -> Result<PageContinuationWire, AppleError> {
    let mut next_url = decode_cursor(&cursor.token)?;
    if (next_url.contains("/v1/me/library/") || next_url.contains("/me/library/"))
        && !next_url.contains("include=")
    {
        let sep = if next_url.contains('?') { '&' } else { '?' };
        next_url.push(sep);
        next_url.push_str("include=catalog");
    }

    let resp = api.send_request(&next_url, &[]).await?;

    if let Some(section_id) = &cursor.section_id {
        let mut items = mapper::map_apple_data_to_items(&resp);
        let is_library = next_url.contains("/library/")
            || matches!(
                route,
                PageRoute::LibrarySongs
                    | PageRoute::LibraryAlbums
                    | PageRoute::LibraryArtists
                    | PageRoute::LibraryPlaylists
                    | PageRoute::LibraryRecentlyAdded
            );
        if is_library {
            for item in &mut items {
                item.in_library = Some(true);
                item.actions
                    .retain(|a| !matches!(a, PageActionWire::AddToLibrary(_)));
            }
        }
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

// ──────────────────────── HOME ────────────────────────

fn determine_top_pick_overline(group_title: &str, item_name: &str) -> String {
    let lower = group_title.to_lowercase();
    if lower.starts_with("more from") {
        group_title.to_string()
    } else if lower.contains("fans like") || lower.contains("fans also like") {
        if let Some(artist) = group_title.split(" Fans").next() {
            format!("Featuring {artist}")
        } else {
            "Featuring".to_string()
        }
    } else if lower.contains("new release") {
        "New Release".to_string()
    } else if lower.contains("made for you") {
        "Made for You".to_string()
    } else if lower.contains("station") {
        if item_name.contains("& Similar Artists") {
            let artist = item_name
                .replace("& Similar Artists", "")
                .trim()
                .to_string();
            format!("Featuring {artist}")
        } else if item_name.to_lowercase().contains("station") {
            "Made for You".to_string()
        } else {
            "Station".to_string()
        }
    } else if lower.contains("recently played") || lower.contains("heavy rotation") {
        "Listen Again".to_string()
    } else {
        group_title.to_string()
    }
}

async fn build_home(api: &OfficialAppleMusicApi) -> Result<PageWire, AppleError> {
    let mut page = PageWire::new("home", "Home");

    // Fetch recommendations, recently played, and heavy rotation sequentially
    let rec_res = api
        .send_request("/v1/me/recommendations", &[("limit", "10")])
        .await;
    let rp_res = api
        .send_request("/v1/me/recent/played", &[("limit", "10")])
        .await;
    let hr_res = api
        .send_request("/v1/me/history/heavy-rotation", &[("limit", "10")])
        .await;

    let mut top_picks: Vec<PageItemWire> = Vec::new();
    let mut rec_sections: Vec<PageSectionWire> = Vec::new();
    let mut rec_continuation = None;

    // Process recommendations
    if let Ok(rec_data) = rec_res {
        let _ = tokio::fs::write(
            "/tmp/rec_data.json",
            serde_json::to_string_pretty(&rec_data).unwrap_or_default(),
        )
        .await;

        if let Some(next) = rec_data.get("next").and_then(|n| n.as_str()) {
            rec_continuation = Some(PageCursorWire::new(encode_cursor(next)));
        }

        if let Some(groups) = rec_data.get("data").and_then(|d| d.as_array()) {
            let mut sampled_groups: Vec<Vec<PageItemWire>> = Vec::new();

            for group in groups {
                let title = group
                    .get("attributes")
                    .and_then(|a| a.get("title"))
                    .and_then(|t| t.get("stringForDisplay"))
                    .and_then(|s| s.as_str())
                    .unwrap_or("Recommendations");

                if let Some(contents) = group
                    .get("relationships")
                    .and_then(|r| r.get("contents"))
                    .and_then(|c| c.get("data"))
                    .and_then(|d| d.as_array())
                {
                    let mut group_items: Vec<PageItemWire> = Vec::new();
                    for item in contents {
                        if let Some(mut page_item) = mapper::map_apple_resource_to_item(item) {
                            let overline = determine_top_pick_overline(title, &page_item.title);
                            page_item.tertiary_text = Some(overline);
                            group_items.push(page_item);
                        }
                    }
                    if !group_items.is_empty() {
                        sampled_groups.push(group_items);
                    }
                }
            }

            // Interleave items across sampled groups (round-robin) up to 12 items
            for round in 0..3 {
                for group in &sampled_groups {
                    if top_picks.len() >= 12 {
                        break;
                    }
                    if let Some(item) = group
                        .get(round)
                        .filter(|item| !top_picks.iter().any(|existing| existing.id == item.id))
                    {
                        top_picks.push(item.clone());
                    }
                }
            }

            rec_sections = mapper::map_recommendations_to_sections(&rec_data);
        }
    }

    // Top Picks for You Shelf at top of Home
    if !top_picks.is_empty() {
        let section = PageSectionWire::new(
            "top-picks",
            Some("Top Picks for You".to_string()),
            top_picks,
        )
        .with_hint("top-picks-shelf");
        page.sections.push(section);
    }

    // Recently Played
    if let Ok(rp_data) = rp_res {
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

    // Recommendations Shelves
    page.sections.extend(rec_sections);
    page.continuation = rec_continuation;

    // Heavy Rotation
    if let Ok(hr_data) = hr_res {
        let items = mapper::map_apple_data_to_items(&hr_data);
        if !items.is_empty() {
            let section =
                PageSectionWire::new("heavy-rotation", Some("Heavy Rotation".to_string()), items)
                    .with_hint("shelf");
            page.sections.push(section);
        }
    }

    Ok(page)
}

// ──────────────────────── NEW (CHARTS) ────────────────────────

async fn build_new(api: &OfficialAppleMusicApi) -> Result<PageWire, AppleError> {
    let mut page = PageWire::new("new", "Browse");
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

async fn build_radio(api: &OfficialAppleMusicApi) -> Result<PageWire, AppleError> {
    let mut page = PageWire::new("radio", "Radio");

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

async fn build_library_recently_added(api: &OfficialAppleMusicApi) -> Result<PageWire, AppleError> {
    build_library_list(
        api,
        "library:recently-added",
        "Recently Added",
        "/v1/me/library/recently-added",
        "grid",
    )
    .await
}

async fn build_library_songs(api: &OfficialAppleMusicApi) -> Result<PageWire, AppleError> {
    build_library_list(
        api,
        "library:songs",
        "Songs",
        "/v1/me/library/songs",
        "track-list",
    )
    .await
}

async fn build_library_albums(api: &OfficialAppleMusicApi) -> Result<PageWire, AppleError> {
    build_library_list(
        api,
        "library:albums",
        "Albums",
        "/v1/me/library/albums",
        "grid",
    )
    .await
}

async fn build_library_artists(api: &OfficialAppleMusicApi) -> Result<PageWire, AppleError> {
    build_library_list(
        api,
        "library:artists",
        "Artists",
        "/v1/me/library/artists",
        "artist-list",
    )
    .await
}

async fn build_library_playlists(api: &OfficialAppleMusicApi) -> Result<PageWire, AppleError> {
    build_library_list(
        api,
        "library:playlists",
        "Playlists",
        "/v1/me/library/playlists",
        "grid",
    )
    .await
}

async fn build_library_made_for_you(api: &OfficialAppleMusicApi) -> Result<PageWire, AppleError> {
    let data = api
        .send_request(
            "/v1/me/library/playlists",
            &[("limit", "100"), ("include", "catalog")],
        )
        .await?;

    let mut items = Vec::new();
    if let Some(arr) = data.get("data").and_then(|d| d.as_array()) {
        for res in arr {
            let cat_id = res
                .get("relationships")
                .and_then(|r| r.get("catalog"))
                .and_then(|c| c.get("data"))
                .and_then(|d| d.as_array())
                .and_then(|a| a.first())
                .and_then(|e| e.get("id"))
                .and_then(|i| i.as_str());

            let is_personal_mix = cat_id.is_some_and(|id| id.starts_with("pl.pm-"));

            if !is_personal_mix {
                continue;
            }

            if let Some(mut item) = mapper::map_apple_resource_to_item(res) {
                item.in_library = Some(true);
                item.actions
                    .retain(|a| !matches!(a, PageActionWire::AddToLibrary(_)));
                items.push(item);
            }
        }
    }

    let mut page = PageWire::new("library:made-for-you", "Made for You");
    let section = PageSectionWire::new("all", None, items).with_hint("grid");
    page.sections.push(section);
    Ok(page)
}

async fn build_library_list(
    api: &OfficialAppleMusicApi,
    page_id: &str,
    title: &str,
    endpoint: &str,
    hint: &str,
) -> Result<PageWire, AppleError> {
    let limit = if endpoint == "/v1/me/library/recently-added" {
        "25"
    } else {
        "100"
    };
    let include = "catalog";
    let mut query = vec![("limit", limit), ("include", include)];
    // Default to time-based sorting (most recently added first) for endpoints supporting it
    if endpoint != "/v1/me/library/artists" && endpoint != "/v1/me/library/recently-added" {
        query.push(("sort", "-dateAdded"));
    }
    let data = api.send_request(endpoint, &query).await?;
    let mut items = mapper::map_apple_data_to_items(&data);
    for item in &mut items {
        item.in_library = Some(true);
        item.actions
            .retain(|a| !matches!(a, PageActionWire::AddToLibrary(_)));
    }
    let mut page = PageWire::new(page_id, title);
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
) -> Result<PageWire, AppleError> {
    if album_id.starts_with("l.") {
        return build_library_album_detail(api, album_id).await;
    }

    let path = format!("/v1/catalog/{{storefront}}/albums/{album_id}");
    let catalog_res = api.send_request(&path, &[("include", "tracks")]).await;

    let data = match catalog_res {
        Ok(d) => d,
        Err(e) => {
            if matches!(e, crate::api::AppleApiError::NotFound(_)) {
                return build_library_album_detail(api, album_id).await;
            }
            return Err(e.into());
        }
    };

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
    let page_id = format!("album:{album_id}");

    let mut page = PageWire::new(&page_id, name);
    let mut header = PageHeaderWire::new(name);
    header.actions = mapper::map_detail_actions(album);
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

async fn build_library_album_detail(
    api: &OfficialAppleMusicApi,
    album_id: &str,
) -> Result<PageWire, AppleError> {
    let path = format!("/v1/me/library/albums/{album_id}");
    let data = api
        .send_request(&path, &[("include", "tracks,catalog")])
        .await?;

    let album = data
        .get("data")
        .and_then(|d| d.as_array())
        .and_then(|a| a.first())
        .ok_or_else(|| AppleError::NotFound(format!("Library album {album_id} not found")))?;

    if let Some(cat_id) = album
        .get("relationships")
        .and_then(|r| r.get("catalog"))
        .and_then(|c| c.get("data"))
        .and_then(|d| d.as_array())
        .and_then(|a| a.first())
        .and_then(|e| e.get("id"))
        .and_then(|i| i.as_str())
        && let Ok(cat_page) = Box::pin(build_album_detail(api, cat_id)).await
    {
        return Ok(cat_page);
    }

    let attrs = album.get("attributes").unwrap_or(album);
    let name = attrs
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("Album");
    let artist = attrs
        .get("artistName")
        .and_then(|a| a.as_str())
        .unwrap_or("");
    let page_id = format!("album:{album_id}");

    let mut page = PageWire::new(&page_id, name);
    let mut header = PageHeaderWire::new(name);
    header.actions = mapper::map_detail_actions(album);
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
) -> Result<PageWire, AppleError> {
    if artist_id.starts_with("r.") || artist_id.starts_with("l.") {
        return build_library_artist_detail(api, artist_id).await;
    }

    let path = format!("/v1/catalog/{{storefront}}/artists/{artist_id}");
    let catalog_res = api
        .send_request(
            &path,
            &[(
                "views",
                "top-songs,full-albums,singles,similar-artists,latest-release",
            )],
        )
        .await;

    let data = match catalog_res {
        Ok(d) => d,
        Err(e) => {
            if matches!(e, crate::api::AppleApiError::NotFound(_)) {
                return build_library_artist_detail(api, artist_id).await;
            }
            return Err(e.into());
        }
    };

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
    let page_id = format!("artist:{artist_id}");

    let mut page = PageWire::new(&page_id, name);
    let mut header = PageHeaderWire::new(name);
    header.actions = mapper::map_detail_actions(artist);
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

async fn build_library_artist_detail(
    api: &OfficialAppleMusicApi,
    artist_id: &str,
) -> Result<PageWire, AppleError> {
    let path = format!("/v1/me/library/artists/{artist_id}");
    let data = api
        .send_request(&path, &[("include", "catalog,albums")])
        .await?;

    let artist = data
        .get("data")
        .and_then(|d| d.as_array())
        .and_then(|a| a.first())
        .ok_or_else(|| AppleError::NotFound(format!("Library artist {artist_id} not found")))?;

    let attrs = artist.get("attributes").unwrap_or(artist);
    let name = attrs
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("Artist");
    let page_id = format!("artist:{artist_id}");

    let mut page = PageWire::new(&page_id, name);
    let mut header = PageHeaderWire::new(name);

    let catalog_art = artist
        .get("relationships")
        .and_then(|r| r.get("catalog"))
        .and_then(|c| c.get("data"))
        .and_then(|d| d.as_array())
        .and_then(|a| a.first())
        .and_then(|cat| cat.get("attributes"))
        .and_then(|a| a.get("artwork"));

    let first_album_art = artist
        .get("relationships")
        .and_then(|r| r.get("albums"))
        .and_then(|a| a.get("data"))
        .and_then(|d| d.as_array())
        .and_then(|a| a.first())
        .and_then(|alb| alb.get("attributes"))
        .and_then(|a| a.get("artwork"));

    header.artwork = attrs
        .get("artwork")
        .or(catalog_art)
        .or(first_album_art)
        .and_then(crate::api::parse::parse_apple_artwork);

    let cat_id = artist
        .get("relationships")
        .and_then(|r| r.get("catalog"))
        .and_then(|c| c.get("data"))
        .and_then(|d| d.as_array())
        .and_then(|a| a.first())
        .and_then(|e| e.get("id"))
        .and_then(|i| i.as_str());

    if let Some(cid) = cat_id {
        header.metadata.push(format!("catalog_id:{cid}"));
        header
            .actions
            .push(PageActionWire::Play(MediaRef::Artist(cid.to_string())));
        header
            .actions
            .push(PageActionWire::Favorite(MediaRef::Artist(cid.to_string())));
    }
    // Library-only artists have no catalog top-songs endpoint. Their albums
    // remain playable below; do not advertise an invalid catalog action.

    page.header = Some(header);

    // Only albums from user library
    let mut items = Vec::new();
    if let Some(albums) = artist
        .get("relationships")
        .and_then(|r| r.get("albums"))
        .and_then(|a| a.get("data"))
        .and_then(|d| d.as_array())
    {
        for alb in albums {
            if let Some(mut it) = mapper::map_apple_resource_to_item(alb) {
                it.in_library = Some(true);
                it.actions
                    .retain(|a| !matches!(a, PageActionWire::AddToLibrary(_)));
                items.push(it);
            }
        }
    }

    if items.is_empty() {
        let albums_path = format!("/v1/me/library/artists/{artist_id}/albums");
        if let Ok(albums_data) = api
            .send_request(&albums_path, &[("limit", "100"), ("include", "catalog")])
            .await
        {
            let mut fetched = mapper::map_apple_data_to_items(&albums_data);
            for it in &mut fetched {
                it.in_library = Some(true);
                it.actions
                    .retain(|a| !matches!(a, PageActionWire::AddToLibrary(_)));
            }
            items = fetched;
        }
    }

    if !items.is_empty() {
        let section =
            PageSectionWire::new("albums", Some("Albums".to_string()), items).with_hint("grid");
        page.sections.push(section);
    }

    Ok(page)
}

// ──────────────────────── PLAYLIST DETAIL ────────────────────────

async fn build_playlist_detail(
    api: &OfficialAppleMusicApi,
    playlist_id: &str,
) -> Result<PageWire, AppleError> {
    if playlist_id.starts_with("p.") {
        return build_library_playlist_detail(api, playlist_id).await;
    }

    let path = format!("/v1/catalog/{{storefront}}/playlists/{playlist_id}");
    let catalog_res = api.send_request(&path, &[("include", "tracks")]).await;

    let data = match catalog_res {
        Ok(d) => d,
        Err(e) => {
            if matches!(e, crate::api::AppleApiError::NotFound(_)) {
                return build_library_playlist_detail(api, playlist_id).await;
            }
            return Err(e.into());
        }
    };

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
    let page_id = format!("playlist:{playlist_id}");

    let mut page = PageWire::new(&page_id, name);
    let mut header = PageHeaderWire::new(name);
    header.actions = mapper::map_detail_actions(playlist);
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
    let can_edit = attrs["canEdit"].as_bool().unwrap_or(false);
    header.can_edit = can_edit;
    header.can_delete = attrs["canDelete"].as_bool().unwrap_or(can_edit);
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

async fn build_library_playlist_detail(
    api: &OfficialAppleMusicApi,
    playlist_id: &str,
) -> Result<PageWire, AppleError> {
    let path = format!("/v1/me/library/playlists/{playlist_id}");
    let data = api
        .send_request(&path, &[("include", "tracks,catalog")])
        .await?;

    let playlist = data
        .get("data")
        .and_then(|d| d.as_array())
        .and_then(|a| a.first())
        .ok_or_else(|| AppleError::NotFound(format!("Library playlist {playlist_id} not found")))?;

    if let Some(cat_id) = playlist
        .get("relationships")
        .and_then(|r| r.get("catalog"))
        .and_then(|c| c.get("data"))
        .and_then(|d| d.as_array())
        .and_then(|a| a.first())
        .and_then(|e| e.get("id"))
        .and_then(|i| i.as_str())
        && let Ok(cat_page) = Box::pin(build_playlist_detail(api, cat_id)).await
    {
        return Ok(cat_page);
    }

    let attrs = playlist.get("attributes").unwrap_or(playlist);
    let name = attrs
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("Playlist");
    let page_id = format!("playlist:{playlist_id}");

    let mut page = PageWire::new(&page_id, name);
    let mut header = PageHeaderWire::new(name);
    header.actions = mapper::map_detail_actions(playlist);
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
    let can_edit = attrs["canEdit"].as_bool().unwrap_or(false);
    header.can_edit = can_edit;
    header.can_delete = attrs["canDelete"].as_bool().unwrap_or(can_edit);
    page.header = Some(header);

    // Track list
    if let Some(tracks) = playlist
        .get("relationships")
        .and_then(|r| r.get("tracks"))
        .and_then(|t| t.get("data"))
        .and_then(|d| d.as_array())
    {
        let is_fav_playlist = name == "Favourite Songs" || name == "Favorite Songs";

        let items: Vec<_> = tracks
            .iter()
            .filter_map(mapper::map_apple_resource_to_item)
            .map(|mut item| {
                item.in_library = Some(true);
                item.actions
                    .retain(|a| !matches!(a, PageActionWire::AddToLibrary(_)));
                if is_fav_playlist {
                    item.is_favorite = Some(true);
                    item.actions
                        .retain(|a| !matches!(a, PageActionWire::Favorite(_)));
                    if let Some(ref mref) = item.entity
                        && !item
                            .actions
                            .iter()
                            .any(|a| matches!(a, PageActionWire::Unfavorite(_)))
                    {
                        item.actions.push(PageActionWire::Unfavorite(mref.clone()));
                    }
                }
                item
            })
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

async fn build_replay(api: &OfficialAppleMusicApi, year: u16) -> Result<PageWire, AppleError> {
    let year_str = year.to_string();
    let data = api
        .send_request(
            "/v1/me/music-summaries",
            &[
                ("filter[year]", &year_str),
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
        .unwrap_or(year as u64) as u16;

    let page_id = format!("replay:{actual_year}");
    let mut page = PageWire::new(&page_id, format!("Replay {actual_year}"));

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
