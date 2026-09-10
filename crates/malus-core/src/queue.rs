//! Pure domain queue manipulation.

use crate::track::Track;

/// An ordered playback queue with an active index.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Queue {
    items: Vec<Track>,
    current_index: Option<usize>,
}

impl Queue {
    pub fn new() -> Self {
        Self::default()
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

    pub fn current_track(&self) -> Option<&Track> {
        self.current_index.and_then(|i| self.items.get(i))
    }

    pub fn enqueue(&mut self, track: Track) {
        self.items.push(track);
        if self.current_index.is_none() {
            self.current_index = Some(0);
        }
    }

    pub fn enqueue_next(&mut self, track: Track) {
        if let Some(idx) = self.current_index {
            self.items.insert(idx + 1, track);
        } else {
            self.items.push(track);
            self.current_index = Some(0);
        }
    }

    pub fn set_items(&mut self, items: Vec<Track>, current_index: Option<usize>) {
        self.items = items;
        self.current_index = if self.items.is_empty() {
            None
        } else {
            current_index.map(|i| i.min(self.items.len() - 1))
        };
    }

    pub fn remove(&mut self, index: usize) -> Option<Track> {
        if index >= self.items.len() {
            return None;
        }
        let removed = self.items.remove(index);
        if self.items.is_empty() {
            self.current_index = None;
        } else if let Some(cur) = self.current_index {
            if index < cur {
                self.current_index = Some(cur - 1);
            } else if cur >= self.items.len() {
                self.current_index = Some(self.items.len() - 1);
            }
        }
        Some(removed)
    }

    pub fn move_item(&mut self, from: usize, to: usize) -> bool {
        let len = self.items.len();
        if from >= len || to >= len || from == to {
            return false;
        }
        let item = self.items.remove(from);
        self.items.insert(to, item);

        if let Some(cur) = self.current_index {
            if cur == from {
                self.current_index = Some(to);
            } else if from < cur && to >= cur {
                self.current_index = Some(cur - 1);
            } else if from > cur && to <= cur {
                self.current_index = Some(cur + 1);
            }
        }
        true
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<&Track> {
        if self.items.is_empty() {
            return None;
        }
        match self.current_index {
            Some(i) if i + 1 < self.items.len() => {
                self.current_index = Some(i + 1);
            }
            _ => self.current_index = None,
        }
        self.current_track()
    }

    pub fn previous(&mut self) -> Option<&Track> {
        if self.items.is_empty() {
            return None;
        }
        match self.current_index {
            Some(i) if i > 0 => {
                self.current_index = Some(i - 1);
            }
            _ => {
                self.current_index = Some(0);
            }
        }
        self.current_track()
    }

    pub fn jump(&mut self, index: usize) -> Option<&Track> {
        if index < self.items.len() {
            self.current_index = Some(index);
            self.current_track()
        } else {
            None
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.current_index = None;
    }
}
