//! Response mapper converting Apple Music API JSON into typed PageItemWire structures.
//!
//! Enforces the Apple-first architectural model:
//! - Directly maps Apple entities (song, album, artist, playlist, station) to `MediaRef`.
//! - Attaches typed `PageActionWire` actions (`Play`).
//! - Sets `open_route` for navigational drill-down (`PageRoute`).
//! - Normalizes artwork, durations, badges, and presentation hints.

use malus_ipc::wire::{PageActionWire, PageBadgeWire, PageItemWire, PageSectionWire};
use malus_model::{MediaRef, PageRoute};
use serde_json::Value;

use crate::api::parse::parse_apple_artwork;

/// Reuse resource identity and capabilities for detail headers as well as cards.
pub(super) fn map_detail_actions(resource: &Value) -> Vec<PageActionWire> {
    let Some(item) = map_apple_resource_to_item(resource) else {
        return Vec::new();
    };
    let mut actions = item.actions;
    if let Some(reference @ MediaRef::Artist(_)) = item.entity {
        actions = vec![
            PageActionWire::Play(reference.clone()),
            PageActionWire::Favorite(reference),
        ];
    }
    let favorite = resource
        .pointer("/attributes/isFavorite")
        .and_then(Value::as_bool);
    if favorite == Some(true) {
        for action in &mut actions {
            if let PageActionWire::Favorite(reference) = action {
                *action = PageActionWire::Unfavorite(reference.clone());
            }
        }
    }
    actions
}

/// Map a single Apple Music resource JSON object into a PageItemWire.
pub fn map_apple_resource_to_item(item: &Value) -> Option<PageItemWire> {
    let typ = item.get("type").and_then(|t| t.as_str())?;
    let id = item.get("id").and_then(|i| i.as_str())?;
    let attrs = item.get("attributes").unwrap_or(item);

    let catalog_item = item
        .get("relationships")
        .and_then(|r| r.get("catalog"))
        .and_then(|c| c.get("data"))
        .and_then(|d| d.as_array())
        .and_then(|a| a.first());
    let catalog_id = catalog_item
        .and_then(|e| e.get("id"))
        .and_then(|i| i.as_str());

    match typ {
        "songs" | "library-songs" => {
            let title = attrs.get("name").and_then(|n| n.as_str()).unwrap_or("Song");
            let mut page_item = PageItemWire::new(id, title);
            page_item.subtitle = attrs
                .get("artistName")
                .and_then(|a| a.as_str())
                .map(str::to_string);
            page_item.tertiary_text = attrs
                .get("albumName")
                .and_then(|a| a.as_str())
                .map(str::to_string);
            page_item.artwork = attrs.get("artwork").and_then(parse_apple_artwork);
            let play_id = catalog_id.unwrap_or(id);
            let mref = MediaRef::Song(play_id.to_string());
            page_item.entity = Some(mref.clone());

            let is_library = typ.starts_with("library-")
                || id.starts_with("i.")
                || attrs
                    .get("inLibrary")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
            page_item.in_library = Some(is_library);

            let is_fav = attrs
                .get("personalRating")
                .and_then(|r| r.as_i64())
                .map(|r| r == 1)
                .or_else(|| attrs.get("inFavorites").and_then(|b| b.as_bool()))
                .unwrap_or(false);
            page_item.is_favorite = Some(is_fav);

            let mut actions = vec![
                PageActionWire::Play(mref.clone()),
                PageActionWire::PlayNext(mref.clone()),
                PageActionWire::PlayLater(mref.clone()),
            ];
            if !is_library {
                actions.push(PageActionWire::AddToLibrary(mref.clone()));
            }
            if is_fav {
                actions.push(PageActionWire::Unfavorite(mref.clone()));
            } else {
                actions.push(PageActionWire::Favorite(mref.clone()));
            }
            actions.push(PageActionWire::SuggestLess(mref));
            page_item.actions = actions;
            page_item.presentation_hint = Some("track-row".to_string());

            if let Some(dur) = attrs
                .get("durationInMillis")
                .and_then(|d| d.as_u64().or_else(|| d.as_f64().map(|f| f as u64)))
            {
                page_item.duration_ms = Some(dur);
                let mins = dur / 60_000;
                let secs = (dur % 60_000) / 1000;
                page_item.metadata.push(format!("{mins}:{secs:02}"));
            }

            if attrs.get("contentRating").and_then(|r| r.as_str()) == Some("explicit") {
                page_item
                    .badges
                    .push(PageBadgeWire::with_style("E", "explicit"));
            }

            Some(page_item)
        }

        "albums" | "library-albums" => {
            let title = attrs
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("Album");
            let mut page_item = PageItemWire::new(id, title);
            page_item.subtitle = attrs
                .get("artistName")
                .and_then(|a| a.as_str())
                .map(str::to_string);
            page_item.tertiary_text = attrs
                .get("releaseDate")
                .and_then(|r| r.as_str())
                .map(|r| r.split('-').next().unwrap_or(r).to_string());
            page_item.artwork = attrs.get("artwork").and_then(parse_apple_artwork);
            let open_id = catalog_id.unwrap_or(id);
            let mref = MediaRef::Album(open_id.to_string());
            page_item.entity = Some(mref.clone());
            page_item.open_route = Some(PageRoute::Album(open_id.to_string()));

            let is_library = typ.starts_with("library-")
                || id.starts_with("l.")
                || attrs
                    .get("inLibrary")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
            page_item.in_library = Some(is_library);

            let mut actions = vec![
                PageActionWire::Play(mref.clone()),
                PageActionWire::PlayNext(mref.clone()),
                PageActionWire::PlayLater(mref.clone()),
            ];
            if !is_library {
                actions.push(PageActionWire::AddToLibrary(mref.clone()));
            }
            actions.push(PageActionWire::Favorite(mref));
            page_item.actions = actions;
            page_item.presentation_hint = Some("card".to_string());

            if attrs.get("contentRating").and_then(|r| r.as_str()) == Some("explicit") {
                page_item
                    .badges
                    .push(PageBadgeWire::with_style("E", "explicit"));
            }

            Some(page_item)
        }

        "artists" | "library-artists" => {
            let name = attrs
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("Artist");
            let mut page_item = PageItemWire::new(id, name);

            let catalog_art = catalog_item
                .and_then(|c| c.get("attributes"))
                .and_then(|a| a.get("artwork"));
            let album_art = item
                .get("relationships")
                .and_then(|r| r.get("albums"))
                .and_then(|a| a.get("data"))
                .and_then(|d| d.as_array())
                .and_then(|arr| arr.first())
                .and_then(|alb| alb.get("attributes"))
                .and_then(|a| a.get("artwork"));

            page_item.artwork = attrs
                .get("artwork")
                .or(catalog_art)
                .or(album_art)
                .and_then(parse_apple_artwork);

            let is_library = typ == "library-artists" || id.starts_with("r.");
            let open_id = if is_library {
                id
            } else {
                catalog_id.unwrap_or(id)
            };
            page_item.entity = Some(MediaRef::Artist(catalog_id.unwrap_or(id).to_string()));
            page_item.open_route = Some(PageRoute::Artist(open_id.to_string()));
            page_item.presentation_hint = Some("circle-card".to_string());

            Some(page_item)
        }

        "playlists" | "library-playlists" => {
            let title = attrs
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("Playlist");
            let mut page_item = PageItemWire::new(id, title);
            page_item.subtitle = attrs
                .get("curatorName")
                .and_then(|c| c.as_str())
                .map(str::to_string);
            page_item.artwork = attrs.get("artwork").and_then(parse_apple_artwork);
            let open_id = catalog_id.unwrap_or(id);
            let mref = MediaRef::Playlist(open_id.to_string());
            page_item.entity = Some(mref.clone());
            page_item.open_route = Some(PageRoute::Playlist(open_id.to_string()));

            let is_library = typ.starts_with("library-")
                || id.starts_with("p.")
                || attrs
                    .get("inLibrary")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
            page_item.in_library = Some(is_library);
            page_item.can_edit = attrs["canEdit"].as_bool().unwrap_or(false);
            page_item.can_delete = attrs["canDelete"].as_bool().unwrap_or(false);

            let mut actions = vec![
                PageActionWire::Play(mref.clone()),
                PageActionWire::PlayNext(mref.clone()),
                PageActionWire::PlayLater(mref.clone()),
            ];
            if !is_library {
                actions.push(PageActionWire::AddToLibrary(mref.clone()));
            }
            actions.push(PageActionWire::Favorite(mref));
            page_item.actions = actions;
            page_item.presentation_hint = Some("card".to_string());

            Some(page_item)
        }

        "stations" => {
            let title = attrs
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("Station");
            let mut page_item = PageItemWire::new(id, title);
            page_item.subtitle = attrs
                .get("stationProviderName")
                .and_then(|p| p.as_str())
                .map(str::to_string);
            page_item.artwork = attrs.get("artwork").and_then(parse_apple_artwork);
            let mref = MediaRef::Station(id.to_string());
            page_item.entity = Some(mref.clone());
            page_item.actions = vec![PageActionWire::Play(mref)];
            page_item.presentation_hint = Some("card".to_string());

            if attrs
                .get("isLive")
                .and_then(|l| l.as_bool())
                .unwrap_or(false)
            {
                page_item
                    .badges
                    .push(PageBadgeWire::with_style("LIVE", "live"));
            }

            Some(page_item)
        }

        _ => None,
    }
}

/// Map a `data` array of Apple resources into a vector of `PageItemWire`.
pub fn map_apple_data_to_items(resp: &Value) -> Vec<PageItemWire> {
    if let Some(arr) = resp.get("data").and_then(|d| d.as_array()) {
        arr.iter().filter_map(map_apple_resource_to_item).collect()
    } else {
        Vec::new()
    }
}

/// Map Apple recommendations response (`/v1/me/recommendations`) into structured shelves.
pub fn map_recommendations_to_sections(resp: &Value) -> Vec<PageSectionWire> {
    let mut sections = Vec::new();

    if let Some(groups) = resp.get("data").and_then(|d| d.as_array()) {
        for group in groups {
            let group_id = group.get("id").and_then(|i| i.as_str()).unwrap_or("group");
            let attrs = group.get("attributes");
            let title = attrs
                .and_then(|a| a.get("title"))
                .and_then(|t| t.get("stringForDisplay").or_else(|| t.get("value")))
                .and_then(|s| s.as_str())
                .unwrap_or("Featured");
            let subtitle = attrs
                .and_then(|a| a.get("reason"))
                .and_then(|r| r.get("stringForDisplay").or_else(|| r.get("value")))
                .and_then(|s| s.as_str())
                .map(str::to_string);

            let items = if let Some(contents) = group
                .get("relationships")
                .and_then(|r| r.get("contents"))
                .and_then(|c| c.get("data"))
                .and_then(|d| d.as_array())
            {
                contents
                    .iter()
                    .filter_map(map_apple_resource_to_item)
                    .collect()
            } else {
                Vec::new()
            };

            if !items.is_empty() {
                let mut section = PageSectionWire::new(group_id, Some(title.to_string()), items)
                    .with_hint("shelf");
                section.subtitle = subtitle;
                sections.push(section);
            }
        }
    }

    sections
}

/// Map an Apple Music Replay period summary item into a PageItemWire.
///
/// Handles `album-period-summaries`, `artist-period-summaries`, and `song-period-summaries`.
pub fn map_period_summary_to_item(item: &Value) -> Option<PageItemWire> {
    // Check relationships for the catalog entity
    if let Some(rels) = item.get("relationships").and_then(|r| r.as_object()) {
        for (_rel_key, rel_val) in rels {
            if let Some(target) = rel_val
                .get("data")
                .and_then(|d| d.as_array())
                .and_then(|a| a.first())
                && let Some(mut page_item) = map_apple_resource_to_item(target)
            {
                // Enrich with play count from period summary attributes if available
                if let Some(play_count) = item
                    .get("attributes")
                    .and_then(|a| a.get("playCount").or_else(|| a.get("listenCount")))
                    .and_then(|c| c.as_u64())
                {
                    page_item.metadata.push(format!("{play_count} plays"));
                }
                return Some(page_item);
            }
        }
    }

    // Fallback if relationships are not expanded
    map_apple_resource_to_item(item)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_map_song_resource() {
        let song = json!({
            "type": "songs",
            "id": "1440857781",
            "attributes": {
                "name": "Get Lucky",
                "artistName": "Daft Punk",
                "albumName": "Random Access Memories",
                "durationInMillis": 369626,
                "contentRating": "explicit",
                "artwork": {
                    "url": "https://example.com/{w}x{h}.jpg",
                    "width": 1000,
                    "height": 1000
                }
            }
        });

        let item = map_apple_resource_to_item(&song).expect("mapped song");
        assert_eq!(item.id, "1440857781");
        assert_eq!(item.title, "Get Lucky");
        assert_eq!(item.subtitle.as_deref(), Some("Daft Punk"));
        assert_eq!(
            item.tertiary_text.as_deref(),
            Some("Random Access Memories")
        );
        assert_eq!(item.entity, Some(MediaRef::Song("1440857781".to_string())));
        assert_eq!(
            item.actions,
            vec![
                PageActionWire::Play(MediaRef::Song("1440857781".to_string())),
                PageActionWire::PlayNext(MediaRef::Song("1440857781".to_string())),
                PageActionWire::PlayLater(MediaRef::Song("1440857781".to_string())),
                PageActionWire::AddToLibrary(MediaRef::Song("1440857781".to_string())),
                PageActionWire::Favorite(MediaRef::Song("1440857781".to_string())),
                PageActionWire::SuggestLess(MediaRef::Song("1440857781".to_string())),
            ]
        );
        assert_eq!(item.badges.len(), 1);
        assert_eq!(item.badges[0].label, "E");
        assert_eq!(item.metadata, vec!["6:09"]);
    }

    #[test]
    fn test_map_album_resource() {
        let album = json!({
            "type": "albums",
            "id": "1440857780",
            "attributes": {
                "name": "Random Access Memories",
                "artistName": "Daft Punk",
                "releaseDate": "2013-05-17",
                "artwork": {
                    "url": "https://example.com/{w}x{h}.jpg",
                    "width": 1000,
                    "height": 1000
                }
            }
        });

        let item = map_apple_resource_to_item(&album).expect("mapped album");
        assert_eq!(item.id, "1440857780");
        assert_eq!(item.title, "Random Access Memories");
        assert_eq!(item.entity, Some(MediaRef::Album("1440857780".to_string())));
        assert_eq!(
            item.open_route,
            Some(PageRoute::Album("1440857780".to_string()))
        );
        assert_eq!(
            item.actions,
            vec![
                PageActionWire::Play(MediaRef::Album("1440857780".to_string())),
                PageActionWire::PlayNext(MediaRef::Album("1440857780".to_string())),
                PageActionWire::PlayLater(MediaRef::Album("1440857780".to_string())),
                PageActionWire::AddToLibrary(MediaRef::Album("1440857780".to_string())),
                PageActionWire::Favorite(MediaRef::Album("1440857780".to_string())),
            ]
        );
    }
}

#[cfg(test)]
mod detail_action_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn catalog_and_library_headers_keep_real_resource_actions() {
        let catalog =
            json!({"id":"123","type":"albums","attributes":{"name":"Album","isFavorite":true}});
        let actions = map_detail_actions(&catalog);
        assert!(actions.contains(&PageActionWire::Play(MediaRef::Album("123".into()))));
        assert!(actions.contains(&PageActionWire::AddToLibrary(MediaRef::Album("123".into()))));
        assert!(actions.contains(&PageActionWire::Unfavorite(MediaRef::Album("123".into()))));
        let library = json!({"id":"p.real-library-id","type":"library-playlists","attributes":{"name":"Playlist"}});
        let actions = map_detail_actions(&library);
        assert!(actions.contains(&PageActionWire::Play(MediaRef::Playlist(
            "p.real-library-id".into()
        ))));
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, PageActionWire::AddToLibrary(_)))
        );
        assert!(map_detail_actions(&json!({"attributes":{"name":"No identity"}})).is_empty());
    }
}
