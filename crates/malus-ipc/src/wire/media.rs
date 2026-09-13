//! Normalized wire data transfer objects (DTOs) for audio media and actions.
//!
//! Enforces physical serialization boundaries between `malus-model` domain models
//! and over-the-wire IPC representations.

use malus_model::{
    Album, AlbumRef, Artist, ArtistRef, Artwork, MediaRef, MediaRefError, PlaybackState, Player,
    Playlist, Queue, RepeatMode, Track,
};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Wire representation of a canonical media reference (`<kind>:<id>`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(try_from = "String", into = "String")]
pub struct MediaRefWire(String);

impl MediaRefWire {
    pub fn parse(s: &str) -> Result<Self, String> {
        MediaRef::parse(s)
            .map(|r| Self(r.to_string()))
            .map_err(|e| e.to_string())
    }

    pub fn new(kind: &str, id: &str) -> Result<Self, String> {
        let s = format!("{kind}:{id}");
        MediaRef::parse(&s)
            .map(|r| Self(r.to_string()))
            .map_err(|e| e.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn kind(&self) -> &str {
        self.0.split(':').next().unwrap_or("")
    }

    pub fn id(&self) -> &str {
        self.0.split_once(':').map(|(_, id)| id).unwrap_or("")
    }

    pub fn to_media_ref(&self) -> Result<MediaRef, MediaRefError> {
        MediaRef::parse(&self.0)
    }
}

impl TryFrom<String> for MediaRefWire {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<MediaRefWire> for String {
    fn from(wire: MediaRefWire) -> Self {
        wire.0
    }
}

impl From<&MediaRef> for MediaRefWire {
    fn from(r: &MediaRef) -> Self {
        Self(r.to_string())
    }
}

impl From<MediaRef> for MediaRefWire {
    fn from(r: MediaRef) -> Self {
        Self(r.to_string())
    }
}

impl TryFrom<MediaRefWire> for MediaRef {
    type Error = MediaRefError;

    fn try_from(wire: MediaRefWire) -> Result<Self, Self::Error> {
        MediaRef::parse(&wire.0)
    }
}

impl std::ops::Deref for MediaRefWire {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl fmt::Display for MediaRefWire {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Wire representation of artwork metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtworkWire {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

impl From<&Artwork> for ArtworkWire {
    fn from(a: &Artwork) -> Self {
        Self {
            url: a.url.clone(),
            width: a.width,
            height: a.height,
        }
    }
}

impl From<Artwork> for ArtworkWire {
    fn from(a: Artwork) -> Self {
        Self::from(&a)
    }
}

impl From<ArtworkWire> for Artwork {
    fn from(w: ArtworkWire) -> Self {
        Self {
            url: w.url,
            width: w.width,
            height: w.height,
        }
    }
}

/// Wire representation of a lightweight artist reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtistRefWire {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
}

impl ArtistRefWire {
    pub fn new(id: Option<impl Into<String>>, name: impl Into<String>) -> Self {
        Self {
            id: id.map(Into::into),
            name: name.into(),
        }
    }

    pub fn named(name: impl Into<String>) -> Self {
        Self {
            id: None,
            name: name.into(),
        }
    }
}

impl From<&ArtistRef> for ArtistRefWire {
    fn from(r: &ArtistRef) -> Self {
        Self {
            id: r.id.as_ref().map(|id| id.to_string()),
            name: r.name.clone(),
        }
    }
}

impl From<ArtistRef> for ArtistRefWire {
    fn from(r: ArtistRef) -> Self {
        Self::from(&r)
    }
}

impl TryFrom<ArtistRefWire> for ArtistRef {
    type Error = MediaRefError;

    fn try_from(w: ArtistRefWire) -> Result<Self, Self::Error> {
        let id = match w.id {
            Some(ref s) => Some(MediaRef::parse(s)?),
            None => None,
        };
        Ok(Self { id, name: w.name })
    }
}

/// Wire representation of a lightweight album reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlbumRefWire {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub title: String,
}

impl AlbumRefWire {
    pub fn new(id: Option<impl Into<String>>, title: impl Into<String>) -> Self {
        Self {
            id: id.map(Into::into),
            title: title.into(),
        }
    }

    pub fn titled(title: impl Into<String>) -> Self {
        Self {
            id: None,
            title: title.into(),
        }
    }
}

impl From<&AlbumRef> for AlbumRefWire {
    fn from(r: &AlbumRef) -> Self {
        Self {
            id: r.id.as_ref().map(|id| id.to_string()),
            title: r.title.clone(),
        }
    }
}

impl From<AlbumRef> for AlbumRefWire {
    fn from(r: AlbumRef) -> Self {
        Self::from(&r)
    }
}

impl TryFrom<AlbumRefWire> for AlbumRef {
    type Error = MediaRefError;

    fn try_from(w: AlbumRefWire) -> Result<Self, Self::Error> {
        let id = match w.id {
            Some(ref s) => Some(MediaRef::parse(s)?),
            None => None,
        };
        Ok(Self { id, title: w.title })
    }
}

/// Wire representation of a track.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackWire {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artists: Vec<ArtistRefWire>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album: Option<AlbumRefWire>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_number: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disc_number: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explicit: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artwork: Option<ArtworkWire>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
}

impl TrackWire {
    pub fn new(id: impl Into<String>, title: impl Into<String>, artist: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            artists: vec![ArtistRefWire::named(artist)],
            album: None,
            duration_ms: None,
            track_number: None,
            disc_number: None,
            explicit: None,
            artwork: None,
            uri: None,
        }
    }

    pub fn artist_display(&self) -> String {
        if self.artists.is_empty() {
            String::new()
        } else {
            self.artists
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        }
    }

    pub fn album_title(&self) -> Option<&str> {
        self.album.as_ref().map(|a| a.title.as_str())
    }

    pub fn media_ref(&self) -> Result<MediaRef, MediaRefError> {
        MediaRef::parse(&self.id)
    }
}

impl From<&Track> for TrackWire {
    fn from(track: &Track) -> Self {
        Self {
            id: track.id.to_string(),
            title: track.title.clone(),
            artists: track.artists.iter().map(Into::into).collect(),
            album: track.album.as_ref().map(Into::into),
            duration_ms: track.duration_ms,
            track_number: track.track_number,
            disc_number: track.disc_number,
            explicit: track.explicit,
            artwork: track.artwork.as_ref().map(Into::into),
            uri: track.uri.clone(),
        }
    }
}

impl From<Track> for TrackWire {
    fn from(track: Track) -> Self {
        Self::from(&track)
    }
}

impl TryFrom<TrackWire> for Track {
    type Error = MediaRefError;

    fn try_from(wire: TrackWire) -> Result<Self, Self::Error> {
        let id = MediaRef::parse(&wire.id)?;
        let mut artists = Vec::with_capacity(wire.artists.len());
        for a in wire.artists {
            artists.push(a.try_into()?);
        }
        let album = match wire.album {
            Some(a) => Some(a.try_into()?),
            None => None,
        };
        Ok(Self {
            id,
            title: wire.title,
            artists,
            album,
            duration_ms: wire.duration_ms,
            track_number: wire.track_number,
            disc_number: wire.disc_number,
            explicit: wire.explicit,
            artwork: wire.artwork.map(Into::into),
            uri: wire.uri,
        })
    }
}

/// Wire representation of an album metadata resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlbumWire {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artists: Vec<ArtistRefWire>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artwork: Option<ArtworkWire>,
}

impl AlbumWire {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            artists: Vec::new(),
            release_date: None,
            track_count: None,
            artwork: None,
        }
    }

    pub fn artist_display(&self) -> String {
        if self.artists.is_empty() {
            String::new()
        } else {
            self.artists
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        }
    }

    pub fn media_ref(&self) -> Result<MediaRef, MediaRefError> {
        MediaRef::parse(&self.id)
    }
}

impl From<&Album> for AlbumWire {
    fn from(a: &Album) -> Self {
        Self {
            id: a.id.to_string(),
            title: a.title.clone(),
            artists: a.artists.iter().map(Into::into).collect(),
            release_date: a.release_date.clone(),
            track_count: a.track_count,
            artwork: a.artwork.as_ref().map(Into::into),
        }
    }
}

impl From<Album> for AlbumWire {
    fn from(a: Album) -> Self {
        Self::from(&a)
    }
}

impl TryFrom<AlbumWire> for Album {
    type Error = MediaRefError;

    fn try_from(w: AlbumWire) -> Result<Self, Self::Error> {
        let id = MediaRef::parse(&w.id)?;
        let mut artists = Vec::with_capacity(w.artists.len());
        for a in w.artists {
            artists.push(a.try_into()?);
        }
        Ok(Self {
            id,
            title: w.title,
            artists,
            release_date: w.release_date,
            track_count: w.track_count,
            artwork: w.artwork.map(Into::into),
        })
    }
}

/// Wire representation of an artist resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtistWire {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artwork: Option<ArtworkWire>,
}

impl ArtistWire {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            artwork: None,
        }
    }

    pub fn media_ref(&self) -> Result<MediaRef, MediaRefError> {
        MediaRef::parse(&self.id)
    }
}

impl From<&Artist> for ArtistWire {
    fn from(a: &Artist) -> Self {
        Self {
            id: a.id.to_string(),
            name: a.name.clone(),
            artwork: a.artwork.as_ref().map(Into::into),
        }
    }
}

impl From<Artist> for ArtistWire {
    fn from(a: Artist) -> Self {
        Self::from(&a)
    }
}

impl TryFrom<ArtistWire> for Artist {
    type Error = MediaRefError;

    fn try_from(w: ArtistWire) -> Result<Self, Self::Error> {
        let id = MediaRef::parse(&w.id)?;
        Ok(Self {
            id,
            name: w.name,
            artwork: w.artwork.map(Into::into),
        })
    }
}

/// Wire representation of a playlist metadata resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaylistWire {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub curator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artwork: Option<ArtworkWire>,
}

impl PlaylistWire {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            curator: None,
            description: None,
            track_count: None,
            artwork: None,
        }
    }

    pub fn media_ref(&self) -> Result<MediaRef, MediaRefError> {
        MediaRef::parse(&self.id)
    }
}

impl From<&Playlist> for PlaylistWire {
    fn from(p: &Playlist) -> Self {
        Self {
            id: p.id.to_string(),
            title: p.title.clone(),
            curator: p.curator.clone(),
            description: p.description.clone(),
            track_count: p.track_count,
            artwork: p.artwork.as_ref().map(Into::into),
        }
    }
}

impl From<Playlist> for PlaylistWire {
    fn from(p: Playlist) -> Self {
        Self::from(&p)
    }
}

impl TryFrom<PlaylistWire> for Playlist {
    type Error = MediaRefError;

    fn try_from(w: PlaylistWire) -> Result<Self, Self::Error> {
        let id = MediaRef::parse(&w.id)?;
        Ok(Self {
            id,
            title: w.title,
            curator: w.curator,
            description: w.description,
            track_count: w.track_count,
            artwork: w.artwork.map(Into::into),
        })
    }
}

/// A generic paginated list of items with an opaque continuation cursor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PagedListWire<T> {
    pub items: Vec<T>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
}

impl<T> PagedListWire<T> {
    pub fn new(items: Vec<T>, next_cursor: Option<String>) -> Self {
        Self {
            items,
            next_cursor,
            total: None,
        }
    }

    pub fn empty() -> Self {
        Self {
            items: Vec::new(),
            next_cursor: None,
            total: None,
        }
    }
}

/// Canonical searchable media kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SearchKindWire {
    Track,
    Album,
    Artist,
    Playlist,
}

impl fmt::Display for SearchKindWire {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Track => write!(f, "track"),
            Self::Album => write!(f, "album"),
            Self::Artist => write!(f, "artist"),
            Self::Playlist => write!(f, "playlist"),
        }
    }
}

/// Categorized search results with category-specific pagination cursors.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchResultsWire {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracks: Option<PagedListWire<TrackWire>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub albums: Option<PagedListWire<AlbumWire>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artists: Option<PagedListWire<ArtistWire>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playlists: Option<PagedListWire<PlaylistWire>>,
}

impl SearchResultsWire {
    pub fn is_empty(&self) -> bool {
        self.tracks
            .as_ref()
            .map(|p| p.items.is_empty())
            .unwrap_or(true)
            && self
                .albums
                .as_ref()
                .map(|p| p.items.is_empty())
                .unwrap_or(true)
            && self
                .artists
                .as_ref()
                .map(|p| p.items.is_empty())
                .unwrap_or(true)
            && self
                .playlists
                .as_ref()
                .map(|p| p.items.is_empty())
                .unwrap_or(true)
    }
}

/// A normalized catalog item detail entity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "item")]
pub enum CatalogItemWire {
    Track(TrackWire),
    Album(AlbumWire),
    Artist(ArtistWire),
    Playlist(PlaylistWire),
}

/// Supported read-only library resource categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LibraryKindWire {
    Tracks,
    Albums,
    Playlists,
}

impl fmt::Display for LibraryKindWire {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tracks => write!(f, "tracks"),
            Self::Albums => write!(f, "albums"),
            Self::Playlists => write!(f, "playlists"),
        }
    }
}

/// A paginated read-only library result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "page")]
pub enum LibraryPageWire {
    Tracks(PagedListWire<TrackWire>),
    Albums(PagedListWire<AlbumWire>),
    Playlists(PagedListWire<PlaylistWire>),
}

/// Wire representation of playback state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlaybackStateWire {
    Stopped,
    Playing,
    Paused,
}

impl From<PlaybackState> for PlaybackStateWire {
    fn from(state: PlaybackState) -> Self {
        match state {
            PlaybackState::Stopped => Self::Stopped,
            PlaybackState::Playing => Self::Playing,
            PlaybackState::Paused => Self::Paused,
        }
    }
}

impl From<PlaybackStateWire> for PlaybackState {
    fn from(wire: PlaybackStateWire) -> Self {
        match wire {
            PlaybackStateWire::Stopped => Self::Stopped,
            PlaybackStateWire::Playing => Self::Playing,
            PlaybackStateWire::Paused => Self::Paused,
        }
    }
}

/// Wire representation of repeat mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepeatModeWire {
    Off,
    Track,
    All,
}

impl From<RepeatMode> for RepeatModeWire {
    fn from(mode: RepeatMode) -> Self {
        match mode {
            RepeatMode::Off => Self::Off,
            RepeatMode::Track => Self::Track,
            RepeatMode::All => Self::All,
        }
    }
}

impl From<RepeatModeWire> for RepeatMode {
    fn from(wire: RepeatModeWire) -> Self {
        match wire {
            RepeatModeWire::Off => Self::Off,
            RepeatModeWire::Track => Self::Track,
            RepeatModeWire::All => Self::All,
        }
    }
}

/// Wire representation of authoritative player status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerStatusWire {
    pub state: PlaybackStateWire,
    pub current_track: Option<TrackWire>,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub volume: u8,
    pub muted: bool,
    pub shuffle: bool,
    pub repeat: RepeatModeWire,
}

impl From<&Player> for PlayerStatusWire {
    fn from(p: &Player) -> Self {
        Self {
            state: p.state.into(),
            current_track: p.current_track.as_ref().map(Into::into),
            position_ms: p.position_ms,
            duration_ms: p.duration_ms,
            volume: p.volume,
            muted: p.muted,
            shuffle: p.shuffle,
            repeat: p.repeat.into(),
        }
    }
}

/// Wire representation of an authoritative queue snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueWire {
    pub items: Vec<TrackWire>,
    pub current_index: Option<usize>,
}

impl From<&Queue> for QueueWire {
    fn from(q: &Queue) -> Self {
        Self {
            items: q.items().iter().map(Into::into).collect(),
            current_index: q.current_index(),
        }
    }
}

/// Normalized service authentication state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AuthStateWire {
    Unknown,
    Checking,
    NeedsAuth,
    Authenticating,
    Authenticated,
    Failed,
}

impl fmt::Display for AuthStateWire {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown => write!(f, "unknown"),
            Self::Checking => write!(f, "checking"),
            Self::NeedsAuth => write!(f, "needs-auth"),
            Self::Authenticating => write!(f, "authenticating"),
            Self::Authenticated => write!(f, "authenticated"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

/// Wire representation of service authentication status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthStatusWire {
    pub state: AuthStateWire,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl AuthStatusWire {
    pub fn new(state: AuthStateWire) -> Self {
        Self {
            state,
            message: None,
        }
    }

    pub fn with_message(state: AuthStateWire, message: impl Into<String>) -> Self {
        Self {
            state,
            message: Some(message.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_track_wire_conversion() {
        let id = MediaRef::parse("song:1").unwrap();
        let track = Track::new(id, "Title", "Artist")
            .with_album("Album One")
            .with_duration_ms(180_000);
        let wire = TrackWire::from(&track);
        assert_eq!(wire.id, "song:1");
        assert_eq!(wire.title, "Title");
        assert_eq!(wire.artist_display(), "Artist");
        assert_eq!(wire.album_title(), Some("Album One"));

        let back: Track = wire.try_into().unwrap();
        assert_eq!(back.id.format(), "song:1");
        assert_eq!(back.artist_display(), "Artist");
    }

    #[test]
    fn test_album_wire_conversion() {
        let id = MediaRef::parse("album:1").unwrap();
        let album = Album::new(id, "Discovery")
            .with_artist(ArtistRef::named("Daft Punk"))
            .with_release_date("2001-03-12")
            .with_track_count(14);
        let wire = AlbumWire::from(&album);
        assert_eq!(wire.id, "album:1");
        assert_eq!(wire.title, "Discovery");
        assert_eq!(wire.artist_display(), "Daft Punk");
        assert_eq!(wire.track_count, Some(14));

        let back: Album = wire.try_into().unwrap();
        assert_eq!(back.id.format(), "album:1");
    }

    #[test]
    fn test_search_results_wire_serialization() {
        let results = SearchResultsWire {
            tracks: Some(PagedListWire::new(
                vec![TrackWire::new("song:1", "Song", "Artist")],
                Some("cursor-123".to_string()),
            )),
            albums: Some(PagedListWire::new(
                vec![AlbumWire::new("album:1", "Album Title")],
                None,
            )),
            artists: None,
            playlists: None,
        };

        let json = serde_json::to_string(&results).unwrap();
        let back: SearchResultsWire = serde_json::from_str(&json).unwrap();
        assert_eq!(back.tracks.as_ref().unwrap().items.len(), 1);
        assert_eq!(
            back.tracks.as_ref().unwrap().next_cursor.as_deref(),
            Some("cursor-123")
        );
        assert_eq!(back.albums.as_ref().unwrap().items.len(), 1);
        assert!(back.artists.is_none());
    }

    #[test]
    fn test_catalog_item_wire_serialization() {
        let item = CatalogItemWire::Track(TrackWire::new("song:1", "Song", "Artist"));
        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("\"kind\":\"Track\""));
        let back: CatalogItemWire = serde_json::from_str(&json).unwrap();
        assert_eq!(back, item);
    }

    #[test]
    fn test_library_page_wire_serialization() {
        let page = LibraryPageWire::Albums(PagedListWire::new(
            vec![AlbumWire::new("album:1", "Album")],
            Some("next-token".to_string()),
        ));
        let json = serde_json::to_string(&page).unwrap();
        assert!(json.contains("\"kind\":\"Albums\""));
        let back: LibraryPageWire = serde_json::from_str(&json).unwrap();
        assert_eq!(back, page);
    }

    #[test]
    fn test_media_ref_wire() {
        let wire = MediaRefWire::parse("song:617154362").unwrap();
        assert_eq!(wire.kind(), "song");
        assert_eq!(wire.id(), "617154362");
        assert_eq!(wire.as_str(), "song:617154362");

        let mref = wire.to_media_ref().unwrap();
        assert_eq!(mref, MediaRef::Song("617154362".to_string()));

        let json = serde_json::to_string(&wire).unwrap();
        assert_eq!(json, "\"song:617154362\"");
        let deserialized: MediaRefWire = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, wire);
    }
}
