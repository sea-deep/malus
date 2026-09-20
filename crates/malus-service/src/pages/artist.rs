//! Authentic Apple Music Artist page implementation.
//!
//! Fetches full catalog views:
//! - top-songs, latest-release, featured-albums, full-albums, singles,
//!   live-albums, compilations, appears-on-albums, top-music-videos,
//!   featured-playlists, similar-artists
//!
//! Extends:
//! - artistBio, editorialArtwork, editorialVideo, bornOrFormed, origin
//!
//! Visual Hierarchy:
//! 1. ARTIST HERO (Banner artwork if available, name, quiet context: genre/origin/formation)
//! 2. TOP SONGS (track-list)
//! 3. LATEST RELEASE (featured shelf)
//! 4. MAJOR DISCOGRAPHY (Essential Albums, Full Albums, Singles & EPs)
//! 5. SECONDARY CATALOG (Live Albums, Compilations, Appears On)
//! 6. CURATED & MEDIA (Top Videos, Artist Playlists)
//! 7. SIMILAR ARTISTS (artist-shelf with circular portraits)
//! 8. BIOGRAPHY (excerpt with Read More in header/bio)

use malus_ipc::wire::{PageActionWire, PageHeaderWire, PageSectionWire, PageWire};
use malus_model::MediaRef;

use crate::{
    api::{OfficialAppleMusicApi, parse::parse_apple_artwork},
    error::AppleError,
    pages::mapper,
};

/// Strip simple HTML tags from Apple Music editorial biography.
fn clean_bio(bio: &str) -> String {
    let mut cleaned = String::with_capacity(bio.len());
    let mut in_tag = false;
    for c in bio.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
        } else if !in_tag {
            cleaned.push(c);
        }
    }
    cleaned
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .trim()
        .to_string()
}

/// Build the full Artist detail page.
pub async fn build_artist_page(
    api: &OfficialAppleMusicApi,
    artist_id: &str,
) -> Result<PageWire, AppleError> {
    if artist_id.starts_with("r.") || artist_id.starts_with("l.") {
        return build_library_artist_page(api, artist_id).await;
    }

    let path =
        format!("https://amp-api.music.apple.com/v1/catalog/{{storefront}}/artists/{artist_id}");
    let views_query = "top-songs,latest-release,featured-albums,full-albums,singles,live-albums,compilations,appears-on-albums,top-music-videos,featured-playlists,similar-artists";
    let extend_query = "artistBio,editorialArtwork,editorialVideo,bornOrFormed,origin";

    let catalog_res = api
        .send_request(
            &path,
            &[
                ("views", views_query),
                ("extend", extend_query),
                ("include", "music-videos,playlists"),
            ],
        )
        .await;

    let data = match catalog_res {
        Ok(d) => d,
        Err(e) => {
            if matches!(e, crate::api::AppleApiError::NotFound(_)) {
                return build_library_artist_page(api, artist_id).await;
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

    // Standard circular avatar
    header.artwork = attrs.get("artwork").and_then(parse_apple_artwork);

    // Wide hero banner artwork
    if let Some(editorial_art) = attrs.get("editorialArtwork") {
        let banner = editorial_art
            .get("centeredFullscreenBackground")
            .or_else(|| editorial_art.get("bannerUber"))
            .or_else(|| editorial_art.get("playlistVIPSquare"))
            .or_else(|| editorial_art.get("subscriptionHero"))
            .or_else(|| editorial_art.get("superHeroTall"))
            .and_then(parse_apple_artwork);
        header.banner_artwork = banner;
    }

    // Artist biography
    if let Some(bio) = attrs.get("artistBio").and_then(|b| b.as_str()) {
        let cleaned = clean_bio(bio);
        if !cleaned.is_empty() {
            header.description = Some(cleaned);
        }
    }

    // Quiet supporting metadata: genres, origin, formation
    if let Some(genres) = attrs.get("genreNames").and_then(|g| g.as_array()) {
        for g in genres.iter().take(2) {
            if let Some(s) = g.as_str() {
                header.metadata.push(s.to_string());
            }
        }
    }
    if let Some(origin) = attrs.get("origin").and_then(|o| o.as_str()) {
        let trimmed = origin.trim();
        if !trimmed.is_empty() {
            header.metadata.push(trimmed.to_string());
        }
    }
    if let Some(born) = attrs.get("bornOrFormed").and_then(|b| b.as_str()) {
        let trimmed = born.trim();
        if !trimmed.is_empty() {
            header.metadata.push(trimmed.to_string());
        }
    }

    page.header = Some(header);

    // Dynamic views ordered by intentional visual rhythm
    if let Some(views) = artist.get("views").and_then(|v| v.as_object()) {
        let ordered_views = [
            ("top-songs", "Top Songs", "multi-row-track-shelf"),
            ("latest-release", "Latest Release", "shelf"),
            ("featured-albums", "Essential Albums", "shelf"),
            ("full-albums", "Albums", "shelf"),
            ("singles", "Singles & EPs", "shelf"),
            ("live-albums", "Live Albums", "shelf"),
            ("compilations", "Compilations", "shelf"),
            ("appears-on-albums", "Appears On", "shelf"),
            ("top-music-videos", "Top Videos", "video-shelf"),
            ("featured-playlists", "Artist Playlists", "shelf"),
            ("similar-artists", "Similar Artists", "artist-shelf"),
        ];

        for (view_key, fallback_title, hint) in ordered_views {
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
                    if items.is_empty() {
                        continue;
                    }

                    let section_id = format!("view:{view_key}");
                    let section =
                        PageSectionWire::new(section_id, Some(view_title.to_string()), items)
                            .with_hint(hint);
                    page.sections.push(section);
                }
            }
        }
    }

    Ok(page)
}

/// Fallback for user library artist: only displays the songs and albums
/// present in the user's library, without dumping the entire catalog discography.
async fn build_library_artist_page(
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
        .and_then(parse_apple_artwork);

    page.header = Some(header);

    // 1. Gather library albums
    let mut album_ids = Vec::new();
    let mut album_items = Vec::new();

    if let Some(albums) = artist
        .get("relationships")
        .and_then(|r| r.get("albums"))
        .and_then(|a| a.get("data"))
        .and_then(|d| d.as_array())
    {
        for alb in albums {
            if let Some(aid) = alb.get("id").and_then(|i| i.as_str()) {
                album_ids.push(aid.to_string());
            }
            if let Some(mut it) = mapper::map_apple_resource_to_item(alb) {
                it.in_library = Some(true);
                it.actions
                    .retain(|a| !matches!(a, PageActionWire::AddToLibrary(_)));
                album_items.push(it);
            }
        }
    }

    if album_ids.is_empty() {
        let albums_path = format!("/v1/me/library/artists/{artist_id}/albums");
        if let Ok(albums_data) = api
            .send_request(&albums_path, &[("limit", "100"), ("include", "catalog")])
            .await
            && let Some(arr) = albums_data.get("data").and_then(|d| d.as_array())
        {
            for alb in arr {
                if let Some(aid) = alb.get("id").and_then(|i| i.as_str()) {
                    album_ids.push(aid.to_string());
                }
                if let Some(mut it) = mapper::map_apple_resource_to_item(alb) {
                    it.in_library = Some(true);
                    it.actions
                        .retain(|a| !matches!(a, PageActionWire::AddToLibrary(_)));
                    album_items.push(it);
                }
            }
        }
    }

    // 2. Fetch tracks for each library album concurrently to produce the library artist Songs section
    let mut track_futures = Vec::new();
    for aid in &album_ids {
        let alb_path = format!("/v1/me/library/albums/{aid}");
        track_futures
            .push(async move { api.send_request(&alb_path, &[("include", "tracks")]).await });
    }
    let track_results = futures::future::join_all(track_futures).await;

    let mut song_items = Vec::new();
    for alb_data in track_results.into_iter().flatten() {
        let tracks_opt = alb_data
            .get("data")
            .and_then(|d| d.as_array())
            .and_then(|a| a.first())
            .and_then(|first| first.get("relationships"))
            .and_then(|r| r.get("tracks"))
            .and_then(|t| t.get("data"))
            .and_then(|d| d.as_array());

        if let Some(tracks) = tracks_opt {
            for tr in tracks {
                if let Some(mut it) = mapper::map_apple_resource_to_item(tr) {
                    it.in_library = Some(true);
                    it.actions
                        .retain(|a| !matches!(a, PageActionWire::AddToLibrary(_)));
                    song_items.push(it);
                }
            }
        }
    }

    // Songs first (track-list hint), then Albums (grid hint)
    if !song_items.is_empty() {
        let section = PageSectionWire::new("songs", Some("Songs".to_string()), song_items)
            .with_hint("track-list");
        page.sections.push(section);
    }

    if !album_items.is_empty() {
        let section = PageSectionWire::new("albums", Some("Albums".to_string()), album_items)
            .with_hint("grid");
        page.sections.push(section);
    }

    Ok(page)
}
