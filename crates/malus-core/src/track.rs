//! Pure domain models for tracks, albums, artists, and playlists.

use crate::media_id::MediaId;

/// An immutable identity and metadata snapshot of an audio track.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Track {
    pub id: MediaId,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub duration_ms: Option<u64>,
    pub uri: Option<String>,
}

impl Track {
    pub fn new(id: MediaId, title: impl Into<String>, artist: impl Into<String>) -> Self {
        Self {
            id,
            title: title.into(),
            artist: artist.into(),
            album: None,
            duration_ms: None,
            uri: None,
        }
    }

    pub fn with_album(mut self, album: impl Into<String>) -> Self {
        self.album = Some(album.into());
        self
    }

    pub fn with_duration_ms(mut self, duration_ms: u64) -> Self {
        self.duration_ms = Some(duration_ms);
        self
    }

    pub fn with_uri(mut self, uri: impl Into<String>) -> Self {
        self.uri = Some(uri.into());
        self
    }
}

/// An artist resource.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Artist {
    pub id: MediaId,
    pub name: String,
}

impl Artist {
    pub fn new(id: MediaId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
        }
    }
}

/// An album resource.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Album {
    pub id: MediaId,
    pub title: String,
    pub artist: Option<String>,
    pub track_ids: Vec<MediaId>,
}

impl Album {
    pub fn new(id: MediaId, title: impl Into<String>) -> Self {
        Self {
            id,
            title: title.into(),
            artist: None,
            track_ids: Vec::new(),
        }
    }
}

/// A playlist resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Playlist {
    pub id: MediaId,
    pub name: String,
    pub track_ids: Vec<MediaId>,
}

impl Playlist {
    pub fn new(id: MediaId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            track_ids: Vec::new(),
        }
    }
}
