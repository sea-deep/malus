//! Pure UI state models for Malus GTK.

use malus_model::{
    AccountMediaState, Lyrics, MediaRef, PlaybackState, PlayerStatus, Queue, Rating, RepeatMode,
    Track,
};

/// One immersive page, with an optional lyrics composition. It is an overlay on
/// navigation: opening it never pushes or clears browsing history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NowPlayingMode {
    #[default]
    Player,
    Lyrics,
}

#[derive(Debug, Default)]
pub struct PlayerPresentation {
    pub now: NowPlayingState,
    pub lyrics: LyricsState,
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
    Previous,
    Next,
    Shuffle(bool),
    Repeat(RepeatMode),
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
    pub position_ms: u64,
    pub duration_ms: u64,
    pub volume: u8,
    pub shuffle: bool,
    pub repeat: RepeatMode,
    pub is_favorite: bool,
    pub in_library: bool,
    pub rating: Rating,
    pub last_update: Option<std::time::Instant>,
}

impl Default for NowPlayingState {
    fn default() -> Self {
        Self {
            current_track: None,
            playback_state: PlaybackState::Stopped,
            position_ms: 0,
            duration_ms: 0,
            volume: 100,
            shuffle: false,
            repeat: RepeatMode::Off,
            is_favorite: false,
            in_library: false,
            rating: Rating::Neutral,
            last_update: None,
        }
    }
}

impl NowPlayingState {
    pub fn from_status(status: &PlayerStatus) -> Self {
        let last_update = if status.state == PlaybackState::Playing {
            Some(std::time::Instant::now())
        } else {
            None
        };
        Self {
            current_track: status.current_track.clone(),
            playback_state: status.state,
            position_ms: status.position_ms,
            duration_ms: status.duration_ms,
            volume: status.volume,
            shuffle: status.shuffle,
            repeat: status.repeat,
            is_favorite: false,
            in_library: false,
            rating: Rating::Neutral,
            last_update,
        }
    }

    pub fn update_from_status(&mut self, status: &PlayerStatus) {
        let track_changed = match (&self.current_track, &status.current_track) {
            (Some(a), Some(b)) => a.id != b.id,
            (None, None) => false,
            _ => true,
        };

        self.current_track = status.current_track.clone();
        self.playback_state = status.state;
        self.position_ms = status.position_ms;
        self.duration_ms = status.duration_ms;
        self.volume = status.volume;
        self.shuffle = status.shuffle;
        self.repeat = status.repeat;
        self.last_update = if status.state == PlaybackState::Playing {
            Some(std::time::Instant::now())
        } else {
            None
        };

        if track_changed {
            self.is_favorite = false;
            self.in_library = false;
            self.rating = Rating::Neutral;
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
        if self.playback_state != PlaybackState::Playing {
            return self.position_ms;
        }
        let elapsed_ms = self
            .last_update
            .map(|t| t.elapsed().as_millis() as u64)
            .unwrap_or(0);
        // Freeze position if daemon sample is stale (bridge heartbeat ~1s,
        // so >3s means updates stopped). Prevents ghost advancement.
        if elapsed_ms > 3000 {
            return self.position_ms;
        }
        let pos = self.position_ms + elapsed_ms;
        if self.duration_ms > 0 {
            pos.min(self.duration_ms)
        } else {
            pos
        }
    }

    pub fn progress_fraction(&self) -> f64 {
        let pos = self.extrapolated_position_ms();
        if self.duration_ms == 0 {
            0.0
        } else {
            (pos as f64 / self.duration_ms as f64).clamp(0.0, 1.0)
        }
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
        };
        state.update_from_status(&status);
        state.is_favorite = true;
        status.position_ms = 500;
        state.update_from_status(&status);
        assert!(state.is_favorite);
        assert_eq!(state.volume, 0);
        status.current_track = Some(Track::new(MediaRef::Song("two".into()), "Two", "Artist"));
        status.position_ms = 0;
        state.update_from_status(&status);
        assert!(!state.is_favorite);
        assert!(state.extrapolated_position_ms() < 500);
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
        };

        state.update_from_status(&status);
        assert!(state.is_playing());
        assert!(state.extrapolated_position_ms() >= 1000);
        assert!(state.progress_fraction() >= 0.1);
    }
}
