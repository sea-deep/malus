//! Canonical Apple Music Listen Now (Home) feed adapter.
//!
//! Note on endpoint status:
//! The underlying endpoint (`https://amp-api.music.apple.com/v1/me/recommendations`)
//! is an Apple web-client / private compatibility surface proven through live browser
//! network capture. Its query parameters (`displayFilter[kind]`, `platform=web`,
//! `timezone`, `format[resources]=map`), response schemas, and `display.kind`
//! descriptors are Apple client-private contracts, not part of the public Apple Music API.

use malus_ipc::wire::{PageItemWire, PageSectionWire};
use malus_model::MediaRef;
use serde_json::Value;
use tracing::{debug, warn};

use super::mapper::map_apple_resource_to_item;

pub const CANONICAL_LISTEN_NOW_URL: &str = "https://amp-api.music.apple.com/v1/me/recommendations";

/// Private Apple display kinds translated into a typed internal enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppleDisplayKind {
    CoverShelf,
    CircleCoverShelf,
    NotesHeroShelf,
    SuperHeroShelf,
    CoverGrid,
    SocialCardShelf,
    ConcertsEmptyShelf,
    Unknown,
}

impl AppleDisplayKind {
    pub fn parse(s: &str) -> Self {
        match s {
            "MusicCoverShelf" => Self::CoverShelf,
            "MusicCircleCoverShelf" => Self::CircleCoverShelf,
            "MusicNotesHeroShelf" => Self::NotesHeroShelf,
            "MusicSuperHeroShelf" => Self::SuperHeroShelf,
            "MusicCoverGrid" => Self::CoverGrid,
            "MusicSocialCardShelf" => Self::SocialCardShelf,
            "MusicConcertsEmptyShelf" => Self::ConcertsEmptyShelf,
            other => {
                debug!(
                    "Encountered unknown Apple display kind: '{other}', falling back to generic shelf"
                );
                Self::Unknown
            }
        }
    }
}

/// Dynamically determine the local host system's UTC offset in `+HH:MM` or `-HH:MM` format.
///
/// Uses standard POSIX `localtime_r` to ensure DST transitions are respected without
/// introducing additional external runtime dependencies.
pub fn get_local_timezone_offset() -> String {
    #[cfg(unix)]
    unsafe {
        #[repr(C)]
        struct Tm {
            tm_sec: std::ffi::c_int,
            tm_min: std::ffi::c_int,
            tm_hour: std::ffi::c_int,
            tm_mday: std::ffi::c_int,
            tm_mon: std::ffi::c_int,
            tm_year: std::ffi::c_int,
            tm_wday: std::ffi::c_int,
            tm_yday: std::ffi::c_int,
            tm_isdst: std::ffi::c_int,
            tm_gmtoff: std::ffi::c_long,
            tm_zone: *const std::ffi::c_char,
        }
        unsafe extern "C" {
            fn localtime_r(timep: *const i64, result: *mut Tm) -> *mut Tm;
        }
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let mut tm: Tm = std::mem::zeroed();
        if !localtime_r(&t, &mut tm).is_null() {
            let offset_secs = tm.tm_gmtoff;
            let sign = if offset_secs >= 0 { '+' } else { '-' };
            let abs_secs = offset_secs.abs();
            let hours = abs_secs / 3600;
            let mins = (abs_secs % 3600) / 60;
            return format!("{sign}{hours:02}:{mins:02}");
        }
    }
    "+00:00".to_string()
}

/// Derive the appropriate Apple Music language tag (`l`) from the storefront or environment.
pub fn get_locale_for_storefront(storefront: &str) -> String {
    if let Ok(loc) = std::env::var("APPLE_MUSIC_LOCALE") {
        let trimmed = loc.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    match storefront.to_lowercase().as_str() {
        "us" => "en-US".to_string(),
        "gb" | "uk" | "in" | "au" | "nz" | "ie" | "za" => "en-GB".to_string(),
        "ca" => "en-CA".to_string(),
        "fr" => "fr-FR".to_string(),
        "de" => "de-DE".to_string(),
        "jp" => "ja-JP".to_string(),
        "es" => "es-ES".to_string(),
        "it" => "it-IT".to_string(),
        _ => {
            if let Ok(lang) = std::env::var("LANG")
                && let Some(locale) = lang.split('.').next()
            {
                let apple_loc = locale.replace('_', "-");
                if apple_loc.contains('-') {
                    return apple_loc;
                }
            }
            "en-US".to_string()
        }
    }
}

/// Centralized query parameter builder for the canonical Listen Now request.
///
/// Contains the exact parameters captured from the official Apple Music client.
pub fn build_listen_now_query(timezone: &str, locale: &str) -> Vec<(&'static str, String)> {
    vec![
        ("name", "listen-now".to_string()),
        (
            "displayFilter[kind]",
            "MusicCircleCoverShelf,MusicConcertsEmptyShelf,MusicCoverGrid,MusicCoverShelf,MusicNotesHeroShelf,MusicSocialCardShelf,MusicSuperHeroShelf".to_string(),
        ),
        (
            "extend",
            "editorialArtwork,editorialVideo,plainEditorialCard,plainEditorialNotes".to_string(),
        ),
        ("extend[playlists]", "artistNames".to_string()),
        (
            "extend[stations]",
            "airTime,supportsAirTimeUpdates".to_string(),
        ),
        ("fields[artists]", "name,artwork,url".to_string()),
        ("format[resources]", "map".to_string()),
        ("include[albums]", "artists".to_string()),
        ("include[library-playlists]", "catalog".to_string()),
        (
            "include[personal-recommendation]",
            "primary-content".to_string(),
        ),
        ("include[stations]", "radio-show".to_string()),
        ("meta[stations]", "inflectionPoints".to_string()),
        (
            "types",
            "activities,albums,apple-curators,artists,concerts,curators,editorial-items,library-albums,library-playlists,music-movies,music-videos,playlists,social-profiles,social-upsells,songs,stations,tv-episodes,tv-shows,uploaded-audios,uploaded-videos".to_string(),
        ),
        ("with", "friendsMix,library,social".to_string()),
        ("l", locale.to_string()),
        ("platform", "web".to_string()),
        ("timezone", timezone.to_string()),
    ]
}

/// Predicate identifying the Apple Music Replay shelf for exclusion in this milestone.
///
/// Runtime evidence from live network trace:
/// - Contained playlist IDs follow the `pl.rp-<hash>` prefix pattern ("Replay Playlist").
/// - Contained editorial items reference the external target `https://replay.music.apple.com`.
/// - Apple's branded shelf title starts with or contains "Replay".
///
/// This check is strictly isolated here as a temporary product exclusion for this milestone
/// and is never used for general shelf classification.
pub fn is_replay_home_shelf(group: &Value, resources: &Value) -> bool {
    // 1. Check if any contained item has a Replay playlist ID (pl.rp-*) or Replay editorial link
    if let Some(contents) = group
        .get("relationships")
        .and_then(|r| r.get("contents"))
        .and_then(|c| c.get("data"))
        .and_then(|d| d.as_array())
    {
        for item_ref in contents {
            if let Some(id) = item_ref.get("id").and_then(|i| i.as_str())
                && id.starts_with("pl.rp-")
            {
                return true;
            }
            if let Some(typ) = item_ref.get("type").and_then(|t| t.as_str())
                && typ == "editorial-items"
                && let Some(id) = item_ref.get("id").and_then(|i| i.as_str())
                && let Some(ed_item) = resources.get("editorial-items").and_then(|e| e.get(id))
                && let Some(url) = ed_item
                    .get("attributes")
                    .and_then(|a| {
                        a.get("url")
                            .or_else(|| a.get("link").and_then(|l| l.get("url")))
                    })
                    .and_then(|u| u.as_str())
                && url.contains("replay.music.apple.com")
            {
                return true;
            }
        }
    }

    // 2. Apple branded display title fallback
    if let Some(title) = group
        .get("attributes")
        .and_then(|a| a.get("title"))
        .and_then(|t| t.get("stringForDisplay").or_else(|| t.get("value")))
        .and_then(|s| s.as_str())
    {
        let lower = title.to_lowercase();
        if lower.starts_with("replay") || lower.contains("replay:") {
            return true;
        }
    }

    false
}

/// Map the canonical Apple Listen Now response into ordered `PageSectionWire`s.
///
/// Follows Apple's exact `data[]` shelf ordering and dereferences items through `resources[type][id]`.
pub fn map_listen_now_response(resp: &Value) -> Vec<PageSectionWire> {
    let mut sections = Vec::new();

    let empty_map = serde_json::Map::new();
    let resources = resp
        .get("resources")
        .and_then(|r| r.as_object())
        .unwrap_or(&empty_map);
    let p_recs = resources
        .get("personal-recommendation")
        .and_then(|pr| pr.as_object());

    if let Some(data_arr) = resp.get("data").and_then(|d| d.as_array()) {
        for group_ref in data_arr {
            let gid = group_ref
                .get("id")
                .and_then(|i| i.as_str())
                .unwrap_or_default();
            let group = p_recs.and_then(|pr| pr.get(gid)).unwrap_or(group_ref);

            // 1. Check Replay product exclusion
            if is_replay_home_shelf(group, resp.get("resources").unwrap_or(&Value::Null)) {
                debug!("Omitting Replay shelf '{gid}' per milestone exclusion");
                continue;
            }

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

            let raw_display_kind = attrs
                .and_then(|a| a.get("display"))
                .and_then(|d| d.get("kind"))
                .and_then(|k| k.as_str())
                .unwrap_or("MusicCoverShelf");

            let display_kind = AppleDisplayKind::parse(raw_display_kind);

            let rtypes = attrs
                .and_then(|a| a.get("resourceTypes"))
                .and_then(|rt| rt.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<&str>>())
                .unwrap_or_default();

            let contents = group
                .get("relationships")
                .and_then(|r| r.get("contents"))
                .and_then(|c| c.get("data"))
                .and_then(|d| d.as_array());

            let mut items: Vec<PageItemWire> = Vec::new();
            if let Some(contents) = contents {
                for item_ref in contents {
                    let ctype = item_ref
                        .get("type")
                        .and_then(|t| t.as_str())
                        .unwrap_or_default();
                    let cid = item_ref
                        .get("id")
                        .and_then(|i| i.as_str())
                        .unwrap_or_default();

                    let resource = match resources.get(ctype).and_then(|type_map| type_map.get(cid))
                    {
                        Some(res) => res,
                        None => {
                            if item_ref.get("attributes").is_some() {
                                item_ref
                            } else {
                                debug!(
                                    "Missing referenced resource {ctype}:{cid} in resources map; skipping"
                                );
                                continue;
                            }
                        }
                    };

                    if let Some(mut page_item) = map_apple_resource_to_item(resource) {
                        // For hero feature shelves, attach plain editorial copy to tertiary_text
                        if matches!(
                            display_kind,
                            AppleDisplayKind::NotesHeroShelf | AppleDisplayKind::SuperHeroShelf
                        ) && page_item.tertiary_text.is_none()
                            && let Some(editorial) = resource
                                .get("attributes")
                                .and_then(|a| {
                                    a.get("plainEditorialNotes")
                                        .or_else(|| a.get("editorialNotes"))
                                })
                                .and_then(|e| e.get("standard").or_else(|| e.get("short")))
                                .and_then(|s| s.as_str())
                        {
                            page_item.tertiary_text = Some(editorial.to_string());
                        }
                        items.push(page_item);
                    }
                }
            }

            // Omit empty concert shelves or empty shelves
            if display_kind == AppleDisplayKind::ConcertsEmptyShelf && items.is_empty() {
                debug!("Omitting empty concerts shelf '{gid}'");
                continue;
            }

            if items.is_empty() {
                continue;
            }

            // Map internal AppleDisplayKind and resource types to wire presentation hint
            let hint = match display_kind {
                AppleDisplayKind::NotesHeroShelf | AppleDisplayKind::SuperHeroShelf => {
                    "top-picks-shelf"
                }
                AppleDisplayKind::CircleCoverShelf => "artist-shelf",
                AppleDisplayKind::CoverGrid => "grid",
                AppleDisplayKind::CoverShelf
                | AppleDisplayKind::SocialCardShelf
                | AppleDisplayKind::ConcertsEmptyShelf
                | AppleDisplayKind::Unknown => {
                    if rtypes.len() == 1 && rtypes[0] == "stations" {
                        "stations-shelf"
                    } else if rtypes.len() == 1 && rtypes[0] == "artists" {
                        "artist-shelf"
                    } else if items
                        .iter()
                        .all(|it| matches!(it.entity, Some(MediaRef::Station(_))))
                    {
                        "stations-shelf"
                    } else if items
                        .iter()
                        .all(|it| matches!(it.entity, Some(MediaRef::Artist(_))))
                    {
                        "artist-shelf"
                    } else {
                        "shelf"
                    }
                }
            };

            let mut section =
                PageSectionWire::new(gid, Some(title.to_string()), items).with_hint(hint);
            section.subtitle = subtitle;
            sections.push(section);
        }
    } else {
        warn!("Listen Now response missing 'data' array");
    }

    sections
}
