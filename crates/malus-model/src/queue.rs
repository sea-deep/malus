//! Pure domain queue snapshot model.

use crate::media::Track;
use serde::{Deserialize, Serialize};

/// An authoritative queue snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Queue {
    pub items: Vec<Track>,
    pub current_index: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autoplay_start_index: Option<usize>,
}

impl Queue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_items(items: Vec<Track>, current_index: Option<usize>) -> Self {
        Self {
            items,
            current_index,
            autoplay_start_index: None,
        }
    }

    pub fn with_autoplay(
        items: Vec<Track>,
        current_index: Option<usize>,
        autoplay_start_index: Option<usize>,
    ) -> Self {
        Self {
            items,
            current_index,
            autoplay_start_index,
        }
    }

    pub fn items(&self) -> &[Track] {
        &self.items
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn current_index(&self) -> Option<usize> {
        self.current_index
    }

    pub fn autoplay_start_index(&self) -> Option<usize> {
        self.autoplay_start_index
    }

    pub fn current_track(&self) -> Option<&Track> {
        self.current_index.and_then(|i| self.items.get(i))
    }
}
