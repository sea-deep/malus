//! Presentation player model with monotonic local extrapolation.
//!
//! Exposes only consumer playback states: Playing, Paused, Stopped.
//! Zero engineering or browser concepts.

use malus_client::{PlaybackStateWire, TrackWire};
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct PlayerModel {
    pub state: PlaybackStateWire,
    pub current_track: Option<TrackWire>,
    pub position_ms: u64,
    pub duration_ms: Option<u64>,
    pub volume: u8,
    pub last_update: Option<Instant>,
}

impl Default for PlayerModel {
    fn default() -> Self {
        Self {
            state: PlaybackStateWire::Stopped,
            current_track: None,
            position_ms: 0,
            duration_ms: None,
            volume: 100,
            last_update: None,
        }
    }
}

impl PlayerModel {
    pub fn update_from_wire(
        &mut self,
        state: PlaybackStateWire,
        current_track: Option<TrackWire>,
        position_ms: u64,
        duration_ms: Option<u64>,
        volume: Option<u8>,
    ) {
        self.state = state;
        self.current_track = current_track;
        self.position_ms = position_ms;
        self.duration_ms = duration_ms;
        if let Some(vol) = volume {
            self.volume = vol;
        }
        self.last_update = if state == PlaybackStateWire::Playing {
            Some(Instant::now())
        } else {
            None
        };
    }

    /// Monotonically extrapolated playback position in milliseconds.
    /// Freezes when paused or stopped. Clamps strictly to duration if known.
    pub fn extrapolated_position_ms(&self) -> u64 {
        if self.state != PlaybackStateWire::Playing {
            return self.position_ms;
        }

        let elapsed_ms = self
            .last_update
            .map(|t| t.elapsed().as_millis() as u64)
            .unwrap_or(0);

        let pos = self.position_ms + elapsed_ms;
        if let Some(dur) = self.duration_ms {
            pos.min(dur)
        } else {
            pos
        }
    }

    pub fn is_idle(&self) -> bool {
        self.current_track.is_none() && self.state == PlaybackStateWire::Stopped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_player_idle_and_extrapolation() {
        let mut model = PlayerModel::default();
        assert!(model.is_idle());
        assert_eq!(model.extrapolated_position_ms(), 0);

        model.update_from_wire(PlaybackStateWire::Playing, None, 1000, Some(5000), Some(80));
        assert_eq!(model.volume, 80);
        assert!(!model.is_idle());
        assert!(model.extrapolated_position_ms() >= 1000);
    }
}
