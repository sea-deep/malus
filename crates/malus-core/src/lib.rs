//! `malus-core`: Pure domain logic for Malus.
//!
//! Free of I/O, tokio, serde, UI frameworks, provider-specific details, or external transports.

pub mod media;
pub mod media_id;
pub mod playback;
pub mod player;
pub mod queue;

pub use media::{Album, AlbumRef, Artist, ArtistRef, Artwork, Playlist, Track};
pub use media_id::{MediaId, MediaIdError};
pub use playback::{PlaybackState, RepeatMode};
pub use player::Player;
pub use queue::Queue;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_track_creation() {
        let id = MediaId::parse("mock:track:1").unwrap();
        let track = Track::new(id.clone(), "Track One", "Artist One")
            .with_album("Album One")
            .with_duration_ms(180_000)
            .with_track_number(3)
            .with_disc_number(1)
            .with_explicit(false)
            .with_artwork(Artwork::new("https://example.com/art.jpg").with_dimensions(600, 600));

        assert_eq!(track.id, id);
        assert_eq!(track.title, "Track One");
        assert_eq!(track.artists.len(), 1);
        assert_eq!(track.artists[0].name, "Artist One");
        assert_eq!(track.artist_display(), "Artist One");
        assert_eq!(
            track.album.as_ref().map(|a| a.title.as_str()),
            Some("Album One")
        );
        assert_eq!(track.duration_ms, Some(180_000));
        assert_eq!(track.track_number, Some(3));
        assert_eq!(track.disc_number, Some(1));
        assert_eq!(track.explicit, Some(false));
        assert_eq!(
            track.artwork.as_ref().map(|a| a.url.as_str()),
            Some("https://example.com/art.jpg")
        );
    }

    #[test]
    fn test_track_multi_artist() {
        let id = MediaId::parse("mock:track:2").unwrap();
        let artists = vec![
            ArtistRef::named("Primary Artist"),
            ArtistRef::new(
                Some(MediaId::parse("mock:artist:2").unwrap()),
                "Featured Artist",
            ),
        ];
        let track = Track::with_artists(id, "Collab Song", artists);
        assert_eq!(track.artists.len(), 2);
        assert_eq!(track.artist_display(), "Primary Artist, Featured Artist");
    }

    #[test]
    fn test_album_and_playlist_metadata() {
        let album_id = MediaId::parse("mock:album:10").unwrap();
        let album = Album::new(album_id.clone(), "Greatest Hits")
            .with_artist(ArtistRef::named("The Band"))
            .with_release_date("2023-01-01")
            .with_track_count(12);

        assert_eq!(album.id, album_id);
        assert_eq!(album.title, "Greatest Hits");
        assert_eq!(album.artist_display(), "The Band");
        assert_eq!(album.release_date.as_deref(), Some("2023-01-01"));
        assert_eq!(album.track_count, Some(12));

        let playlist_id = MediaId::parse("mock:playlist:20").unwrap();
        let playlist = Playlist::new(playlist_id.clone(), "Chill Mix")
            .with_curator("Apple Music")
            .with_description("Relaxing vibes")
            .with_track_count(50);

        assert_eq!(playlist.id, playlist_id);
        assert_eq!(playlist.title, "Chill Mix");
        assert_eq!(playlist.curator.as_deref(), Some("Apple Music"));
        assert_eq!(playlist.description.as_deref(), Some("Relaxing vibes"));
        assert_eq!(playlist.track_count, Some(50));
    }

    #[test]
    fn test_queue_lifecycle() {
        let mut q = Queue::new();
        assert!(q.is_empty());
        q.enqueue(Track::new(
            MediaId::parse("mock:track:1").unwrap(),
            "Song 1",
            "Artist",
        ));
        q.enqueue(Track::new(
            MediaId::parse("mock:track:2").unwrap(),
            "Song 2",
            "Artist",
        ));
        q.enqueue(Track::new(
            MediaId::parse("mock:track:3").unwrap(),
            "Song 3",
            "Artist",
        ));
        assert_eq!(q.len(), 3);
        assert_eq!(q.current_index(), Some(0));

        let next = q.next();
        assert_eq!(next.map(|t| t.id.as_str()), Some("mock:track:2"));
        assert_eq!(q.current_index(), Some(1));

        let prev = q.previous();
        assert_eq!(prev.map(|t| t.id.as_str()), Some("mock:track:1"));
        assert_eq!(q.current_index(), Some(0));

        q.move_item(2, 0);
        assert_eq!(q.items()[0].id.as_str(), "mock:track:3");
    }

    #[test]
    fn test_player_volume_clamping() {
        let mut p = Player::new();
        p.set_volume(150);
        assert_eq!(p.volume, 100);
        p.set_volume(45);
        assert_eq!(p.volume, 45);
    }
}
