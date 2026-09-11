//! Pure domain models and logic for malus-gui.
//!
//! Contains zero GTK / UI code. 100% testable and decoupled.

use malus_client::{AuthStateWire, PlaybackStateWire, ProviderInfoWire, TrackWire};
use std::collections::HashMap;
use std::time::Instant;

/// Active navigation destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    Search,
    Library(LibraryTab),
    AlbumDetail(String),
    ArtistDetail(String),
    PlaylistDetail(String),
}

/// Active library tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LibraryTab {
    #[default]
    Tracks,
    Albums,
    Playlists,
}

/// Stack-based navigation history for browser-like back/forward traversal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavigationHistory {
    current: Route,
    history: Vec<Route>,
    forward_history: Vec<Route>,
}

impl Default for NavigationHistory {
    fn default() -> Self {
        Self {
            current: Route::Search,
            history: Vec::new(),
            forward_history: Vec::new(),
        }
    }
}

impl NavigationHistory {
    pub fn new(initial: Route) -> Self {
        Self {
            current: initial,
            history: Vec::new(),
            forward_history: Vec::new(),
        }
    }

    pub fn current(&self) -> &Route {
        &self.current
    }

    pub fn can_go_back(&self) -> bool {
        !self.history.is_empty()
    }

    pub fn can_go_forward(&self) -> bool {
        !self.forward_history.is_empty()
    }

    pub fn navigate_to(&mut self, route: Route) {
        if self.current != route {
            self.history.push(self.current.clone());
            self.forward_history.clear();
            self.current = route;
        }
    }

    pub fn go_back(&mut self) -> Option<&Route> {
        if let Some(prev) = self.history.pop() {
            self.forward_history.push(self.current.clone());
            self.current = prev;
            Some(&self.current)
        } else {
            None
        }
    }

    pub fn go_forward(&mut self) -> Option<&Route> {
        if let Some(next) = self.forward_history.pop() {
            self.history.push(self.current.clone());
            self.current = next;
            Some(&self.current)
        } else {
            None
        }
    }
}

/// Authoritative search session owned by MalusApp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchSession {
    pub query: String,
    pub provider: String,
    pub generation: u64,
}

impl SearchSession {
    pub fn new(provider: impl Into<String>) -> Self {
        Self {
            query: String::new(),
            provider: provider.into(),
            generation: 0,
        }
    }

    pub fn set_query(&mut self, query: String) -> u64 {
        self.query = query;
        self.generation = self.generation.wrapping_add(1);
        self.generation
    }

    pub fn set_provider(&mut self, provider: String) -> u64 {
        self.provider = provider;
        self.generation = self.generation.wrapping_add(1);
        self.generation
    }
}

/// Presentation model for player state with monotonic extrapolation.
#[derive(Debug, Clone)]
pub struct PlayerModel {
    pub state: PlaybackStateWire,
    pub current_track: Option<TrackWire>,
    pub position_ms: u64,
    pub duration_ms: Option<u64>,
    pub last_update: Option<Instant>,
    pub playback_provider: Option<String>,
}

impl Default for PlayerModel {
    fn default() -> Self {
        Self {
            state: PlaybackStateWire::Stopped,
            current_track: None,
            position_ms: 0,
            duration_ms: None,
            last_update: None,
            playback_provider: None,
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
    ) {
        // Derive playback provider from track ID namespace (e.g. "apple:track:123" -> "apple")
        if let Some(ref track) = current_track {
            let parts: Vec<&str> = track.id.split(':').collect();
            if !parts.is_empty() && !parts[0].is_empty() {
                self.playback_provider = Some(parts[0].to_string());
            }
        }

        self.state = state;
        self.current_track = current_track;
        self.position_ms = position_ms;
        self.duration_ms = duration_ms;
        self.last_update = if state == PlaybackStateWire::Playing {
            Some(Instant::now())
        } else {
            None
        };
    }

    /// Monotonically extrapolated playback position in milliseconds.
    /// Freezes when paused or stopped. Clamps to duration if known.
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
}

/// Cached provider registry and capability index.
#[derive(Debug, Clone, Default)]
pub struct ProviderCache {
    providers: HashMap<String, ProviderInfoWire>,
    order: Vec<String>,
}

impl ProviderCache {
    pub fn set_providers(&mut self, list: Vec<ProviderInfoWire>) {
        self.providers.clear();
        self.order.clear();
        for p in list {
            self.order.push(p.id.clone());
            self.providers.insert(p.id.clone(), p);
        }
    }

    pub fn get(&self, id: &str) -> Option<&ProviderInfoWire> {
        self.providers.get(id)
    }

    pub fn list(&self) -> Vec<&ProviderInfoWire> {
        self.order
            .iter()
            .filter_map(|id| self.providers.get(id))
            .collect()
    }

    pub fn has_capability(&self, provider_id: &str, capability: &str) -> bool {
        self.providers
            .get(provider_id)
            .map(|p| p.capabilities.iter().any(|c| c == capability))
            .unwrap_or(false)
    }

    pub fn is_authenticated(&self, provider_id: &str) -> bool {
        self.providers
            .get(provider_id)
            .and_then(|p| p.auth_state)
            .map(|a| a == AuthStateWire::Authenticated)
            .unwrap_or(false)
    }

    pub fn auth_state(&self, provider_id: &str) -> Option<AuthStateWire> {
        self.providers.get(provider_id).and_then(|p| p.auth_state)
    }
}

/// Formats milliseconds into `M:SS` or `MM:SS`.
pub fn format_time(ms: u64) -> String {
    let total_secs = ms / 1000;
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    format!("{mins}:{secs:02}")
}
