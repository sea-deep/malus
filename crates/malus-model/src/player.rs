//! Pure domain player status model representing audio playback state.

use crate::{
    media::Track,
    playback::{PlaybackState, RepeatMode},
};
use serde::{Deserialize, Serialize};

/// High-level authoritative player status snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerStatus {
    pub state: PlaybackState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_track: Option<Track>,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub volume: u8,
    pub muted: bool,
    pub shuffle: bool,
    pub repeat: RepeatMode,
}

impl Default for PlayerStatus {
    fn default() -> Self {
        Self {
            state: PlaybackState::Stopped,
            current_track: None,
            position_ms: 0,
            duration_ms: 0,
            volume: 100,
            muted: false,
            shuffle: false,
            repeat: RepeatMode::Off,
        }
    }
}

impl PlayerStatus {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_volume(&mut self, volume: u8) {
        self.volume = volume.min(100);
    }

    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
    }

    pub fn set_state(&mut self, state: PlaybackState) {
        self.state = state;
    }

    pub fn set_position_ms(&mut self, position_ms: u64) {
        self.position_ms = position_ms;
    }

    pub fn set_duration_ms(&mut self, duration_ms: u64) {
        self.duration_ms = duration_ms;
    }

    pub fn set_current_track(&mut self, track: Option<Track>) {
        if let Some(ref t) = track {
            self.duration_ms = t.duration_ms.unwrap_or(0);
        } else {
            self.duration_ms = 0;
            self.position_ms = 0;
        }
        self.current_track = track;
    }
}
