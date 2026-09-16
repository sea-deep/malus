//! Canonical response parsers and normalizers for Apple Music API data.

use malus_model::{Album, AlbumRef, Artist, ArtistRef, Artwork, MediaRef, Playlist, Track};
use serde_json::Value;

/// Parse Apple artwork JSON into normalized Artwork.
///
/// Expands `{w}x{h}` placeholders to `600x600`.
pub fn parse_apple_artwork(art: &Value) -> Option<Artwork> {
    let raw_url = art["url"].as_str()?;
    let width = art["width"].as_u64().map(|w| w as u32);
    let height = art["height"].as_u64().map(|h| h as u32);
    let url = raw_url.replace("{w}", "600").replace("{h}", "600");
    Some(Artwork { url, width, height })
}

/// Parse Apple track (song) JSON into normalized Track with `song:{id}` MediaRef.
pub fn parse_apple_track(item: &Value) -> Option<Track> {
    let attrs = item.get("attributes").unwrap_or(item);
    let id_str = item["id"]
        .as_str()
        .or_else(|| attrs["playParams"]["id"].as_str())?;

    let raw_id = id_str.strip_prefix("song:").unwrap_or(id_str);
    let media_ref = MediaRef::Song(raw_id.to_string());

    let title = attrs["name"]
        .as_str()
        .or_else(|| item["title"].as_str())
        .unwrap_or("Unknown Title")
        .to_string();

    let mut artists = Vec::new();
    if let Some(art_arr) = item["relationships"]["artists"]["data"].as_array() {
        for a in art_arr {
            let name = a["attributes"]["name"]
                .as_str()
                .or_else(|| a["name"].as_str())
                .unwrap_or("");
            if !name.is_empty() {
                let id = a["id"].as_str().map(|i| MediaRef::Artist(i.to_string()));
                artists.push(ArtistRef::new(id, name));
            }
        }
    }
    if artists.is_empty()
        && let Some(name) = attrs["artistName"]
            .as_str()
            .or_else(|| item["artist"].as_str())
        && !name.is_empty()
    {
        artists.push(ArtistRef::named(name));
    }

    let album = if let Some(alb_arr) = item["relationships"]["albums"]["data"].as_array()
        && let Some(first_alb) = alb_arr.first()
    {
        let title = first_alb["attributes"]["name"]
            .as_str()
            .or_else(|| attrs["albumName"].as_str())
            .unwrap_or("");
        let id = first_alb["id"]
            .as_str()
            .map(|i| MediaRef::Album(i.to_string()));
        Some(AlbumRef::new(id, title))
    } else if let Some(title) = attrs["albumName"]
        .as_str()
        .or_else(|| item["album"].as_str())
    {
        if !title.is_empty() {
            Some(AlbumRef::titled(title))
        } else {
            None
        }
    } else {
        None
    };

    let duration_ms = attrs["durationInMillis"]
        .as_u64()
        .or_else(|| attrs["durationInMillis"].as_f64().map(|f| f as u64))
        .or_else(|| item["duration_ms"].as_u64());

    let track_number = attrs["trackNumber"].as_u64().map(|n| n as u32);
    let disc_number = attrs["discNumber"].as_u64().map(|n| n as u32);
    let explicit = attrs["contentRating"].as_str().map(|r| r == "explicit");

    let artwork = attrs
        .get("artwork")
        .or_else(|| item.get("artwork"))
        .and_then(parse_apple_artwork);
    let uri = attrs["url"]
        .as_str()
        .or_else(|| item["url"].as_str())
        .map(str::to_string);

    Some(Track {
        id: media_ref,
        title,
        artists,
        album,
        duration_ms,
        track_number,
        disc_number,
        explicit,
        artwork,
        uri,
    })
}

/// Parse Apple album JSON into normalized Album with `album:{id}` MediaRef.
pub fn parse_apple_album(item: &Value) -> Option<Album> {
    let attrs = item.get("attributes").unwrap_or(item);
    let id_str = item["id"].as_str()?;
    let raw_id = id_str.strip_prefix("album:").unwrap_or(id_str);
    let media_ref = MediaRef::Album(raw_id.to_string());

    let title = attrs["name"]
        .as_str()
        .unwrap_or("Unknown Album")
        .to_string();

    let mut artists = Vec::new();
    if let Some(art_arr) = item["relationships"]["artists"]["data"].as_array() {
        for a in art_arr {
            let name = a["attributes"]["name"]
                .as_str()
                .or_else(|| a["name"].as_str())
                .unwrap_or("");
            if !name.is_empty() {
                let id = a["id"].as_str().map(|i| MediaRef::Artist(i.to_string()));
                artists.push(ArtistRef::new(id, name));
            }
        }
    }
    if artists.is_empty()
        && let Some(name) = attrs["artistName"].as_str()
        && !name.is_empty()
    {
        artists.push(ArtistRef::named(name));
    }

    let track_count = attrs["trackCount"].as_u64().map(|n| n as u32);
    let release_date = attrs["releaseDate"].as_str().map(str::to_string);
    let artwork = attrs.get("artwork").and_then(parse_apple_artwork);

    Some(Album {
        id: media_ref,
        title,
        artists,
        release_date,
        track_count,
        artwork,
    })
}

/// Parse Apple artist JSON into normalized Artist with `artist:{id}` MediaRef.
pub fn parse_apple_artist(item: &Value) -> Option<Artist> {
    let attrs = item.get("attributes").unwrap_or(item);
    let id_str = item["id"].as_str()?;
    let raw_id = id_str.strip_prefix("artist:").unwrap_or(id_str);
    let media_ref = MediaRef::Artist(raw_id.to_string());

    let name = attrs["name"]
        .as_str()
        .unwrap_or("Unknown Artist")
        .to_string();
    let artwork = attrs.get("artwork").and_then(parse_apple_artwork);

    Some(Artist {
        id: media_ref,
        name,
        artwork,
    })
}

/// Parse Apple playlist JSON into normalized Playlist with `playlist:{id}` MediaRef.
pub fn parse_apple_playlist(item: &Value) -> Option<Playlist> {
    let attrs = item.get("attributes").unwrap_or(item);
    let id_str = item["id"].as_str()?;
    let raw_id = id_str.strip_prefix("playlist:").unwrap_or(id_str);
    let media_ref = MediaRef::Playlist(raw_id.to_string());

    let title = attrs["name"]
        .as_str()
        .unwrap_or("Unknown Playlist")
        .to_string();
    let description = attrs["description"]["standard"]
        .as_str()
        .or_else(|| attrs["description"].as_str())
        .map(str::to_string);
    let curator = attrs["curatorName"].as_str().map(str::to_string);
    let track_count = attrs["trackCount"].as_u64().map(|n| n as u32);
    let artwork = attrs.get("artwork").and_then(parse_apple_artwork);
    let can_edit = attrs["canEdit"].as_bool().unwrap_or(false);
    let can_delete = attrs["canDelete"].as_bool().unwrap_or(can_edit);
    let is_library = item["type"].as_str() == Some("library-playlists")
        || attrs
            .get("playParams")
            .and_then(|p| p["isLibrary"].as_bool())
            .unwrap_or(false)
        || raw_id.starts_with("p.");

    Some(Playlist {
        id: media_ref,
        title,
        curator,
        description,
        track_count,
        artwork,
        is_library,
        can_edit,
        can_delete,
    })
}
