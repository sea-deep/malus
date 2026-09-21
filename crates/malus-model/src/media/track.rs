//! Pure domain models for audio track entities.

use super::{album::AlbumRef, artist::ArtistRef, artwork::Artwork};
use crate::media_ref::MediaRef;

use serde::{Deserialize, Serialize};

/// An immutable identity and metadata snapshot of an audio track.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Track {
    pub id: MediaRef,
    pub title: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artists: Vec<ArtistRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album: Option<AlbumRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_number: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disc_number: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explicit: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artwork: Option<Artwork>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_live: Option<bool>,
}

impl Track {
    /// Construct a track with a single primary artist name.
    pub fn new(id: MediaRef, title: impl Into<String>, artist: impl Into<String>) -> Self {
        Self {
            id,
            title: title.into(),
            artists: vec![ArtistRef::named(artist)],
            album: None,
            duration_ms: None,
            track_number: None,
            disc_number: None,
            explicit: None,
            artwork: None,
            uri: None,
            is_live: None,
        }
    }

    /// Construct a track with multiple credited artists.
    pub fn with_artists(id: MediaRef, title: impl Into<String>, artists: Vec<ArtistRef>) -> Self {
        Self {
            id,
            title: title.into(),
            artists,
            album: None,
            duration_ms: None,
            track_number: None,
            disc_number: None,
            explicit: None,
            artwork: None,
            uri: None,
            is_live: None,
        }
    }

    /// Display string formatted from credited artists.
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

    pub fn is_live(&self) -> bool {
        self.is_live
            .unwrap_or(matches!(self.id, MediaRef::Station(_)))
    }

    pub fn with_album(mut self, album: impl Into<String>) -> Self {
        self.album = Some(AlbumRef::titled(album));
        self
    }

    pub fn with_album_ref(mut self, album: AlbumRef) -> Self {
        self.album = Some(album);
        self
    }

    pub fn with_duration_ms(mut self, duration_ms: u64) -> Self {
        self.duration_ms = Some(duration_ms);
        self
    }

    pub fn with_track_number(mut self, track_number: u32) -> Self {
        self.track_number = Some(track_number);
        self
    }

    pub fn with_disc_number(mut self, disc_number: u32) -> Self {
        self.disc_number = Some(disc_number);
        self
    }

    pub fn with_explicit(mut self, explicit: bool) -> Self {
        self.explicit = Some(explicit);
        self
    }

    pub fn with_artwork(mut self, artwork: Artwork) -> Self {
        self.artwork = Some(artwork);
        self
    }

    pub fn with_uri(mut self, uri: impl Into<String>) -> Self {
        self.uri = Some(uri.into());
        self
    }
}
