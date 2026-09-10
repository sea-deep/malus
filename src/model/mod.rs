//! Domain models for Malus (Apple Music TUI client).
#![allow(dead_code, unused_imports)]

pub mod lyrics;
pub mod player;
pub mod playlist;
pub mod track;

pub use lyrics::{LyricLine, parse_ttml};
pub use player::{PlaybackStatus, PlayerState, RepeatMode};
pub use playlist::{CuratedMix, Library, Playlist, RadioStation};
pub use track::{Album, Artist, AudioFormat, Track};
