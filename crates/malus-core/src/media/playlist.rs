//! Pure domain model for playlist entities.

use super::artwork::Artwork;
use crate::media_id::MediaId;

/// A playlist metadata resource.
///
/// Track listings are fetched separately through paginated collection requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Playlist {
    pub id: MediaId,
    pub title: String,
    pub curator: Option<String>,
    pub description: Option<String>,
    pub track_count: Option<u32>,
    pub artwork: Option<Artwork>,
}

impl Playlist {
    pub fn new(id: MediaId, title: impl Into<String>) -> Self {
        Self {
            id,
            title: title.into(),
            curator: None,
            description: None,
            track_count: None,
            artwork: None,
        }
    }

    pub fn with_curator(mut self, curator: impl Into<String>) -> Self {
        self.curator = Some(curator.into());
        self
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
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
}
