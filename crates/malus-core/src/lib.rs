//! `malus-core`: Pure domain logic for Malus.
//!
//! Free of I/O, tokio, serde, UI frameworks, provider-specific details, or external transports.

pub mod media_id;
pub mod playback;
pub mod player;
pub mod queue;
pub mod track;

pub use media_id::{MediaId, MediaIdError};
pub use playback::{PlaybackState, RepeatMode};
pub use player::Player;
pub use queue::Queue;
pub use track::{Album, Artist, Playlist, Track};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_track_creation() {
        let id = MediaId::parse("mock:track:1").unwrap();
        let track = Track::new(id.clone(), "Track One", "Artist One")
            .with_album("Album One")
            .with_duration_ms(180_000);
        assert_eq!(track.id, id);
        assert_eq!(track.title, "Track One");
        assert_eq!(track.artist, "Artist One");
        assert_eq!(track.album.as_deref(), Some("Album One"));
        assert_eq!(track.duration_ms, Some(180_000));
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
