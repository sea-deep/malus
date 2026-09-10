//! Pure domain playback state enums.

/// High-level playback status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum PlaybackState {
    #[default]
    Stopped,
    Playing,
    Paused,
}

impl PlaybackState {
    pub fn is_playing(&self) -> bool {
        matches!(self, Self::Playing)
    }
}

/// Loop / repeat options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum RepeatMode {
    #[default]
    Off,
    Track,
    All,
}

impl RepeatMode {
    pub fn cycle(&self) -> Self {
        match self {
            Self::Off => Self::All,
            Self::All => Self::Track,
            Self::Track => Self::Off,
        }
    }
}
