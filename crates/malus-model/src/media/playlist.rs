//! Pure domain model for playlist entities.

use super::artwork::Artwork;
use crate::media_ref::MediaRef;

use serde::{Deserialize, Serialize};

/// A playlist metadata resource.
///
/// Track listings are fetched separately through paginated collection requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Playlist {
    pub id: MediaRef,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub curator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artwork: Option<Artwork>,
}

impl Playlist {
    pub fn new(id: MediaRef, title: impl Into<String>) -> Self {
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
