//! Playback state, queue, volume controller, and lyrics engine for Malus.

use super::track::Track;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackStatus {
    Playing,
    Paused,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

impl RepeatMode {
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Off => "Repeat: Off",
            Self::All => "Repeat: All",
            Self::One => "Repeat: One",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            Self::Off => Self::All,
            Self::All => Self::One,
            Self::One => Self::Off,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LyricLine {
    pub time_secs: u64,
    pub text: String,
}

impl LyricLine {
    pub fn new(time_secs: u64, text: impl Into<String>) -> Self {
        Self {
            time_secs,
            text: text.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlayerState {
    pub current_track: Option<Track>,
    pub status: PlaybackStatus,
    pub elapsed_secs: u64,
    pub volume: u8,
    pub shuffle: bool,
    pub repeat: RepeatMode,
    pub queue: Vec<Track>,
    pub history: Vec<Track>,
    pub lyrics_sync_enabled: bool,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            current_track: None,
            status: PlaybackStatus::Stopped,
            elapsed_secs: 0,
            volume: 80,
            shuffle: false,
            repeat: RepeatMode::Off,
            queue: Vec::new(),
            history: Vec::new(),
            lyrics_sync_enabled: true,
        }
    }
}

impl PlayerState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn play_track(&mut self, track: Track) {
        if let Some(prev) = self.current_track.take() {
            self.history.push(prev);
        }
        self.current_track = Some(track);
        self.elapsed_secs = 0;
        self.status = PlaybackStatus::Playing;
    }

    pub fn toggle_play(&mut self) {
        self.status = match self.status {
            PlaybackStatus::Playing => PlaybackStatus::Paused,
            PlaybackStatus::Paused => PlaybackStatus::Playing,
            PlaybackStatus::Stopped => {
                if self.current_track.is_some() {
                    PlaybackStatus::Playing
                } else if !self.queue.is_empty() {
                    let next = self.queue.remove(0);
                    self.play_track(next);
                    PlaybackStatus::Playing
                } else {
                    PlaybackStatus::Stopped
                }
            }
        };
    }

    pub fn next_track(&mut self) -> Option<Track> {
        if self.repeat == RepeatMode::One {
            self.elapsed_secs = 0;
            return self.current_track.clone();
        }

        if !self.queue.is_empty() {
            let next = self.queue.remove(0);
            self.play_track(next.clone());
            Some(next)
        } else if self.repeat == RepeatMode::All && !self.history.is_empty() {
            let mut all = std::mem::take(&mut self.history);
            if let Some(curr) = self.current_track.take() {
                all.push(curr);
            }
            if !all.is_empty() {
                let first = all.remove(0);
                self.queue = all;
                self.play_track(first.clone());
                Some(first)
            } else {
                self.status = PlaybackStatus::Stopped;
                None
            }
        } else {
            self.status = PlaybackStatus::Stopped;
            None
        }
    }

    pub fn prev_track(&mut self) -> Option<Track> {
        if self.elapsed_secs > 3 {
            self.elapsed_secs = 0;
            return self.current_track.clone();
        }

        if let Some(prev) = self.history.pop() {
            if let Some(curr) = self.current_track.take() {
                self.queue.insert(0, curr);
            }
            self.current_track = Some(prev.clone());
            self.elapsed_secs = 0;
            self.status = PlaybackStatus::Playing;
            Some(prev)
        } else {
            self.elapsed_secs = 0;
            self.current_track.clone()
        }
    }

    pub fn toggle_shuffle(&mut self) {
        self.shuffle = !self.shuffle;
    }

    pub fn cycle_repeat(&mut self) {
        self.repeat = self.repeat.next();
    }

    pub fn adjust_volume(&mut self, delta: i8) {
        let new_vol = (self.volume as i16 + delta as i16).clamp(0, 100);
        self.volume = new_vol as u8;
    }

    pub fn tick_second(&mut self) {
        if self.status != PlaybackStatus::Playing {
            return;
        }

        if let Some(ref track) = self.current_track {
            if self.elapsed_secs < track.duration_secs {
                self.elapsed_secs += 1;
            } else {
                self.next_track();
            }
        }
    }

    pub fn progress_ratio(&self) -> f64 {
        match &self.current_track {
            Some(t) if t.duration_secs > 0 => {
                (self.elapsed_secs as f64 / t.duration_secs as f64).clamp(0.0, 1.0)
            }
            _ => 0.0,
        }
    }

    pub fn formatted_progress(&self) -> String {
        let elapsed_m = self.elapsed_secs / 60;
        let elapsed_s = self.elapsed_secs % 60;
        let total = self
            .current_track
            .as_ref()
            .map(|t| t.formatted_duration())
            .unwrap_or_else(|| "0:00".to_string());
        format!("{elapsed_m}:{elapsed_s:02} / {total}")
    }

    pub fn insert_next(&mut self, track: Track) {
        self.queue.insert(0, track);
    }

    pub fn remove_queue_item(&mut self, index: usize) -> Option<Track> {
        if index < self.queue.len() {
            Some(self.queue.remove(index))
        } else {
            None
        }
    }

    pub fn move_queue_item(&mut self, from: usize, to: usize) {
        if from < self.queue.len() && to < self.queue.len() {
            let item = self.queue.remove(from);
            self.queue.insert(to, item);
        }
    }

    pub fn clear_queue(&mut self) {
        self.queue.clear();
    }

    /// Lyrics are populated only when a supported provider supplies them.
    pub fn current_lyrics(&self) -> Vec<LyricLine> {
        Vec::new()
    }

    /// Finds the index of the active lyric line according to elapsed playback time.
    pub fn current_lyric_index(&self) -> Option<usize> {
        let lyrics = self.current_lyrics();
        if lyrics.is_empty() {
            return None;
        }

        let mut current = 0;
        for (i, line) in lyrics.iter().enumerate() {
            if self.elapsed_secs >= line.time_secs {
                current = i;
            } else {
                break;
            }
        }
        Some(current)
    }

    pub fn current_track(&self) -> Option<&Track> {
        self.current_track.as_ref()
    }

    pub fn position_ms(&self) -> u64 {
        self.elapsed_secs * 1000
    }

    pub fn duration_ms(&self) -> u64 {
        self.current_track
            .as_ref()
            .map_or(0, |t| t.duration_secs * 1000)
    }

    pub fn progress(&self) -> f64 {
        self.progress_ratio()
    }

    pub fn active_lyric_index(&self) -> Option<usize> {
        self.current_lyric_index()
    }
}
