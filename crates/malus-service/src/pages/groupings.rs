//! Apple Editorial Groupings parser and resolver.
//!
//! Resolves `/v1/editorial/{storefront}/groupings?name={name}&tabs=subscriber&format[resources]=map`
//! for canonical Radio and Browse (Music) feeds.
//!
//! Structure:
//! `groupings[id]` -> `tabs` -> `children` (editorial elements)
//! Each child editorial element contains either direct `contents` or nested `children`,
//! referencing resources mapped in `resources[type][id]`.

use malus_ipc::wire::{PageActionWire, PageBadgeWire, PageItemWire, PageSectionWire};
use malus_model::MediaRef;
use serde_json::Value;

use crate::api::parse::parse_apple_artwork;
use crate::pages::mapper;

/// Canonical grouping endpoint URL template.
pub const CANONICAL_GROUPING_URL: &str =
    "https://amp-api-edge.music.apple.com/v1/editorial/{storefront}/groupings";

/// Query parameters for canonical editorial groupings.
pub fn build_grouping_query<'a>(name: &'a str, locale: &'a str) -> Vec<(&'a str, &'a str)> {
    vec![
        ("name", name),
        ("tabs", "subscriber"),
        ("format[resources]", "map"),
        ("platform", "web"),
        ("l", locale),
        ("art[url]", "c,f"),
        ("extend", "artistUrl,editorialArtwork,plainEditorialNotes"),
        ("extend[station-events]", "editorialVideo"),
        (
            "fields[albums]",
            "artistName,artistUrl,artwork,contentRating,editorialArtwork,plainEditorialNotes,name,playParams,releaseDate,url,trackCount",
        ),
        (
            "fields[artists]",
            "name,url,artwork,editorialArtwork,genreNames,plainEditorialNotes",
        ),
        ("include[albums]", "artists"),
        ("include[music-videos]", "artists"),
        ("include[songs]", "artists"),
        ("include[stations]", "events,radio-show"),
        ("omit[resource:artists]", "autos"),
        ("relate[songs]", "albums"),
    ]
}

/// Map editorial grouping JSON response into an ordered list of PageSectionWire.
pub fn map_groupings_response(resp: &Value, page_kind: &str) -> Vec<PageSectionWire> {
    let Some(res) = resp.get("resources").and_then(|r| r.as_object()) else {
        return Vec::new();
    };

    let groupings = res.get("groupings").and_then(|g| g.as_object());
    let elements = res.get("editorial-elements").and_then(|e| e.as_object());

    let Some(elements) = elements else {
        return Vec::new();
    };

    // Find root grouping
    let root_gid = resp
        .get("data")
        .and_then(|d| d.as_array())
        .and_then(|a| a.first())
        .and_then(|g| g.get("id"))
        .and_then(|i| i.as_str());

    let grouping = root_gid
        .and_then(|gid| groupings.and_then(|g| g.get(gid)))
        .or_else(|| groupings.and_then(|g| g.values().next()));

    // Find tab elements
    let tab_id = grouping
        .and_then(|g| g.get("relationships"))
        .and_then(|r| r.get("tabs"))
        .and_then(|t| t.get("data"))
        .and_then(|d| d.as_array())
        .and_then(|a| a.first())
        .and_then(|item| item.get("id"))
        .and_then(|i| i.as_str())
        .unwrap_or("default");

    let tab_elem = elements.get(tab_id);
    let child_refs = tab_elem
        .and_then(|t| t.get("relationships"))
        .and_then(|r| r.get("children"))
        .and_then(|c| c.get("data"))
        .and_then(|d| d.as_array());

    let Some(child_refs) = child_refs else {
        return Vec::new();
    };

    let mut sections = Vec::new();

    for (idx, c_ref) in child_refs.iter().enumerate() {
        let Some(cid) = c_ref.get("id").and_then(|i| i.as_str()) else {
            continue;
        };
        let Some(celem) = elements.get(cid) else {
            continue;
        };

        let cattrs = celem.get("attributes");
        let raw_title = cattrs
            .and_then(|a| a.get("name").and_then(|n| n.as_str()))
            .or_else(|| cattrs.and_then(|a| a.get("title").and_then(|t| t.as_str())))
            .or_else(|| {
                cattrs
                    .and_then(|a| a.get("plainEditorialNotes"))
                    .and_then(|p| p.get("name"))
                    .and_then(|n| n.as_str())
            });

        // Resolve items
        let mut raw_items = Vec::new();

        // 1. Direct contents
        if let Some(contents) = celem
            .get("relationships")
            .and_then(|r| r.get("contents"))
            .and_then(|c| c.get("data"))
            .and_then(|d| d.as_array())
        {
            for it in contents {
                if let (Some(itype), Some(iid)) = (
                    it.get("type").and_then(|t| t.as_str()),
                    it.get("id").and_then(|i| i.as_str()),
                ) && let Some(res_map) = res.get(itype).and_then(|m| m.as_object())
                    && let Some(item_json) = res_map.get(iid)
                {
                    raw_items.push(item_json);
                }
            }
        } else if let Some(nested_children) = celem
            .get("relationships")
            .and_then(|r| r.get("children"))
            .and_then(|c| c.get("data"))
            .and_then(|d| d.as_array())
        {
            // 2. Nested child elements
            for n_ref in nested_children {
                if let Some(nid) = n_ref.get("id").and_then(|i| i.as_str())
                    && let Some(nelem) = elements.get(nid)
                    && let Some(ncontents) = nelem
                        .get("relationships")
                        .and_then(|r| r.get("contents"))
                        .and_then(|c| c.get("data"))
                        .and_then(|d| d.as_array())
                {
                    for it in ncontents {
                        if let (Some(itype), Some(iid)) = (
                            it.get("type").and_then(|t| t.as_str()),
                            it.get("id").and_then(|i| i.as_str()),
                        ) && let Some(res_map) = res.get(itype).and_then(|m| m.as_object())
                            && let Some(item_json) = res_map.get(iid)
                        {
                            raw_items.push(item_json);
                        }
                    }
                }
            }
        }

        if raw_items.is_empty() {
            if let Some(links) = cattrs
                .and_then(|a| a.get("links"))
                .and_then(|l| l.as_array())
            {
                let mut pill_items = Vec::new();
                for link in links {
                    if let Some(label) = link.get("label").and_then(|l| l.as_str()) {
                        let mut item =
                            PageItemWire::new(format!("explore:{}", label.to_lowercase()), label);
                        item.presentation_hint = Some("pill".to_string());
                        item.entity =
                            Some(MediaRef::Song(format!("explore:{}", label.to_lowercase())));
                        pill_items.push(item);
                    }
                }
                if !pill_items.is_empty() {
                    let title = raw_title.unwrap_or("More to Explore");
                    let section_id = format!("grouping:{cid}");
                    let section =
                        PageSectionWire::new(section_id, Some(title.to_string()), pill_items)
                            .with_hint("explore-pills");
                    sections.push(section);
                }
            }
            continue;
        }

        let is_on_air = raw_title == Some("On Air Now") || raw_title == Some("On Air");
        let mut items: Vec<PageItemWire> = if is_on_air {
            raw_items
                .iter()
                .filter_map(|item_json| {
                    let sid = item_json.get("id").and_then(|i| i.as_str())?;
                    let sattrs = item_json.get("attributes").unwrap_or(item_json);
                    let sname = sattrs
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("Apple Music Radio");

                    // Check for current station-event
                    let ev_id = item_json
                        .get("relationships")
                        .and_then(|r| r.get("events"))
                        .and_then(|e| e.get("data"))
                        .and_then(|d| d.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|first| first.get("id"))
                        .and_then(|i| i.as_str());

                    let ev_json = ev_id.and_then(|eid| {
                        res.get("station-events")
                            .and_then(|se| se.as_object())
                            .and_then(|m| m.get(eid))
                    });

                    let (show_title, show_desc, show_art) = if let Some(ev) = ev_json {
                        let eattrs = ev.get("attributes").unwrap_or(ev);
                        let title = eattrs.get("title").and_then(|t| t.as_str());
                        let desc = eattrs
                            .get("description")
                            .and_then(|d| d.get("standard").or_else(|| d.get("short")))
                            .and_then(|s| s.as_str());
                        let art = eattrs.get("heroArtwork").or_else(|| eattrs.get("artwork"));
                        (title, desc, art)
                    } else {
                        (None, None, None)
                    };

                    let title = show_title.unwrap_or(sname);
                    let mut page_item = PageItemWire::new(sid, title);
                    page_item.tertiary_text = Some(sname.to_uppercase());
                    page_item.subtitle = show_desc.map(str::to_string).or_else(|| {
                        sattrs
                            .get("plainEditorialNotes")
                            .and_then(|p| p.get("short").or_else(|| p.get("tagline")))
                            .and_then(|s| s.as_str())
                            .map(str::to_string)
                    });
                    page_item.artwork = show_art
                        .or_else(|| sattrs.get("artwork"))
                        .and_then(parse_apple_artwork);
                    let mref = MediaRef::Station(sid.to_string());
                    page_item.entity = Some(mref.clone());
                    page_item.actions = vec![PageActionWire::Play(mref)];
                    page_item
                        .badges
                        .push(PageBadgeWire::with_style("LIVE", "live"));
                    page_item.presentation_hint = Some("card".to_string());
                    Some(page_item)
                })
                .collect()
        } else {
            raw_items
                .iter()
                .copied()
                .filter_map(mapper::map_apple_resource_to_item)
                .collect()
        };

        if items.is_empty() {
            continue;
        }

        // For Browse Hero Carousel, prefer wide landscape editorial artwork
        if idx == 0 && page_kind == "music" {
            for (item_json, item_wire) in raw_items.iter().zip(items.iter_mut()) {
                let sattrs = item_json.get("attributes").unwrap_or(item_json);
                if let Some(ed_art) = sattrs.get("editorialArtwork") {
                    let wide_art = ed_art
                        .get("superHeroWide")
                        .or_else(|| ed_art.get("subscriptionHero"))
                        .or_else(|| ed_art.get("storeFlowcase"));
                    if let Some(parsed) = wide_art.and_then(parse_apple_artwork) {
                        item_wire.artwork = Some(parsed);
                    }
                }
                if item_wire.tertiary_text.is_none() {
                    let kind_str = match item_wire.entity {
                        Some(MediaRef::Playlist(_)) => "FEATURED PLAYLIST",
                        Some(MediaRef::Album(_)) => "NEW RELEASE",
                        _ => "FEATURED",
                    };
                    item_wire.tertiary_text = Some(kind_str.to_string());
                }
            }
        }

        let title = match raw_title {
            Some(t) if !t.trim().is_empty() => t.trim().to_string(),
            _ => {
                if idx == 0 {
                    if page_kind == "radio" {
                        "Live Radio".to_string()
                    } else {
                        "Featured".to_string()
                    }
                } else {
                    format!("Shelf {idx}")
                }
            }
        };

        // Determine presentation hint based on page context, position, and item types
        let is_hero = idx == 0;
        let hint = if is_hero {
            if page_kind == "radio" {
                "live-stations-shelf"
            } else {
                "featured-banner-shelf"
            }
        } else {
            let all_stations = items
                .iter()
                .all(|it| matches!(it.entity, Some(MediaRef::Station(_))));
            let all_artists = items
                .iter()
                .all(|it| matches!(it.entity, Some(MediaRef::Artist(_))));
            let all_songs = items
                .iter()
                .all(|it| matches!(it.entity, Some(MediaRef::Song(_))));

            let has_gradients = items.iter().any(|it| it.bg_color.is_some());
            let is_episodes = title == "Latest Radio Episodes"
                || title == "New Radio Episodes"
                || title.contains("Episodes");

            if all_stations
                && (has_gradients || title == "Top Stations" || title == "Stations by Genre")
            {
                "gradient-stations-shelf"
            } else if all_stations && is_episodes {
                "multi-row-episode-shelf"
            } else if all_stations {
                "stations-shelf"
            } else if all_artists {
                "artist-shelf"
            } else if all_songs && items.len() >= 4 {
                "multi-row-track-shelf"
            } else if all_songs {
                "track-list"
            } else {
                "shelf"
            }
        };

        let section_id = format!("grouping:{cid}");
        let section = PageSectionWire::new(section_id, Some(title), items).with_hint(hint);
        sections.push(section);
    }

    sections
}
