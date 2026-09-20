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
    Search,
    LibraryRecentlyAdded,
    LibrarySongs,
    LibraryAlbums,
    LibraryGenres,
    LibraryArtists,
    LibraryPlaylists,
    LibraryMadeForYou,
    Album(String),
    Artist(String),
    Playlist(String),
    Curator(String),
    Replay(u16),
}

impl PageRoute {
    /// Parse from a canonical route string (e.g. "home", "search", "library:songs", "album:123").
    pub fn parse(route: &str) -> Option<Self> {
        match route {
            "home" => Some(Self::Home),
            "new" => Some(Self::New),
            "radio" => Some(Self::Radio),
            "search" | "search:" => Some(Self::Search),
            "library:recently-added" => Some(Self::LibraryRecentlyAdded),
            "library:songs" => Some(Self::LibrarySongs),
            "library:albums" => Some(Self::LibraryAlbums),
            "library:genres" => Some(Self::LibraryGenres),
            "library:artists" => Some(Self::LibraryArtists),
            "library:playlists" => Some(Self::LibraryPlaylists),
            "library:made-for-you" => Some(Self::LibraryMadeForYou),
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
                if let Some(id) = route.strip_prefix("curator:")
                    && !id.is_empty()
                {
                    return Some(Self::Curator(id.to_string()));
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
            Self::Search => "search".to_string(),
            Self::LibraryRecentlyAdded => "library:recently-added".to_string(),
            Self::LibrarySongs => "library:songs".to_string(),
            Self::LibraryAlbums => "library:albums".to_string(),
            Self::LibraryGenres => "library:genres".to_string(),
            Self::LibraryArtists => "library:artists".to_string(),
            Self::LibraryPlaylists => "library:playlists".to_string(),
            Self::LibraryMadeForYou => "library:made-for-you".to_string(),
            Self::Album(id) => format!("album:{id}"),
            Self::Artist(id) => format!("artist:{id}"),
            Self::Playlist(id) => format!("playlist:{id}"),
            Self::Curator(id) => format!("curator:{id}"),
            Self::Replay(year) => format!("replay:{year}"),
        }
    }

    /// Return the canonical Apple Music web share URL if applicable.
    pub fn web_url(&self) -> Option<String> {
        match self {
            Self::Album(id) => Some(format!("https://music.apple.com/album/{id}")),
            Self::Artist(id) => Some(format!("https://music.apple.com/artist/{id}")),
            Self::Playlist(id) => Some(format!("https://music.apple.com/playlist/{id}")),
            Self::Curator(id) => Some(format!("https://music.apple.com/curator/{id}")),
            Self::Replay(year) => Some(format!("https://replay.music.apple.com/{year}")),
            _ => None,
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
        assert_eq!(PageRoute::parse("search"), Some(PageRoute::Search));
        assert_eq!(PageRoute::parse("search:"), Some(PageRoute::Search));
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
            PageRoute::parse("library:genres"),
            Some(PageRoute::LibraryGenres)
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
            PageRoute::parse("library:made-for-you"),
            Some(PageRoute::LibraryMadeForYou)
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
            PageRoute::parse("playlist:pl.123"),
            Some(PageRoute::Playlist("pl.123".to_string()))
        );
        assert_eq!(
            PageRoute::parse("curator:982307152"),
            Some(PageRoute::Curator("982307152".to_string()))
        );
        assert_eq!(
            PageRoute::parse("replay:2024"),
            Some(PageRoute::Replay(2024))
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
