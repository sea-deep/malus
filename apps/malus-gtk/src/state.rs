//! Pure UI state models for Malus GTK.

use malus_model::{
    AccountMediaState, Lyrics, MediaRef, PlaybackState, PlayerStatus, PresentationClock, Queue,
    Rating, RepeatMode, Track,
};

/// One immersive page, with an optional lyrics composition. It is an overlay on
/// navigation: opening it never pushes or clears browsing history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NowPlayingMode {
    #[default]
    Player,
    Lyrics,
    Queue,
}

#[derive(Debug, Default)]
pub struct PlayerPresentation {
    pub now: NowPlayingState,
    pub lyrics: LyricsState,
    pub queue: Option<Queue>,
}

pub type SharedPlayer = std::rc::Rc<std::cell::RefCell<PlayerPresentation>>;

#[derive(Debug, Default)]
pub struct LyricsState {
    pub generation: u64,
    pub content: Option<Lyrics>,
    pub loading: bool,
    pub error: Option<String>,
    pub active: Option<usize>,
}

impl LyricsState {
    pub fn update_position(&mut self, position_ms: u64) {
        self.active = self.content.as_ref().and_then(|lyrics| {
            if !lyrics.synced {
                return None;
            }
            lyrics
                .lines
                .iter()
                .enumerate()
                .filter(|(_, line)| line.start_ms.is_some_and(|start| start <= position_ms))
                .map(|(index, _)| index)
                .next_back()
        });
    }
}

#[derive(Debug, Clone)]
pub enum PlayerCommand {
    TogglePlay,
    Pause,
    Previous,
    Next,
    Shuffle(bool),
    Repeat(RepeatMode),
    Autoplay(bool),
    Seek { track: MediaRef, position_ms: u64 },
    Volume(u8),
}

/// Mutually exclusive modes for the right-hand utility pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UtilityMode {
    #[default]
    Closed,
    Queue,
    Lyrics,
}

impl UtilityMode {
    pub fn is_open(&self) -> bool {
        !matches!(self, Self::Closed)
    }

    pub fn is_queue(&self) -> bool {
        matches!(self, Self::Queue)
    }

    pub fn is_lyrics(&self) -> bool {
        matches!(self, Self::Lyrics)
    }

    pub fn toggle_queue(&mut self) {
        *self = if *self == Self::Queue {
            Self::Closed
        } else {
            Self::Queue
        };
    }

    pub fn toggle_lyrics(&mut self) {
        *self = if *self == Self::Lyrics {
            Self::Closed
        } else {
            Self::Lyrics
        };
    }
}

/// Now-playing playback state mirror.
#[derive(Debug, Clone, PartialEq)]
pub struct NowPlayingState {
    pub current_track: Option<Track>,
    pub playback_state: PlaybackState,
    pub duration_ms: u64,
    pub volume: u8,
    pub shuffle: bool,
    pub repeat: RepeatMode,
    pub autoplay: bool,
    pub is_live: bool,
    pub is_favorite: bool,
    pub in_library: bool,
    pub rating: Rating,
    pub clock: PresentationClock,
    pub is_changing_track: bool,
}

impl Default for NowPlayingState {
    fn default() -> Self {
        Self {
            current_track: None,
            playback_state: PlaybackState::Stopped,
            duration_ms: 0,
            volume: 100,
            shuffle: false,
            repeat: RepeatMode::Off,
            autoplay: malus_ipc::PlayerPreferences::load().autoplay,
            is_live: false,
            is_favorite: false,
            in_library: false,
            rating: Rating::Neutral,
            clock: PresentationClock::new(),
            is_changing_track: false,
        }
    }
}

impl NowPlayingState {
    pub fn is_live(&self) -> bool {
        self.is_live || self.current_track.as_ref().is_some_and(|t| t.is_live())
    }

    pub fn from_status(status: &PlayerStatus) -> Self {
        let mut clock = PresentationClock::new();
        clock.update(status);
        Self {
            current_track: status.current_track.clone(),
            playback_state: status.state,
            duration_ms: status.duration_ms,
            volume: status.volume,
            shuffle: status.shuffle,
            repeat: status.repeat,
            autoplay: status.autoplay,
            is_live: status.is_live(),
            is_favorite: false,
            in_library: false,
            rating: Rating::Neutral,
            clock,
            is_changing_track: false,
        }
    }

    pub fn update_from_status(&mut self, status: &PlayerStatus) {
        let track_changed = match (&self.current_track, &status.current_track) {
            (Some(a), Some(b)) => {
                a.id != b.id
                    || (matches!(a.id, MediaRef::Station(_))
                        && (a.title != b.title || a.artwork != b.artwork))
            }
            (None, None) => false,
            _ => true,
        };

        if track_changed {
            if status.current_track.is_none()
                && (self.is_changing_track || self.current_track.is_some())
            {
                // Retain optimistic or existing track metadata; do not wipe to None on transient empty status.
            } else {
                self.current_track = status.current_track.clone();
                self.is_favorite = false;
                self.in_library = false;
                self.rating = Rating::Neutral;
                if status.current_track.is_some()
                    && status.state != PlaybackState::Paused
                    && status.state != PlaybackState::Stopped
                    && status.position_ms == 0
                {
                    self.is_changing_track = true;
                }
            }
        } else if let (Some(cur), Some(new)) = (&mut self.current_track, &status.current_track) {
            // Same track identity: update dynamic fields but never downgrade enriched metadata.
            if new.duration_ms.is_some() && cur.duration_ms.is_none() {
                cur.duration_ms = new.duration_ms;
            }
            if new.artwork != cur.artwork && new.artwork.is_some() {
                cur.artwork = new.artwork.clone();
            }
            if matches!(cur.id, MediaRef::Station(_)) {
                if !new.title.is_empty() {
                    cur.title = new.title.clone();
                }
                if !new.artists.is_empty() {
                    cur.artists = new.artists.clone();
                }
                if new.album.is_some() {
                    cur.album = new.album.clone();
                }
            } else {
                if cur.album.as_ref().and_then(|a| a.id.as_ref()).is_none() && new.album.is_some() {
                    cur.album = new.album.clone();
                }
                // Preserve enriched artists:
                // Only adopt new.artists if new actually has more artists or has artist IDs while cur doesn't.
                let cur_has_ids = cur.artists.iter().any(|a| a.id.is_some());
                let new_has_ids = new.artists.iter().any(|a| a.id.is_some());
                if new.artists.len() > cur.artists.len() || (!cur_has_ids && new_has_ids) {
                    cur.artists = new.artists.clone();
                }
            }
        }

        self.playback_state = status.state;
        self.duration_ms = status.duration_ms;
        self.volume = status.volume;
        self.shuffle = status.shuffle;
        self.repeat = status.repeat;
        self.autoplay = status.autoplay;
        self.is_live = status.is_live();
        self.clock.update(status);

        if self.is_changing_track {
            let is_playing = status.state == PlaybackState::Playing;
            let clock_moved = self.clock.position_ms() > 0;
            let status_moved = status.position_ms > 0;
            if is_playing && (clock_moved || status_moved) {
                self.is_changing_track = false;
            }
        }
    }

    pub fn update_media_state(&mut self, state: &AccountMediaState) {
        if let Some(ref track) = self.current_track
            && track.id == state.reference
        {
            self.is_favorite = state.favorite;
            self.in_library = state.in_library;
            self.rating = state.rating;
        }
    }

    pub fn is_playing(&self) -> bool {
        self.playback_state == PlaybackState::Playing
    }

    pub fn extrapolated_position_ms(&self) -> u64 {
        self.clock.position_ms()
    }

    pub fn progress_fraction(&self) -> f64 {
        self.clock.progress_fraction()
    }
}

/// Queue mirror state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QueueState {
    pub queue: Queue,
}

impl QueueState {
    pub fn is_empty(&self) -> bool {
        self.queue.items.is_empty()
    }

    pub fn current_track(&self) -> Option<&Track> {
        self.queue.current_track()
    }

    pub fn upcoming_items(&self) -> &[Track] {
        if let Some(idx) = self.queue.current_index {
            if idx + 1 < self.queue.items.len() {
                &self.queue.items[idx + 1..]
            } else {
                &[]
            }
        } else {
            &self.queue.items
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lyrics_position_handles_opening_mid_song_and_unsynced_text() {
        use malus_model::LyricLine;
        let mut state = LyricsState {
            content: Some(Lyrics::new(
                vec![
                    LyricLine::new("first").with_timing(1000, 2000),
                    LyricLine::new("second").with_timing(3000, 4000),
                ],
                true,
            )),
            ..Default::default()
        };
        state.update_position(3500);
        assert_eq!(state.active, Some(1));
        state.update_position(500);
        assert_eq!(state.active, None);
        state.content.as_mut().unwrap().synced = false;
        state.update_position(3500);
        assert_eq!(state.active, None);
    }

    #[test]
    fn status_ticks_preserve_favorite_until_track_changes() {
        let mut state = NowPlayingState::default();
        let mut status = PlayerStatus {
            current_track: Some(Track::new(MediaRef::Song("one".into()), "One", "Artist")),
            state: PlaybackState::Playing,
            position_ms: 0,
            duration_ms: 10_000,
            volume: 0,
            muted: false,
            shuffle: false,
            repeat: RepeatMode::Off,
            autoplay: false,
            is_live: false,
            timeline_id: 1,
            sequence: 1,
        };
        state.update_from_status(&status);
        state.is_favorite = true;
        status.position_ms = 500;
        status.sequence = 2;
        state.update_from_status(&status);
        assert!(state.is_favorite);
        assert_eq!(state.volume, 0);
        status.current_track = Some(Track::new(MediaRef::Song("two".into()), "Two", "Artist"));
        status.position_ms = 0;
        status.timeline_id = 2;
        status.sequence = 3;
        state.update_from_status(&status);
        assert!(!state.is_favorite);
        assert!(state.extrapolated_position_ms() < 500);
    }

    #[test]
    fn update_from_status_preserves_enriched_artists_on_same_track() {
        use malus_model::ArtistRef;

        let mut state = NowPlayingState::default();
        let track_id = MediaRef::Song("12345".into());

        // 1. Initial status with single artist
        let status1 = PlayerStatus {
            current_track: Some(Track::new(
                track_id.clone(),
                "Banda Kaam Ka",
                "Chaar Diwaari",
            )),
            state: PlaybackState::Paused,
            position_ms: 0,
            duration_ms: 200_000,
            ..Default::default()
        };
        state.update_from_status(&status1);
        assert_eq!(state.current_track.as_ref().unwrap().artists.len(), 1);

        // 2. Track enrichment occurs (e.g. from get_catalog_item)
        if let Some(cur) = state.current_track.as_mut() {
            cur.artists = vec![
                ArtistRef::new(Some(MediaRef::Artist("art1".into())), "Chaar Diwaari"),
                ArtistRef::new(Some(MediaRef::Artist("art2".into())), "Sanjith Hegde"),
            ];
        }
        assert_eq!(state.current_track.as_ref().unwrap().artists.len(), 2);
        assert_eq!(
            state.current_track.as_ref().unwrap().artist_display(),
            "Chaar Diwaari, Sanjith Hegde"
        );

        // 3. Subsequent PlayerStatus arrives from MusicKit with single artist
        let status2 = PlayerStatus {
            current_track: Some(Track::new(
                track_id.clone(),
                "Banda Kaam Ka",
                "Chaar Diwaari",
            )),
            state: PlaybackState::Playing,
            position_ms: 1000,
            duration_ms: 200_000,
            ..Default::default()
        };
        state.update_from_status(&status2);

        // Enriched artists MUST NOT be overwritten!
        assert_eq!(state.current_track.as_ref().unwrap().artists.len(), 2);
        assert_eq!(
            state.current_track.as_ref().unwrap().artist_display(),
            "Chaar Diwaari, Sanjith Hegde"
        );
        assert_eq!(state.playback_state, PlaybackState::Playing);
    }

    #[test]
    fn test_utility_mode_toggle() {
        let mut mode = UtilityMode::Closed;
        assert!(!mode.is_open());

        mode.toggle_queue();
        assert_eq!(mode, UtilityMode::Queue);
        assert!(mode.is_open());

        mode.toggle_lyrics();
        assert_eq!(mode, UtilityMode::Lyrics);
        assert!(mode.is_open());

        mode.toggle_lyrics();
        assert_eq!(mode, UtilityMode::Closed);
        assert!(!mode.is_open());
    }

    #[test]
    fn test_now_playing_state_extrapolation() {
        let mut state = NowPlayingState::default();
        assert_eq!(state.extrapolated_position_ms(), 0);
        assert_eq!(state.progress_fraction(), 0.0);

        let status = PlayerStatus {
            state: PlaybackState::Playing,
            current_track: None,
            position_ms: 1000,
            duration_ms: 10000,
            volume: 80,
            muted: false,
            shuffle: false,
            repeat: RepeatMode::Off,
            autoplay: false,
            is_live: false,
            timeline_id: 1,
            sequence: 1,
        };

        state.update_from_status(&status);
        assert!(state.is_playing());
        assert!(state.extrapolated_position_ms() >= 1000);
        assert!(state.progress_fraction() >= 0.1);
    }

    #[test]
    fn test_now_playing_mode_variants() {
        assert_eq!(NowPlayingMode::default(), NowPlayingMode::Player);
        let player = NowPlayingMode::Player;
        let lyrics = NowPlayingMode::Lyrics;
        let queue = NowPlayingMode::Queue;
        assert_ne!(player, lyrics);
        assert_ne!(player, queue);
        assert_ne!(lyrics, queue);
    }

    #[test]
    fn test_track_transition_does_not_wipe_optimistic_track_on_intermediate_stopped_status() {
        let mut state = NowPlayingState::default();
        let track = Track::new(MediaRef::Song("1843376122".into()), "Dalli", "Seedhe Maut");

        // Optimistically set by user click
        state.current_track = Some(track.clone());
        state.is_changing_track = true;
        state.playback_state = PlaybackState::Stopped;

        // Intermediate teardown event from MusicKit (queue teardown: stopped, current_track: None)
        let intermediate_status = PlayerStatus {
            state: PlaybackState::Stopped,
            current_track: None,
            position_ms: 0,
            duration_ms: 0,
            volume: 100,
            muted: false,
            shuffle: false,
            repeat: RepeatMode::Off,
            autoplay: false,
            is_live: false,
            timeline_id: 2,
            sequence: 2,
        };
        state.update_from_status(&intermediate_status);

        // Track metadata must NOT be wiped! Spinner must NOT be cancelled!
        assert_eq!(state.current_track, Some(track.clone()));
        assert!(state.is_changing_track);

        // Track starts playing and time advances
        let playing_status = PlayerStatus {
            state: PlaybackState::Playing,
            current_track: Some(track.clone()),
            position_ms: 120,
            duration_ms: 180_000,
            volume: 100,
            muted: false,
            shuffle: false,
            repeat: RepeatMode::Off,
            autoplay: false,
            is_live: false,
            timeline_id: 2,
            sequence: 3,
        };
        state.update_from_status(&playing_status);

        // Now changing_track clears cleanly and playback is active
        assert!(!state.is_changing_track);
        assert_eq!(state.playback_state, PlaybackState::Playing);
        assert_eq!(state.current_track, Some(track));
    }

    #[test]
    fn test_existing_track_preserved_on_stopped_status() {
        let mut state = NowPlayingState::default();
        let track = Track::new(MediaRef::Song("1843376122".into()), "Dalli", "Seedhe Maut");

        state.current_track = Some(track.clone());
        state.is_changing_track = false;
        state.playback_state = PlaybackState::Playing;

        // Stopped status with no track (e.g. idle teardown or natural finish)
        let stopped_status = PlayerStatus {
            state: PlaybackState::Stopped,
            current_track: None,
            position_ms: 0,
            duration_ms: 0,
            volume: 100,
            muted: false,
            shuffle: false,
            repeat: RepeatMode::Off,
            autoplay: false,
            is_live: false,
            timeline_id: 3,
            sequence: 4,
        };
        state.update_from_status(&stopped_status);

        // Track must be preserved (not wiped to None / Not Playing)
        assert_eq!(state.current_track, Some(track));
        assert!(!state.is_changing_track);
        assert_eq!(state.playback_state, PlaybackState::Stopped);
    }
}
