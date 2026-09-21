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
    #[serde(default)]
    pub autoplay: bool,
    #[serde(default)]
    pub is_live: bool,
    #[serde(default)]
    pub timeline_id: u64,
    #[serde(default)]
    pub sequence: u64,
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
            autoplay: false,
            is_live: false,
            timeline_id: 0,
            sequence: 0,
        }
    }
}

impl PlayerStatus {
    pub fn is_live(&self) -> bool {
        self.is_live || self.current_track.as_ref().is_some_and(|t| t.is_live())
    }
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

    pub fn with_timeline(mut self, timeline_id: u64, sequence: u64) -> Self {
        self.timeline_id = timeline_id;
        self.sequence = sequence;
        self
    }
}
