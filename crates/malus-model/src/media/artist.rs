//! Pure domain models for artist entities and references.

use super::artwork::Artwork;
use crate::media_ref::MediaRef;

/// A lightweight reference to an artist without circular entity graphs.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArtistRef {
    pub id: Option<MediaRef>,
    pub name: String,
}

impl ArtistRef {
    pub fn new(id: Option<MediaRef>, name: impl Into<String>) -> Self {
        Self {
            id,
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

/// An artist resource.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Artist {
    pub id: MediaRef,
    pub name: String,
    pub artwork: Option<Artwork>,
}

impl Artist {
    pub fn new(id: MediaRef, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            artwork: None,
        }
    }

    pub fn with_artwork(mut self, artwork: Artwork) -> Self {
        self.artwork = Some(artwork);
        self
    }
}
