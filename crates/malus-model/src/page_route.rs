//! Canonical page route model for Malus.
//!
//! Represents all consumer destinations across discover, library, detail, and summary pages.

use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

/// Error returned when a route string cannot be parsed into a PageRoute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseRouteError(pub String);

impl fmt::Display for ParseRouteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Invalid page route: '{}'", self.0)
    }
}

impl std::error::Error for ParseRouteError {}

/// Typed page route representing all consumer destinations.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", content = "id", rename_all = "kebab-case")]
pub enum PageRoute {
    Home,
    New,
    Radio,
    LibraryRecentlyAdded,
    LibrarySongs,
    LibraryAlbums,
    LibraryArtists,
    LibraryPlaylists,
    Album(String),
    Artist(String),
    Playlist(String),
    Replay(u16),
}

impl PageRoute {
    /// Parse from a canonical route string (e.g. "home", "library:songs", "album:123").
    pub fn parse(route: &str) -> Option<Self> {
        match route {
            "home" => Some(Self::Home),
            "new" => Some(Self::New),
            "radio" => Some(Self::Radio),
            "library:recently-added" => Some(Self::LibraryRecentlyAdded),
            "library:songs" => Some(Self::LibrarySongs),
            "library:albums" => Some(Self::LibraryAlbums),
            "library:artists" => Some(Self::LibraryArtists),
            "library:playlists" => Some(Self::LibraryPlaylists),
            _ => {
                if let Some(id) = route.strip_prefix("album:")
                    && !id.is_empty()
                {
                    return Some(Self::Album(id.to_string()));
                }
                if let Some(id) = route.strip_prefix("artist:")
                    && !id.is_empty()
                {
                    return Some(Self::Artist(id.to_string()));
                }
                if let Some(id) = route.strip_prefix("playlist:")
                    && !id.is_empty()
                {
                    return Some(Self::Playlist(id.to_string()));
                }
                if let Some(year_str) = route.strip_prefix("replay:")
                    && let Ok(year) = year_str.parse::<u16>()
                {
                    return Some(Self::Replay(year));
                }
                None
            }
        }
    }

    /// Format to canonical route string.
    pub fn format(&self) -> String {
        match self {
            Self::Home => "home".to_string(),
            Self::New => "new".to_string(),
            Self::Radio => "radio".to_string(),
            Self::LibraryRecentlyAdded => "library:recently-added".to_string(),
            Self::LibrarySongs => "library:songs".to_string(),
            Self::LibraryAlbums => "library:albums".to_string(),
            Self::LibraryArtists => "library:artists".to_string(),
            Self::LibraryPlaylists => "library:playlists".to_string(),
            Self::Album(id) => format!("album:{id}"),
            Self::Artist(id) => format!("artist:{id}"),
            Self::Playlist(id) => format!("playlist:{id}"),
            Self::Replay(year) => format!("replay:{year}"),
        }
    }
}

impl fmt::Display for PageRoute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format())
    }
}

impl FromStr for PageRoute {
    type Err = ParseRouteError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or_else(|| ParseRouteError(s.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_route_parse_and_format() {
        assert_eq!(PageRoute::parse("home"), Some(PageRoute::Home));
        assert_eq!(PageRoute::parse("new"), Some(PageRoute::New));
        assert_eq!(PageRoute::parse("radio"), Some(PageRoute::Radio));
        assert_eq!(
            PageRoute::parse("library:recently-added"),
            Some(PageRoute::LibraryRecentlyAdded)
        );
        assert_eq!(
            PageRoute::parse("library:songs"),
            Some(PageRoute::LibrarySongs)
        );
        assert_eq!(
            PageRoute::parse("library:albums"),
            Some(PageRoute::LibraryAlbums)
        );
        assert_eq!(
            PageRoute::parse("library:artists"),
            Some(PageRoute::LibraryArtists)
        );
        assert_eq!(
            PageRoute::parse("library:playlists"),
            Some(PageRoute::LibraryPlaylists)
        );
        assert_eq!(
            PageRoute::parse("album:123"),
            Some(PageRoute::Album("123".to_string()))
        );
        assert_eq!(
            PageRoute::parse("artist:456"),
            Some(PageRoute::Artist("456".to_string()))
        );
        assert_eq!(
            PageRoute::parse("playlist:pl.789"),
            Some(PageRoute::Playlist("pl.789".to_string()))
        );
        assert_eq!(
            PageRoute::parse("replay:2026"),
            Some(PageRoute::Replay(2026))
        );

        // Rejected obsolete prefixes
        assert_eq!(PageRoute::parse("apple:page:home"), None);
        assert_eq!(PageRoute::parse("apple:surface:home"), None);
        assert_eq!(PageRoute::parse("invalid"), None);

        assert_eq!(PageRoute::Home.format(), "home");
        assert_eq!(PageRoute::Album("123".to_string()).format(), "album:123");
        assert_eq!(PageRoute::Replay(2026).format(), "replay:2026");

        // Display and FromStr
        assert_eq!(PageRoute::Home.to_string(), "home");
        assert_eq!(
            "album:123".parse::<PageRoute>().unwrap(),
            PageRoute::Album("123".to_string())
        );
        assert!("invalid".parse::<PageRoute>().is_err());
    }
}
