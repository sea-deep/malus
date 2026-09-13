//! Canonical response parsers and normalizers for Apple Music API data.

use malus_ipc::wire::{
    AlbumRefWire, AlbumWire, ArtistRefWire, ArtistWire, ArtworkWire, PlaylistWire, TrackWire,
};
use serde_json::Value;

/// Parse Apple artwork JSON into normalized ArtworkWire.
///
/// Expands `{w}x{h}` placeholders to `600x600`.
pub fn parse_apple_artwork(art: &Value) -> Option<ArtworkWire> {
    let raw_url = art["url"].as_str()?;
    let width = art["width"].as_u64().map(|w| w as u32);
    let height = art["height"].as_u64().map(|h| h as u32);
    let url = raw_url.replace("{w}", "600").replace("{h}", "600");
    Some(ArtworkWire { url, width, height })
}

/// Parse Apple track (song) JSON into normalized TrackWire with `song:{id}` MediaRef.
pub fn parse_apple_track(item: &Value) -> Option<TrackWire> {
    let attrs = item.get("attributes").unwrap_or(item);
    let id_str = item["id"]
        .as_str()
        .or_else(|| attrs["playParams"]["id"].as_str())?;

    let media_id = if id_str.starts_with("song:") {
        id_str.to_string()
    } else {
        format!("song:{id_str}")
    };

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
                let id = a["id"].as_str().map(|i| format!("artist:{i}"));
                artists.push(ArtistRefWire::new(id, name));
            }
        }
    }
    if artists.is_empty()
        && let Some(name) = attrs["artistName"]
            .as_str()
            .or_else(|| item["artist"].as_str())
        && !name.is_empty()
    {
        artists.push(ArtistRefWire::named(name));
    }

    let album = if let Some(alb_arr) = item["relationships"]["albums"]["data"].as_array()
        && let Some(first_alb) = alb_arr.first()
    {
        let title = first_alb["attributes"]["name"]
            .as_str()
            .or_else(|| attrs["albumName"].as_str())
            .unwrap_or("");
        let id = first_alb["id"].as_str().map(|i| format!("album:{i}"));
        Some(AlbumRefWire::new(id, title))
    } else if let Some(title) = attrs["albumName"]
        .as_str()
        .or_else(|| item["album"].as_str())
    {
        if !title.is_empty() {
            Some(AlbumRefWire::titled(title))
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

    Some(TrackWire {
        id: media_id,
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

/// Parse Apple album JSON into normalized AlbumWire with `album:{id}` MediaRef.
pub fn parse_apple_album(item: &Value) -> Option<AlbumWire> {
    let attrs = item.get("attributes").unwrap_or(item);
    let id_str = item["id"].as_str()?;
    let media_id = if id_str.starts_with("album:") {
        id_str.to_string()
    } else {
        format!("album:{id_str}")
    };

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
                let id = a["id"].as_str().map(|i| format!("artist:{i}"));
                artists.push(ArtistRefWire::new(id, name));
            }
        }
    }
    if artists.is_empty()
        && let Some(name) = attrs["artistName"].as_str()
        && !name.is_empty()
    {
        artists.push(ArtistRefWire::named(name));
    }

    let track_count = attrs["trackCount"].as_u64().map(|n| n as u32);
    let release_date = attrs["releaseDate"].as_str().map(str::to_string);
    let artwork = attrs.get("artwork").and_then(parse_apple_artwork);

    Some(AlbumWire {
        id: media_id,
        title,
        artists,
        release_date,
        track_count,
        artwork,
    })
}

/// Parse Apple artist JSON into normalized ArtistWire with `artist:{id}` MediaRef.
pub fn parse_apple_artist(item: &Value) -> Option<ArtistWire> {
    let attrs = item.get("attributes").unwrap_or(item);
    let id_str = item["id"].as_str()?;
    let media_id = if id_str.starts_with("artist:") {
        id_str.to_string()
    } else {
        format!("artist:{id_str}")
    };

    let name = attrs["name"]
        .as_str()
        .unwrap_or("Unknown Artist")
        .to_string();
    let artwork = attrs.get("artwork").and_then(parse_apple_artwork);

    Some(ArtistWire {
        id: media_id,
        name,
        artwork,
    })
}

/// Parse Apple playlist JSON into normalized PlaylistWire with `playlist:{id}` MediaRef.
pub fn parse_apple_playlist(item: &Value) -> Option<PlaylistWire> {
    let attrs = item.get("attributes").unwrap_or(item);
    let id_str = item["id"].as_str()?;
    let media_id = if id_str.starts_with("playlist:") {
        id_str.to_string()
    } else {
        format!("playlist:{id_str}")
    };

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

    Some(PlaylistWire {
        id: media_id,
        title,
        curator,
        description,
        track_count,
        artwork,
    })
}
