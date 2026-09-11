//! Pure domain models for audio track entities.

use super::{album::AlbumRef, artist::ArtistRef, artwork::Artwork};
use crate::media_id::MediaId;

/// An immutable identity and metadata snapshot of an audio track.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Track {
    pub id: MediaId,
    pub title: String,
    pub artists: Vec<ArtistRef>,
    pub album: Option<AlbumRef>,
    pub duration_ms: Option<u64>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub explicit: Option<bool>,
    pub artwork: Option<Artwork>,
    pub uri: Option<String>,
}

impl Track {
    /// Construct a track with a single primary artist name.
    pub fn new(id: MediaId, title: impl Into<String>, artist: impl Into<String>) -> Self {
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
        }
    }

    /// Construct a track with multiple credited artists.
    pub fn with_artists(id: MediaId, title: impl Into<String>, artists: Vec<ArtistRef>) -> Self {
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
