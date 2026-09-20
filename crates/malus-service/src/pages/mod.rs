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

pub mod artist;
pub mod categories_data;
pub mod curator;
pub mod groupings;
pub mod home;
pub mod manifest;
pub mod mapper;

use malus_ipc::wire::{
    PageActionWire, PageBadgeWire, PageContinuationWire, PageCursorWire, PageHeaderWire,
    PageItemWire, PageSectionWire, PageWire, SubtitleLinkWire,
};
use malus_model::{Artwork, PageRoute};
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
        PageRoute::Search => build_search_landing(api).await,
        PageRoute::LibraryRecentlyAdded => build_library_recently_added(api).await,
        PageRoute::LibrarySongs => build_library_songs(api).await,
        PageRoute::LibraryAlbums => build_library_albums(api).await,
        PageRoute::LibraryGenres => build_library_genres(api).await,
        PageRoute::LibraryArtists => build_library_artists(api).await,
        PageRoute::LibraryPlaylists => build_library_playlists(api).await,
        PageRoute::LibraryMadeForYou => build_library_made_for_you(api).await,
        PageRoute::Album(id) => build_album_detail(api, id).await,
        PageRoute::Artist(id) => build_artist_detail(api, id).await,
        PageRoute::Playlist(id) => build_playlist_detail(api, id).await,
        PageRoute::Curator(id) => curator::build_curator_page(id, api)
            .await
            .map_err(Into::into),
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
    if (next_url.contains("/v1/me/library/") || next_url.contains("/me/library/"))
        && !next_url.contains("extend=")
    {
        let sep = if next_url.contains('?') { '&' } else { '?' };
        next_url.push(sep);
        next_url.push_str("extend=inFavorites");
    }

    let resp = api.send_request(&next_url, &[]).await?;

    if let Some(section_id) = &cursor.section_id {
        let mut items = mapper::map_apple_data_to_items(&resp);
        let is_library = next_url.contains("/library/")
            || matches!(
                route,
                PageRoute::LibrarySongs
                    | PageRoute::LibraryAlbums
                    | PageRoute::LibraryGenres
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
        let sections = home::map_listen_now_response(&resp);
        let next = extract_next_cursor(&resp, None);
        Ok(PageContinuationWire::Sections {
            sections,
            continuation: next,
        })
    }
}

// ──────────────────────── HOME (LISTEN NOW) ────────────────────────

async fn build_home(api: &OfficialAppleMusicApi) -> Result<PageWire, AppleError> {
    let mut page = PageWire::new("home", "Home");

    let storefront = api.storefront();
    let locale = home::get_locale_for_storefront(&storefront);
    let tz_offset = home::get_local_timezone_offset();
    let query_params = home::build_listen_now_query(&tz_offset, &locale);
    let query_refs: Vec<(&str, &str)> =
        query_params.iter().map(|(k, v)| (*k, v.as_str())).collect();

    let resp = api
        .send_request(home::CANONICAL_LISTEN_NOW_URL, &query_refs)
        .await?;

    let sections = home::map_listen_now_response(&resp);
    page.sections = sections;

    if let Some(next) = resp.get("next").and_then(|n| n.as_str()) {
        page.continuation = Some(PageCursorWire::new(encode_cursor(next)));
    }

    Ok(page)
}

// ──────────────────────── NEW (BROWSE) ────────────────────────

async fn build_new(api: &OfficialAppleMusicApi) -> Result<PageWire, AppleError> {
    let mut page = PageWire::new("new", "Browse");
    page.subtitle = Some("Charts and new releases".to_string());

    let storefront = api.storefront();
    let locale = home::get_locale_for_storefront(&storefront);
    let query_params = groupings::build_grouping_query("music", &locale);
    let query_refs: Vec<(&str, &str)> = query_params.iter().map(|(k, v)| (*k, *v)).collect();

    // Canonical Apple Editorial Groupings for Browse
    if let Ok(resp) = api
        .send_request(groupings::CANONICAL_GROUPING_URL, &query_refs)
        .await
    {
        let sections = groupings::map_groupings_response(&resp, "music");
        if !sections.is_empty() {
            page.sections = sections;
            return Ok(page);
        }
    }

    // Fallback to catalog charts if grouping is unavailable
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
                        let section =
                            PageSectionWire::new(&section_id, Some(chart_name.to_string()), items)
                                .with_hint(hint);
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

    let storefront = api.storefront();
    let locale = home::get_locale_for_storefront(&storefront);
    let query_params = groupings::build_grouping_query("radio", &locale);
    let query_refs: Vec<(&str, &str)> = query_params.iter().map(|(k, v)| (*k, *v)).collect();

    // Canonical Apple Radio Editorial Grouping (Apple Music 1, Hits, Country, On Air Now, Shows, Interviews)
    let grouping_res = api
        .send_request(groupings::CANONICAL_GROUPING_URL, &query_refs)
        .await;

    let mut sections = match grouping_res {
        Ok(resp) => groupings::map_groupings_response(&resp, "radio"),
        Err(e) => {
            tracing::warn!("Failed to fetch Radio editorial groupings: {e}");
            Vec::new()
        }
    };

    // Personalized Recent Stations
    if let Ok(recent_data) = api
        .send_request("/v1/me/recent/radio-stations", &[("limit", "10")])
        .await
    {
        let recent_items = mapper::map_apple_data_to_items(&recent_data);
        if !recent_items.is_empty() {
            let recent_shelf = PageSectionWire::new(
                "recent-stations",
                Some("Recently Played".to_string()),
                recent_items,
            )
            .with_hint("stations-shelf");

            // User hierarchy: Live Stations -> On Air Now -> Recent Stations -> Shows
            let insert_pos = if sections.len() >= 2 {
                2
            } else if sections.len() == 1 {
                1
            } else {
                0
            };
            sections.insert(insert_pos, recent_shelf);
        }
    }

    page.sections = sections;
    Ok(page)
}

// ──────────────────────── SEARCH (STOREFRONT LANDING) ────────────────────────

async fn build_search_landing(api: &OfficialAppleMusicApi) -> Result<PageWire, AppleError> {
    let mut page = PageWire::new("search", "Search");
    page.subtitle = Some("Explore by Category".to_string());

    let storefront = api.storefront();
    let locale = home::get_locale_for_storefront(&storefront);

    let query_params = [
        ("name", "search-landing"),
        ("l", locale.as_str()),
        ("platform", "web"),
    ];

    let personal_url = format!(
        "https://amp-api.music.apple.com/v1/recommendations/{storefront}/personal-recommendation"
    );

    let mut sections = Vec::new();

    // 1. Try official dynamic search-landing recommendations
    let rec_res = match api
        .send_request(
            "https://amp-api.music.apple.com/v1/me/recommendations",
            &query_params,
        )
        .await
    {
        Ok(resp) => Ok(resp),
        Err(_) => api.send_request(&personal_url, &query_params).await,
    };

    if let Ok(resp) = rec_res
        && let Some(data) = resp.get("data").and_then(|d| d.as_array())
    {
        let mut dynamic_items = Vec::new();
        for rec in data {
            if let Some(contents) = rec
                .get("relationships")
                .and_then(|r| r.get("contents"))
                .and_then(|c| c.get("data"))
                .and_then(|d| d.as_array())
            {
                for item in contents {
                    if let Some(mut page_item) = mapper::map_apple_resource_to_item(item) {
                        page_item.presentation_hint = Some("category-brick".to_string());
                        dynamic_items.push(page_item);
                    }
                }
            }
        }

        if !dynamic_items.is_empty() {
            let section = PageSectionWire::new(
                "browse-categories",
                Some("Browse Categories".to_string()),
                dynamic_items,
            )
            .with_hint("grid");
            sections.push(section);
        }
    }

    // 2. Fallback to comprehensive 50 official BROWSE_CATEGORIES if dynamic returned nothing (or offline)
    if sections.is_empty() {
        let mut category_items = Vec::with_capacity(malus_ipc::wire::BROWSE_CATEGORIES.len());
        for cat in malus_ipc::wire::BROWSE_CATEGORIES {
            let mut item = PageItemWire::new(format!("category:{}", cat.id), cat.title);
            item.artwork = Some(Artwork::new(cat.artwork_url).with_dimensions(640, 360));
            item.open_route = PageRoute::parse(cat.route);
            item.bg_color = Some(cat.bg_color.to_string());
            item.presentation_hint = Some("category-brick".to_string());
            category_items.push(item);
        }

        let section = PageSectionWire::new(
            "browse-categories",
            Some("Browse Categories".to_string()),
            category_items,
        )
        .with_hint("grid");
        sections.push(section);
    }

    page.sections = sections;
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

async fn build_library_genres(api: &OfficialAppleMusicApi) -> Result<PageWire, AppleError> {
    build_library_list(
        api,
        "library:genres",
        "Genres",
        "/v1/me/library/albums",
        "genre-list",
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
    let mut query = vec![
        ("limit", limit),
        ("include", include),
        ("extend", "inFavorites"),
    ];
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

fn push_known_artist(
    known_artists: &mut Vec<(Option<String>, String)>,
    name: Option<&str>,
    id: &str,
) {
    let clean_id = id.trim();
    if clean_id.is_empty() {
        return;
    }
    if !known_artists
        .iter()
        .any(|(_, existing_id)| existing_id == clean_id)
    {
        known_artists.push((name.map(|n| n.trim().to_string()), clean_id.to_string()));
    } else if let Some(n) = name
        && let Some((existing_name, _)) = known_artists.iter_mut().find(|(_, eid)| eid == clean_id)
        && existing_name.is_none()
        && !n.trim().is_empty()
    {
        *existing_name = Some(n.trim().to_string());
    }
}

fn extract_album_artists(
    album: &serde_json::Value,
    attrs: &serde_json::Value,
    raw_artist_name: &str,
) -> (Option<String>, Option<PageRoute>, Vec<SubtitleLinkWire>) {
    if raw_artist_name.trim().is_empty() {
        return (None, None, Vec::new());
    }

    let mut known_artists: Vec<(Option<String>, String)> = Vec::new();

    // 1. From album.relationships.artists.data
    if let Some(arr) = album
        .get("relationships")
        .and_then(|r| r.get("artists"))
        .and_then(|a| a.get("data"))
        .and_then(|d| d.as_array())
    {
        for a in arr {
            let id = a.get("id").and_then(|i| i.as_str());
            let name = a
                .get("attributes")
                .and_then(|at| at.get("name"))
                .and_then(|n| n.as_str())
                .or_else(|| a.get("name").and_then(|n| n.as_str()));
            if let Some(id) = id {
                push_known_artist(&mut known_artists, name, id);
            }
        }
    }

    // 2. From tracks: album.relationships.tracks.data
    if let Some(tracks) = album
        .get("relationships")
        .and_then(|r| r.get("tracks"))
        .and_then(|t| t.get("data"))
        .and_then(|d| d.as_array())
    {
        for t in tracks {
            if let Some(arr) = t
                .get("relationships")
                .and_then(|r| r.get("artists"))
                .and_then(|a| a.get("data"))
                .and_then(|d| d.as_array())
            {
                for a in arr {
                    let id = a.get("id").and_then(|i| i.as_str());
                    let name = a
                        .get("attributes")
                        .and_then(|at| at.get("name"))
                        .and_then(|n| n.as_str())
                        .or_else(|| a.get("name").and_then(|n| n.as_str()));
                    if let Some(id) = id {
                        push_known_artist(&mut known_artists, name, id);
                    }
                }
            }
        }
    }

    // 3. From attrs.artistUrl if known_artists is still empty
    if known_artists.is_empty()
        && let Some(id) = attrs
            .get("artistUrl")
            .and_then(|u| u.as_str())
            .and_then(|url| url.trim_end_matches('/').rsplit('/').next())
    {
        push_known_artist(&mut known_artists, Some(raw_artist_name), id);
    }

    let primary_route = known_artists
        .first()
        .map(|(_, id)| PageRoute::Artist(id.clone()));

    // Check if raw_artist_name matches a single known artist exactly (e.g. "Simon & Garfunkel")
    if let Some((_, id)) = known_artists.iter().find(|(name, _)| {
        name.as_deref()
            .map(|n| n.eq_ignore_ascii_case(raw_artist_name))
            .unwrap_or(false)
    }) {
        let links = vec![SubtitleLinkWire::new(
            raw_artist_name,
            Some(PageRoute::Artist(id.clone())),
        )];
        return (
            Some(raw_artist_name.to_string()),
            Some(PageRoute::Artist(id.clone())),
            links,
        );
    }

    // If raw_artist_name contains delimiters like " & " or ", "
    let tokens: Vec<&str> = raw_artist_name
        .split(", ")
        .flat_map(|part| part.split(" & "))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();

    let mut links = Vec::new();
    if tokens.len() > 1 {
        for (idx, token) in tokens.iter().enumerate() {
            let matched_id = known_artists
                .iter()
                .find(|(name, _)| {
                    name.as_deref()
                        .map(|n| n.eq_ignore_ascii_case(token))
                        .unwrap_or(false)
                })
                .map(|(_, id)| id.clone())
                .or_else(|| {
                    if known_artists.len() == tokens.len() {
                        Some(known_artists[idx].1.clone())
                    } else if idx == 0 {
                        known_artists.first().map(|(_, id)| id.clone())
                    } else {
                        None
                    }
                });
            links.push(SubtitleLinkWire::new(
                *token,
                matched_id.map(PageRoute::Artist),
            ));
        }
    } else {
        links.push(SubtitleLinkWire::new(
            raw_artist_name,
            primary_route.clone(),
        ));
    }

    (Some(raw_artist_name.to_string()), primary_route, links)
}

async fn build_album_detail(
    api: &OfficialAppleMusicApi,
    album_id: &str,
) -> Result<PageWire, AppleError> {
    if album_id.starts_with("l.") {
        return build_library_album_detail(api, album_id).await;
    }

    let path =
        format!("https://amp-api.music.apple.com/v1/catalog/{{storefront}}/albums/{album_id}");
    let catalog_res = api
        .send_request(
            &path,
            &[
                ("include", "tracks,artists,record-labels"),
                (
                    "views",
                    "more-by-artist,other-versions,appears-on,related-videos,you-might-also-like",
                ),
                ("extend", "editorialArtwork,editorialVideo,editorialNotes"),
            ],
        )
        .await;

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
    let (sub, sub_route, sub_links) = extract_album_artists(album, attrs, artist);
    header.subtitle = sub;
    header.subtitle_route = sub_route;
    header.subtitle_links = sub_links;
    header.artwork = attrs
        .get("artwork")
        .and_then(crate::api::parse::parse_apple_artwork);

    if let Some(notes) = attrs.get("editorialNotes") {
        header.description = notes
            .get("standard")
            .or_else(|| notes.get("short"))
            .and_then(|s| s.as_str())
            .map(str::to_string);
    }

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

    // Related views appended below tracklist
    if let Some(views) = album.get("views").and_then(|v| v.as_object()) {
        let view_order = [
            ("more-by-artist", "More by Artist", "shelf"),
            ("you-might-also-like", "You Might Also Like", "shelf"),
            ("appears-on", "Featured On", "shelf"),
            ("other-versions", "Other Versions", "shelf"),
            ("related-videos", "Music Videos", "video-shelf"),
        ];
        for (view_key, fallback_title, hint) in view_order {
            if let Some(view) = views.get(view_key) {
                let view_title = view
                    .get("attributes")
                    .and_then(|a| a.get("title").and_then(|t| t.as_str()))
                    .unwrap_or(fallback_title);
                if let Some(view_data) = view.get("data").and_then(|d| d.as_array()) {
                    if view_data.is_empty() {
                        continue;
                    }
                    let items: Vec<_> = view_data
                        .iter()
                        .filter_map(mapper::map_apple_resource_to_item)
                        .collect();
                    if !items.is_empty() {
                        let section_id = format!("view:{view_key}");
                        page.sections.push(
                            PageSectionWire::new(section_id, Some(view_title.to_string()), items)
                                .with_hint(hint),
                        );
                    }
                }
            }
        }
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
    let (sub, sub_route, sub_links) = extract_album_artists(album, attrs, artist);
    header.subtitle = sub;
    header.subtitle_route = sub_route;
    header.subtitle_links = sub_links;
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
    artist::build_artist_page(api, artist_id).await
}

// ──────────────────────── PLAYLIST DETAIL ────────────────────────

async fn build_playlist_detail(
    api: &OfficialAppleMusicApi,
    playlist_id: &str,
) -> Result<PageWire, AppleError> {
    if playlist_id.starts_with("p.") {
        return build_library_playlist_detail(api, playlist_id).await;
    }

    let path = format!(
        "https://amp-api.music.apple.com/v1/catalog/{{storefront}}/playlists/{playlist_id}"
    );
    let catalog_res = api
        .send_request(
            &path,
            &[
                ("include", "tracks,curator"),
                ("views", "featured-artists,more-by-curator"),
                ("extend", "editorialArtwork,editorialVideo"),
                ("relate", "library"),
                (
                    "fields[playlists]",
                    "name,description,artwork,curatorName,inLibrary,inFavorites,isFavorite,canEdit,canDelete",
                ),
            ],
        )
        .await;

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
        .and_then(|d| d.get("standard").or_else(|| d.get("short")))
        .and_then(|s| s.as_str())
    {
        header.description = Some(desc.to_string());
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

    // Related discovery appended below tracklist
    if let Some(views) = playlist.get("views").and_then(|v| v.as_object()) {
        let view_order = [
            ("featured-artists", "Featured Artists", "artist-shelf"),
            ("more-by-curator", "More by Curator", "shelf"),
        ];
        for (view_key, fallback_title, hint) in view_order {
            if let Some(view) = views.get(view_key) {
                let view_title = view
                    .get("attributes")
                    .and_then(|a| a.get("title").and_then(|t| t.as_str()))
                    .unwrap_or(fallback_title);
                if let Some(view_data) = view.get("data").and_then(|d| d.as_array()) {
                    if view_data.is_empty() {
                        continue;
                    }
                    let items: Vec<_> = view_data
                        .iter()
                        .filter_map(mapper::map_apple_resource_to_item)
                        .collect();
                    if !items.is_empty() {
                        let section_id = format!("view:{view_key}");
                        page.sections.push(
                            PageSectionWire::new(section_id, Some(view_title.to_string()), items)
                                .with_hint(hint),
                        );
                    }
                }
            }
        }
    }

    Ok(page)
}

async fn build_library_playlist_detail(
    api: &OfficialAppleMusicApi,
    playlist_id: &str,
) -> Result<PageWire, AppleError> {
    let path = format!("/v1/me/library/playlists/{playlist_id}");
    let data = api
        .send_request(
            &path,
            &[
                ("include", "tracks,catalog"),
                (
                    "fields[library-playlists]",
                    "name,description,artwork,canEdit,canDelete,inFavorites,isFavorite,playParams,dateAdded",
                ),
            ],
        )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_album_artists_collab_single() {
        let album = serde_json::json!({
            "relationships": {
                "artists": {
                    "data": [
                        { "id": "1560945939", "type": "artists", "attributes": { "name": "Natkhat" } }
                    ]
                },
                "tracks": {
                    "data": [
                        {
                            "id": "1001",
                            "relationships": {
                                "artists": {
                                    "data": [
                                        { "id": "1560945939", "attributes": { "name": "Natkhat" } },
                                        { "id": "1612345678", "attributes": { "name": "Chaar Diwaari" } }
                                    ]
                                }
                            }
                        }
                    ]
                }
            }
        });
        let attrs = serde_json::json!({
            "artistName": "Natkhat & Chaar Diwaari"
        });

        let (sub, primary_route, links) =
            extract_album_artists(&album, &attrs, "Natkhat & Chaar Diwaari");
        assert_eq!(sub, Some("Natkhat & Chaar Diwaari".to_string()));
        assert_eq!(
            primary_route,
            Some(PageRoute::Artist("1560945939".to_string()))
        );
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].text, "Natkhat");
        assert_eq!(
            links[0].route,
            Some(PageRoute::Artist("1560945939".to_string()))
        );
        assert_eq!(links[1].text, "Chaar Diwaari");
        assert_eq!(
            links[1].route,
            Some(PageRoute::Artist("1612345678".to_string()))
        );
    }

    #[test]
    fn test_extract_album_artists_single_band_with_ampersand() {
        let album = serde_json::json!({
            "relationships": {
                "artists": {
                    "data": [
                        { "id": "12345", "type": "artists", "attributes": { "name": "Simon & Garfunkel" } }
                    ]
                }
            }
        });
        let attrs = serde_json::json!({
            "artistName": "Simon & Garfunkel"
        });

        let (sub, primary_route, links) =
            extract_album_artists(&album, &attrs, "Simon & Garfunkel");
        assert_eq!(sub, Some("Simon & Garfunkel".to_string()));
        assert_eq!(primary_route, Some(PageRoute::Artist("12345".to_string())));
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].text, "Simon & Garfunkel");
        assert_eq!(links[0].route, Some(PageRoute::Artist("12345".to_string())));
    }

    #[test]
    fn test_extract_album_artists_single_artist() {
        let album = serde_json::json!({
            "relationships": {
                "artists": {
                    "data": [
                        { "id": "1440857780", "type": "artists", "attributes": { "name": "Daft Punk" } }
                    ]
                }
            }
        });
        let attrs = serde_json::json!({
            "artistName": "Daft Punk"
        });

        let (sub, primary_route, links) = extract_album_artists(&album, &attrs, "Daft Punk");
        assert_eq!(sub, Some("Daft Punk".to_string()));
        assert_eq!(
            primary_route,
            Some(PageRoute::Artist("1440857780".to_string()))
        );
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].text, "Daft Punk");
        assert_eq!(
            links[0].route,
            Some(PageRoute::Artist("1440857780".to_string()))
        );
    }
}
