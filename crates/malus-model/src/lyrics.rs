//! Pure domain lyrics model.

use serde::{Deserialize, Serialize};

/// Time-synced or unsynced lyrics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Lyrics {
    pub lines: Vec<LyricLine>,
    pub synced: bool,
}

impl Lyrics {
    pub fn new(lines: Vec<LyricLine>, synced: bool) -> Self {
        Self { lines, synced }
    }

    pub fn empty() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// A single line of lyrics with optional start/end timestamps and word-level syllables.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricLine {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub syllables: Option<Vec<LyricSyllable>>,
    /// Singer/agent identifier from TTML `ttm:agent` (e.g. "v1", "v2").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
}

impl LyricLine {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            start_ms: None,
            end_ms: None,
            syllables: None,
            agent: None,
        }
    }

    pub fn with_timing(mut self, start_ms: u64, end_ms: u64) -> Self {
        self.start_ms = Some(start_ms);
        self.end_ms = Some(end_ms);
        self
    }

    pub fn with_syllables(mut self, syllables: Vec<LyricSyllable>) -> Self {
        self.syllables = Some(syllables);
        self
    }
}

/// Syllable / word-level timing within a lyric line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricSyllable {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_ms: Option<u64>,
}

impl LyricSyllable {
    pub fn new(text: impl Into<String>, start_ms: Option<u64>, end_ms: Option<u64>) -> Self {
        Self {
            text: text.into(),
            start_ms,
            end_ms,
        }
    }
}
