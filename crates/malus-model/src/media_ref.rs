//! Canonical media references for Malus.
//!
//! Represents references to Apple Music entities in the un-namespaced format:
//! `<kind>:<id>` (e.g. `song:617154362`, `album:1440833098`, `artist:5468295`,
//! `playlist:pl.u-...`, `station:ra.978194965`).

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{fmt, str::FromStr};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaRefError {
    InvalidFormat(String),
    UnknownKind(String),
    EmptyId(String),
}

impl fmt::Display for MediaRefError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFormat(s) => {
                write!(f, "Invalid media reference '{s}': expected '<kind>:<id>'")
            }
            Self::UnknownKind(k) => write!(
                f,
                "Unknown media kind '{k}': expected 'song', 'album', 'artist', 'playlist', or 'station'"
            ),
            Self::EmptyId(s) => write!(
                f,
                "Invalid media reference '{s}': identifier component cannot be empty"
            ),
        }
    }
}

impl std::error::Error for MediaRefError {}

/// A typed media reference (`song:<id>`, `album:<id>`, etc.).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MediaRef {
    Song(String),
    Album(String),
    Artist(String),
    Playlist(String),
    Station(String),
}

impl MediaRef {
    /// Parse and validate a media reference.
    pub fn parse(s: &str) -> Result<Self, MediaRefError> {
        let (kind, id) = s
            .split_once(':')
            .ok_or_else(|| MediaRefError::InvalidFormat(s.to_string()))?;

        if id.is_empty() {
            return Err(MediaRefError::EmptyId(s.to_string()));
        }

        match kind {
            "song" => Ok(Self::Song(id.to_string())),
            "album" => Ok(Self::Album(id.to_string())),
            "artist" => Ok(Self::Artist(id.to_string())),
            "playlist" => Ok(Self::Playlist(id.to_string())),
            "station" => Ok(Self::Station(id.to_string())),
            other => Err(MediaRefError::UnknownKind(other.to_string())),
        }
    }

    /// Return the entity kind as a static string.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Song(_) => "song",
            Self::Album(_) => "album",
            Self::Artist(_) => "artist",
            Self::Playlist(_) => "playlist",
            Self::Station(_) => "station",
        }
    }

    /// Return the inner identifier string.
    pub fn id(&self) -> &str {
        match self {
            Self::Song(id)
            | Self::Album(id)
            | Self::Artist(id)
            | Self::Playlist(id)
            | Self::Station(id) => id.as_str(),
        }
    }

    /// Format to canonical serialized string (`<kind>:<id>`).
    pub fn format(&self) -> String {
        format!("{}:{}", self.kind(), self.id())
    }

    /// Return the canonical Apple Music web share URL.
    pub fn web_url(&self) -> Option<String> {
        match self {
            Self::Song(id) => Some(format!("https://music.apple.com/song/{id}")),
            Self::Album(id) => Some(format!("https://music.apple.com/album/{id}")),
            Self::Artist(id) => Some(format!("https://music.apple.com/artist/{id}")),
            Self::Playlist(id) => Some(format!("https://music.apple.com/playlist/{id}")),
            Self::Station(id) => Some(format!("https://music.apple.com/station/{id}")),
        }
    }
}

impl fmt::Display for MediaRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.kind(), self.id())
    }
}

impl FromStr for MediaRef {
    type Err = MediaRefError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl Serialize for MediaRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.format())
    }
}

impl<'de> Deserialize<'de> for MediaRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_media_refs() {
        let song = MediaRef::parse("song:617154362").unwrap();
        assert_eq!(song, MediaRef::Song("617154362".to_string()));
        assert_eq!(song.kind(), "song");
        assert_eq!(song.id(), "617154362");
        assert_eq!(song.to_string(), "song:617154362");

        let album = MediaRef::parse("album:1440833098").unwrap();
        assert_eq!(album, MediaRef::Album("1440833098".to_string()));
        assert_eq!(album.to_string(), "album:1440833098");

        let artist = MediaRef::parse("artist:5468295").unwrap();
        assert_eq!(artist, MediaRef::Artist("5468295".to_string()));
        assert_eq!(artist.to_string(), "artist:5468295");

        let playlist = MediaRef::parse("playlist:pl.u-12345").unwrap();
        assert_eq!(playlist, MediaRef::Playlist("pl.u-12345".to_string()));
        assert_eq!(playlist.to_string(), "playlist:pl.u-12345");

        let station = MediaRef::parse("station:ra.978194965").unwrap();
        assert_eq!(station, MediaRef::Station("ra.978194965".to_string()));
        assert_eq!(station.to_string(), "station:ra.978194965");
    }

    #[test]
    fn test_invalid_media_refs() {
        assert!(MediaRef::parse("invalid").is_err());
        assert!(MediaRef::parse("song:").is_err());
        assert!(MediaRef::parse("unknown:123").is_err());
        assert!(MediaRef::parse("track:123").is_err());
        assert!(MediaRef::parse("apple:song:123").is_err());
        assert!(MediaRef::parse("apple:track:123").is_err());
    }

    #[test]
    fn test_media_ref_serde_roundtrip() {
        let refs = vec![
            MediaRef::Song("617154362".to_string()),
            MediaRef::Album("1440833098".to_string()),
            MediaRef::Artist("5468295".to_string()),
            MediaRef::Playlist("pl.u-12345".to_string()),
            MediaRef::Station("ra.978194965".to_string()),
        ];

        for r in refs {
            let json = serde_json::to_string(&r).unwrap();
            assert_eq!(json, format!("\"{}\"", r.format()));
            let decoded: MediaRef = serde_json::from_str(&json).unwrap();
            assert_eq!(r, decoded);
        }

        // Serde deserialization rejects invalid forms
        assert!(serde_json::from_str::<MediaRef>("\"track:123\"").is_err());
        assert!(serde_json::from_str::<MediaRef>("\"apple:song:123\"").is_err());
        assert!(serde_json::from_str::<MediaRef>("\"invalid\"").is_err());
    }
}
