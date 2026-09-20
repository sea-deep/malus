//! Canonical media presentation clock.
//!
//! Synchronizes UI and presentation consumers against authoritative playback samples
//! using local monotonic clock interpolation.
//!
//! Guarantees:
//! 1. Monotonic presentation time during continuous playback on the same timeline.
//! 2. Immediate position reset on timeline discontinuities (seeks, track changes, repeats).
//! 3. Out-of-order and stale sample rejection.
//! 4. Seamless pause and resume synchronization without time drift.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::{playback::PlaybackState, player::PlayerStatus};

/// Default staleness threshold for frozen updates (3 seconds).
pub const DEFAULT_STALENESS_THRESHOLD: Duration = Duration::from_secs(3);

/// An authoritative sample of playback state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybackSample {
    pub position_ms: u64,
    pub duration_ms: u64,
    pub state: PlaybackState,
    pub timeline_id: u64,
    pub sequence: u64,
}

impl From<&PlayerStatus> for PlaybackSample {
    fn from(status: &PlayerStatus) -> Self {
        Self {
            position_ms: status.position_ms,
            duration_ms: status.duration_ms,
            state: status.state,
            timeline_id: status.timeline_id,
            sequence: status.sequence,
        }
    }
}

impl From<PlayerStatus> for PlaybackSample {
    fn from(status: PlayerStatus) -> Self {
        Self::from(&status)
    }
}

/// A canonical presentation clock that synchronizes UI and presentation consumers
/// against authoritative playback samples.
#[derive(Debug)]
pub struct PresentationClock {
    /// The current authoritative sample anchor.
    sample: PlaybackSample,
    /// Local monotonic time when the anchor was set.
    anchor_instant: Instant,
    /// The authoritative audio playback position (ms) at `anchor_instant`.
    anchor_position_ms: u64,
    /// The highest presentation position (ms) displayed or committed on the current timeline.
    /// Preserves visual monotonicity across intermediate timer ticks without ratcheting the anchor.
    last_presented_ms: AtomicU64,
    /// Whether the clock has received at least one sample.
    initialized: bool,
}

impl Clone for PresentationClock {
    fn clone(&self) -> Self {
        Self {
            sample: self.sample.clone(),
            anchor_instant: self.anchor_instant,
            anchor_position_ms: self.anchor_position_ms,
            last_presented_ms: AtomicU64::new(self.last_presented_ms.load(Ordering::Relaxed)),
            initialized: self.initialized,
        }
    }
}

impl PartialEq for PresentationClock {
    fn eq(&self, other: &Self) -> bool {
        self.sample == other.sample
            && self.anchor_instant == other.anchor_instant
            && self.anchor_position_ms == other.anchor_position_ms
            && self.last_presented_ms.load(Ordering::Relaxed)
                == other.last_presented_ms.load(Ordering::Relaxed)
            && self.initialized == other.initialized
    }
}

impl Eq for PresentationClock {}

impl Default for PresentationClock {
    fn default() -> Self {
        Self {
            sample: PlaybackSample {
                position_ms: 0,
                duration_ms: 0,
                state: PlaybackState::Stopped,
                timeline_id: 0,
                sequence: 0,
            },
            anchor_instant: Instant::now(),
            anchor_position_ms: 0,
            last_presented_ms: AtomicU64::new(0),
            initialized: false,
        }
    }
}

impl PresentationClock {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the clock has received at least one sample.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Update clock with a new authoritative sample using the current time.
    pub fn update(&mut self, sample: impl Into<PlaybackSample>) {
        self.update_at(sample.into(), Instant::now());
    }

    /// Update clock with an authoritative sample at an explicit instant (for deterministic testing and measurement).
    pub fn update_at(&mut self, sample: PlaybackSample, now: Instant) {
        if !self.initialized {
            self.initialized = true;
            self.sample = sample.clone();
            self.anchor_instant = now;
            self.anchor_position_ms = sample.position_ms;
            self.last_presented_ms
                .store(sample.position_ms, Ordering::Relaxed);
            return;
        }

        // 1. Is this a new timeline (discontinuity)?
        if sample.timeline_id != self.sample.timeline_id {
            // Discontinuity: Seek, restart_current_item, new media item,
            // Next, Previous, QueueJump, queue replacement, Repeat-One wrap.
            // Reset immediately to the authoritative position.
            self.sample = sample.clone();
            self.anchor_instant = now;
            self.anchor_position_ms = sample.position_ms;
            self.last_presented_ms
                .store(sample.position_ms, Ordering::Relaxed);
            return;
        }

        // 2. Same timeline: Is this sample newer than the last sample?
        if sample.sequence <= self.sample.sequence {
            // Out-of-order or duplicate sample on the same timeline: discard
            return;
        }

        // 3. Same timeline, newer sample:
        // Record the current presentation position before updating anchor to preserve visual monotonicity.
        let current_pos = self.position_ms_at(now);

        self.sample = sample.clone();
        self.anchor_instant = now;
        self.anchor_position_ms = sample.position_ms;

        // If there was a large backward jump (> 1000ms) on the same timeline without a new timeline_id,
        // snap last_presented_ms to avoid freezing presentation for multiple seconds.
        let last = self.last_presented_ms.load(Ordering::Relaxed);
        if last > sample.position_ms + 1000 {
            self.last_presented_ms
                .store(sample.position_ms, Ordering::Relaxed);
        } else {
            self.last_presented_ms
                .store(last.max(current_pos), Ordering::Relaxed);
        }
    }

    /// Get current presentation position in milliseconds using current time.
    pub fn position_ms(&self) -> u64 {
        self.position_ms_at(Instant::now())
    }

    /// Get presentation position in milliseconds at an explicit instant.
    pub fn position_ms_at(&self, now: Instant) -> u64 {
        if !self.initialized {
            return 0;
        }

        if self.sample.state != PlaybackState::Playing {
            return self.anchor_position_ms;
        }

        let elapsed_ms = now
            .saturating_duration_since(self.anchor_instant)
            .as_millis() as u64;

        // Freeze position if updates stopped (>3000ms stale) to prevent infinite ghost advancement
        let effective_elapsed = if elapsed_ms > DEFAULT_STALENESS_THRESHOLD.as_millis() as u64 {
            DEFAULT_STALENESS_THRESHOLD.as_millis() as u64
        } else {
            elapsed_ms
        };

        let calculated_pos = self.anchor_position_ms + effective_elapsed;
        let last = self.last_presented_ms.load(Ordering::Relaxed);
        let presented = calculated_pos.max(last);
        let final_pos = if self.sample.duration_ms > 0 {
            presented.min(self.sample.duration_ms)
        } else {
            presented
        };
        self.last_presented_ms.store(final_pos, Ordering::Relaxed);
        final_pos
    }

    /// Progress fraction between 0.0 and 1.0.
    pub fn progress_fraction(&self) -> f64 {
        self.progress_fraction_at(Instant::now())
    }

    pub fn progress_fraction_at(&self, now: Instant) -> f64 {
        let pos = self.position_ms_at(now);
        if self.sample.duration_ms == 0 {
            0.0
        } else {
            (pos as f64 / self.sample.duration_ms as f64).clamp(0.0, 1.0)
        }
    }

    pub fn duration_ms(&self) -> u64 {
        self.sample.duration_ms
    }

    pub fn state(&self) -> PlaybackState {
        self.sample.state
    }

    pub fn timeline_id(&self) -> u64 {
        self.sample.timeline_id
    }

    pub fn sequence(&self) -> u64 {
        self.sample.sequence
    }

    pub fn sample(&self) -> &PlaybackSample {
        &self.sample
    }

    pub fn is_stale_at(&self, now: Instant, threshold: Duration) -> bool {
        now.saturating_duration_since(self.anchor_instant) > threshold
    }
}
