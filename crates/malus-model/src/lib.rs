//! `malus-model`: Pure domain logic for Malus.
//!
//! Free of I/O, tokio, UI frameworks, provider-specific details, or external transports.

pub mod account;
pub mod credits;
pub mod lyrics;
pub mod media;
pub mod media_ref;
pub mod page_route;
pub mod playback;
pub mod player;
pub mod queue;

pub use account::{AccountMediaState, Rating};
pub use credits::{CreditCategory, CreditItem, Credits};
pub use lyrics::{LyricLine, LyricSyllable, Lyrics};
pub use media::{Album, AlbumRef, Artist, ArtistRef, Artwork, Playlist, Track};
pub use media_ref::{MediaRef, MediaRefError};
pub use page_route::{PageRoute, ParseRouteError};
pub use playback::{PlaybackState, RepeatMode};
pub use player::PlayerStatus;
pub use queue::Queue;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_track_creation() {
        let id = MediaRef::parse("song:1").unwrap();
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
        let id = MediaRef::parse("song:2").unwrap();
        let artists = vec![
            ArtistRef::named("Primary Artist"),
            ArtistRef::new(
                Some(MediaRef::parse("artist:2").unwrap()),
                "Featured Artist",
            ),
        ];
        let track = Track::with_artists(id, "Collab Song", artists);
        assert_eq!(track.artists.len(), 2);
        assert_eq!(track.artist_display(), "Primary Artist, Featured Artist");
    }

    #[test]
    fn test_album_and_playlist_metadata() {
        let album_id = MediaRef::parse("album:10").unwrap();
        let album = Album::new(album_id.clone(), "Greatest Hits")
            .with_artist(ArtistRef::named("The Band"))
            .with_release_date("2023-01-01")
            .with_track_count(12);

        assert_eq!(album.id, album_id);
        assert_eq!(album.title, "Greatest Hits");
        assert_eq!(album.artist_display(), "The Band");
        assert_eq!(album.release_date.as_deref(), Some("2023-01-01"));
        assert_eq!(album.track_count, Some(12));

        let playlist_id = MediaRef::parse("playlist:20").unwrap();
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
    fn test_queue_snapshot_and_serde() {
        let q = Queue::with_items(
            vec![
                Track::new(MediaRef::parse("song:1").unwrap(), "Song 1", "Artist"),
                Track::new(MediaRef::parse("song:2").unwrap(), "Song 2", "Artist"),
            ],
            Some(0),
        );
        assert_eq!(q.len(), 2);
        assert_eq!(q.current_index(), Some(0));
        assert_eq!(
            q.current_track().map(|t| t.id.format()),
            Some("song:1".to_string())
        );

        let json = serde_json::to_string(&q).unwrap();
        let decoded: Queue = serde_json::from_str(&json).unwrap();
        assert_eq!(q, decoded);
    }

    #[test]
    fn test_player_volume_clamping() {
        let mut p = PlayerStatus::new();
        p.set_volume(150);
        assert_eq!(p.volume, 100);
        p.set_volume(45);
        assert_eq!(p.volume, 45);
    }

    #[test]
    fn test_lyrics_and_credits_models() {
        let line = LyricLine::new("Test lyric line").with_timing(0, 3000);
        let lyrics = Lyrics::new(vec![line], true);
        assert!(lyrics.synced);
        assert_eq!(lyrics.lines.len(), 1);

        let json = serde_json::to_string(&lyrics).unwrap();
        let decoded: Lyrics = serde_json::from_str(&json).unwrap();
        assert_eq!(lyrics, decoded);

        let cat = CreditCategory::new(
            "COMPOSITION & LYRICS",
            "songwriters",
            vec![CreditItem::new("Artist", vec!["Composer".to_string()])],
        );
        let credits = Credits::new(vec![cat]);
        let json = serde_json::to_string(&credits).unwrap();
        let decoded: Credits = serde_json::from_str(&json).unwrap();
        assert_eq!(credits, decoded);
    }

    #[test]
    fn test_account_media_state_model() {
        let reference = MediaRef::Song("123".to_string());
        let state = AccountMediaState::new(reference.clone(), true, Rating::Favorite);
        assert!(state.in_library);
        assert_eq!(state.rating, Rating::Favorite);
        assert!(state.is_favorite());

        let json = serde_json::to_string(&state).unwrap();
        let decoded: AccountMediaState = serde_json::from_str(&json).unwrap();
        assert_eq!(state, decoded);
    }
}
