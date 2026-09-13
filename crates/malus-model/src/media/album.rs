//! Pure domain models for album entities and references.

use super::{artist::ArtistRef, artwork::Artwork};
use crate::media_ref::MediaRef;

use serde::{Deserialize, Serialize};

/// A lightweight reference to an album without embedding nested tracks.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AlbumRef {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<MediaRef>,
    pub title: String,
}

impl AlbumRef {
    pub fn new(id: Option<MediaRef>, title: impl Into<String>) -> Self {
        Self {
            id,
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

/// An album metadata resource.
///
/// Track listings are fetched separately through paginated collection requests.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Album {
    pub id: MediaRef,
    pub title: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artists: Vec<ArtistRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artwork: Option<Artwork>,
}

impl Album {
    pub fn new(id: MediaRef, title: impl Into<String>) -> Self {
        Self {
            id,
            title: title.into(),
            artists: Vec::new(),
            release_date: None,
            track_count: None,
            artwork: None,
        }
    }

    pub fn with_artist(mut self, artist: ArtistRef) -> Self {
        self.artists.push(artist);
        self
    }

    pub fn with_release_date(mut self, date: impl Into<String>) -> Self {
        self.release_date = Some(date.into());
        self
    }

    pub fn with_track_count(mut self, count: u32) -> Self {
        self.track_count = Some(count);
        self
    }

    pub fn with_artwork(mut self, artwork: Artwork) -> Self {
        self.artwork = Some(artwork);
        self
    }

    /// Primary artist display helper.
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
}
