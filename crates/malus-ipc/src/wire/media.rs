//! Normalized wire data transfer objects (DTOs) for transport and re-exports.
//!
//! Re-exports domain models from `malus-model` directly, and defines
//! wire transport containers (pagination, search results, auth status).

pub use malus_model::{
    AccountMediaState, Album, AlbumRef, Artist, ArtistRef, Artwork, CreditCategory, CreditItem,
    Credits, LyricLine, LyricSyllable, Lyrics, MediaRef, MediaRefError, PlaybackState,
    PlayerStatus, Playlist, Queue, Rating, RepeatMode, Track,
};
use serde::{Deserialize, Serialize};
use std::fmt;

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
    Station,
}

impl fmt::Display for SearchKindWire {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Track => write!(f, "track"),
            Self::Album => write!(f, "album"),
            Self::Artist => write!(f, "artist"),
            Self::Playlist => write!(f, "playlist"),
            Self::Station => write!(f, "station"),
        }
    }
}

/// Search scope: Apple Music catalog vs Personal Library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SearchScopeWire {
    #[default]
    Catalog,
    Library,
}

impl fmt::Display for SearchScopeWire {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog => write!(f, "catalog"),
            Self::Library => write!(f, "library"),
        }
    }
}

/// Categorized search results with category-specific pagination cursors.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchResultsWire {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_results: Option<Vec<super::page::PageItemWire>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracks: Option<PagedListWire<Track>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub albums: Option<PagedListWire<Album>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artists: Option<PagedListWire<Artist>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playlists: Option<PagedListWire<Playlist>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stations: Option<PagedListWire<super::page::PageItemWire>>,
}

impl SearchResultsWire {
    pub fn is_empty(&self) -> bool {
        self.top_results
            .as_ref()
            .map(|t| t.is_empty())
            .unwrap_or(true)
            && self
                .tracks
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
            && self
                .stations
                .as_ref()
                .map(|p| p.items.is_empty())
                .unwrap_or(true)
    }
}

/// A normalized catalog item detail entity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "item")]
pub enum CatalogItemWire {
    Track(Track),
    Album(Album),
    Artist(Artist),
    Playlist(Playlist),
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
    Tracks(PagedListWire<Track>),
    Albums(PagedListWire<Album>),
    Playlists(PagedListWire<Playlist>),
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
    fn test_search_results_wire_serialization() {
        let results = SearchResultsWire {
            tracks: Some(PagedListWire::new(
                vec![Track::new(
                    MediaRef::parse("song:1").unwrap(),
                    "Song",
                    "Artist",
                )],
                Some("cursor-123".to_string()),
            )),
            albums: Some(PagedListWire::new(
                vec![Album::new(
                    MediaRef::parse("album:1").unwrap(),
                    "Album Title",
                )],
                None,
            )),
            artists: None,
            playlists: None,
            ..Default::default()
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
        let item = CatalogItemWire::Track(Track::new(
            MediaRef::parse("song:1").unwrap(),
            "Song",
            "Artist",
        ));
        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("\"kind\":\"Track\""));
        let back: CatalogItemWire = serde_json::from_str(&json).unwrap();
        assert_eq!(back, item);
    }

    #[test]
    fn test_library_page_wire_serialization() {
        let page = LibraryPageWire::Albums(PagedListWire::new(
            vec![Album::new(MediaRef::parse("album:1").unwrap(), "Album")],
            Some("next-token".to_string()),
        ));
        let json = serde_json::to_string(&page).unwrap();
        assert!(json.contains("\"kind\":\"Albums\""));
        let back: LibraryPageWire = serde_json::from_str(&json).unwrap();
        assert_eq!(back, page);
    }
}
