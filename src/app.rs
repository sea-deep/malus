//! Application state. Apple Music owns live playback; the UI owns navigation.
use crate::{
    engine::{EngineCommand, MusicKitEvent},
    model::{Library, PlaybackStatus, PlayerState, Track},
};
use ratcn::{
    runtime::{FocusState, ModalState},
    toast::{Toast, ToasterState},
};
use std::{collections::HashMap, time::Duration};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Screen {
    #[default]
    ListenNow,
    Browse,
    Radio,
    Library,
    AlbumDetail(String),
    ArtistDetail(String),
    PlaylistDetail(String),
    NowPlaying,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LibrarySubTab {
    #[default]
    Songs,
    Albums,
    Artists,
    Playlists,
}
impl LibrarySubTab {
    pub const ALL: [Self; 4] = [Self::Songs, Self::Albums, Self::Artists, Self::Playlists];
    pub const fn label(self) -> &'static str {
        match self {
            Self::Songs => "Songs",
            Self::Albums => "Albums",
            Self::Artists => "Artists",
            Self::Playlists => "Playlists",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Overlay {
    Search,
    Queue,
    Lyrics,
    ContextMenu(String),
    CommandPalette,
    Help,
    Settings,
}
#[derive(Debug, Clone)]
pub struct CommandItem {
    pub title: &'static str,
    pub category: &'static str,
    pub shortcut: Option<&'static str>,
    pub action: Msg,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKind {
    Main,
    Search,
    Queue,
    Commands,
}
#[derive(Debug, Clone, Default)]
pub struct Cursor {
    pub selected: usize,
    pub offset: usize,
}
impl Cursor {
    pub fn start(&self, rows: usize, len: usize) -> usize {
        let rows = rows.max(1);
        self.offset
            .min(self.selected)
            .max(self.selected.saturating_add(1).saturating_sub(rows))
            .min(len.saturating_sub(rows))
    }
    fn move_to(&mut self, index: usize, rows: usize, len: usize) {
        self.selected = index.min(len.saturating_sub(1));
        self.offset = self.start(rows, len);
    }
}
#[derive(Debug, Clone)]
pub enum Msg {
    Navigate(Screen),
    NavigateBack,
    SelectLibrarySubTab(LibrarySubTab),
    OpenAlbum(String),
    OpenArtist(String),
    OpenPlaylist(String),
    OpenOverlay(Overlay),
    CloseOverlay,
    ToggleOverlay(Overlay),
    OpenContextMenu(String),
    PlayTrack(String),
    PlayAlbum(String),
    PlayPlaylist(String),
    PlayStation(String),
    PlayNext(String),
    AddToQueue(String),
    TogglePlay,
    NextTrack,
    PrevTrack,
    ToggleShuffle,
    CycleRepeat,
    VolumeUp,
    VolumeDown,
    SetVolume(u8),
    JumpQueue(usize),
    RemoveFromQueue(usize),
    MoveQueue(usize, usize),
    ClearQueue,
    MoveSelection(ListKind, i32, usize),
    SelectRow(ListKind, usize, usize),
    ActivateRow(ListKind, usize, usize),
    SearchInput(char),
    SearchPaste(String),
    SearchBackspace,
    SearchClear,
    CommandInput(char),
    CommandBackspace,
    ExecuteCommand(usize),
    FocusChanged(FocusState),
    Quit,
    Noop,
    Tick,
    Refresh,
    Reconnect,
    EngineConnected(String),
    EngineDisconnected,
    EngineMusicKitEvent(MusicKitEvent),
    EngineError(String),
    EngineSeek(f64),
}

pub struct AppState {
    pub focus: FocusState,
    pub modals: ModalState,
    pub toasts: ToasterState<'static>,
    pub active_screen: Screen,
    pub nav_history: Vec<Screen>,
    pub active_overlay: Option<Overlay>,
    pub library_subtab: LibrarySubTab,
    pub cursor: Cursor,
    pub overlay_cursor: Cursor,
    pub search_query: String,
    pub search_results: Vec<Track>,
    pub search_loading: bool,
    pub search_error: Option<String>,
    pub command_filter: String,
    pub library: Library,
    pub catalog: Vec<Track>,
    pub resource_tracks: HashMap<String, Track>,
    pub player: PlayerState,
    pub should_quit: bool,
    pub demo: bool,
    pub engine_tx: Option<tokio::sync::mpsc::Sender<EngineCommand>>,
    pub is_authorized: bool,
    pub browser_name: Option<String>,
    pub engine_error: Option<String>,
    pub connecting: bool,
    pub reconnect_requested: bool,
    pub library_loading: bool,
    pub catalog_loading: bool,
    pub now: Duration,
    pub transition_at: Duration,
    pub reduced_motion: bool,
    pub artwork: HashMap<String, crate::ui::artwork::Cover>,
    search_due: Option<Duration>,
    progress_at: Duration,
    progress_secs: f64,
}
impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
impl AppState {
    pub fn new() -> Self {
        Self {
            focus: FocusState::default(),
            modals: ModalState::default(),
            toasts: ToasterState::default(),
            active_screen: Screen::default(),
            nav_history: vec![],
            active_overlay: None,
            library_subtab: LibrarySubTab::Songs,
            cursor: Cursor::default(),
            overlay_cursor: Cursor::default(),
            search_query: String::new(),
            search_results: vec![],
            search_loading: false,
            search_error: None,
            command_filter: String::new(),
            library: Library::default(),
            catalog: vec![],
            resource_tracks: HashMap::new(),
            player: PlayerState::new(),
            should_quit: false,
            demo: false,
            engine_tx: None,
            is_authorized: false,
            browser_name: None,
            engine_error: None,
            connecting: true,
            reconnect_requested: false,
            library_loading: true,
            catalog_loading: false,
            now: Duration::ZERO,
            transition_at: Duration::ZERO,
            reduced_motion: std::env::var_os("MALUS_REDUCED_MOTION").is_some(),
            artwork: HashMap::new(),
            search_due: None,
            progress_at: Duration::ZERO,
            progress_secs: 0.0,
        }
    }
    pub fn demo() -> Self {
        let mut state = Self::new();
        state.demo = true;
        state.connecting = false;
        state.library_loading = false;
        state.library = Library::mock();
        state.library.replace_tracks(state.library.tracks.clone());
        state.catalog = state.library.tracks.clone();
        state
    }
    pub fn find_track(&self, id: &str) -> Option<&Track> {
        self.library
            .tracks
            .iter()
            .chain(&self.search_results)
            .chain(&self.catalog)
            .chain(self.resource_tracks.values())
            .chain(&self.player.queue)
            .chain(self.player.current_track.iter())
            .find(|t| t.id == id || t.catalog_id.as_deref() == Some(id))
    }
    pub fn filtered_search_results(&self) -> Vec<Track> {
        let query = self.search_query.trim().to_lowercase();
        let mut tracks = if query.is_empty() {
            self.library.tracks.clone()
        } else {
            self.library
                .tracks
                .iter()
                .filter(|t| {
                    format!("{} {} {}", t.title, t.artist, t.album)
                        .to_lowercase()
                        .contains(&query)
                })
                .cloned()
                .collect()
        };
        for track in &self.search_results {
            if !tracks
                .iter()
                .any(|t| t.playback_id() == track.playback_id())
            {
                tracks.push(track.clone());
            }
        }
        tracks
    }
    pub fn screen_tracks(&self) -> Vec<Track> {
        let ids = match &self.active_screen {
            Screen::AlbumDetail(id) => self.library.find_album(id).map(|a| &a.track_ids),
            Screen::ArtistDetail(id) => self.library.find_artist(id).map(|a| &a.top_track_ids),
            Screen::PlaylistDetail(id) => self.library.find_playlist(id).map(|a| &a.track_ids),
            _ => None,
        };
        if let Some(ids) = ids {
            return ids
                .iter()
                .filter_map(|id| self.find_track(id).cloned())
                .collect();
        }
        if matches!(self.active_screen, Screen::Browse) {
            return self.catalog.clone();
        }
        self.library.tracks.clone()
    }
    pub fn list_len(&self, kind: ListKind) -> usize {
        match kind {
            ListKind::Search => self.filtered_search_results().len(),
            ListKind::Queue => self.player.queue.len(),
            ListKind::Commands => self.filtered_commands().len(),
            ListKind::Main => match (&self.active_screen, self.library_subtab) {
                (Screen::Library, LibrarySubTab::Albums) => self.library.albums.len(),
                (Screen::Library, LibrarySubTab::Artists) => self.library.artists.len(),
                (Screen::Library, LibrarySubTab::Playlists) => self.library.playlists.len(),
                (Screen::Radio, _) => self.library.radio_stations.len(),
                _ => self.screen_tracks().len(),
            },
        }
    }
    pub fn cursor_for(&self, kind: ListKind) -> &Cursor {
        if kind == ListKind::Main {
            &self.cursor
        } else {
            &self.overlay_cursor
        }
    }
    pub fn selection_action(&self, kind: ListKind) -> Option<Msg> {
        let i = self.cursor_for(kind).selected;
        match kind {
            ListKind::Search => self
                .filtered_search_results()
                .get(i)
                .map(|t| Msg::PlayTrack(t.id.clone())),
            ListKind::Queue => (i < self.player.queue.len()).then_some(Msg::JumpQueue(i)),
            ListKind::Commands => Some(Msg::ExecuteCommand(i)),
            ListKind::Main => match (&self.active_screen, self.library_subtab) {
                (Screen::Library, LibrarySubTab::Albums) => self
                    .library
                    .albums
                    .get(i)
                    .map(|a| Msg::OpenAlbum(a.id.clone())),
                (Screen::Library, LibrarySubTab::Artists) => self
                    .library
                    .artists
                    .get(i)
                    .map(|a| Msg::OpenArtist(a.id.clone())),
                (Screen::Library, LibrarySubTab::Playlists) => self
                    .library
                    .playlists
                    .get(i)
                    .map(|a| Msg::OpenPlaylist(a.id.clone())),
                (Screen::Radio, _) => self
                    .library
                    .radio_stations
                    .get(i)
                    .map(|s| Msg::PlayStation(s.id.clone())),
                _ => self
                    .screen_tracks()
                    .get(i)
                    .map(|t| Msg::PlayTrack(t.id.clone())),
            },
        }
    }
    pub fn navigate_to(&mut self, screen: Screen) {
        self.close_overlay();
        if self.active_screen != screen {
            self.nav_history.push(self.active_screen.clone());
            self.active_screen = screen;
            self.cursor = Cursor::default();
            self.focus = FocusState::intent(["content", "list"]);
            self.transition_at = self.now;
        }
    }
    pub fn navigate_back(&mut self) {
        if self.active_overlay.is_some() {
            self.close_overlay();
        } else if let Some(screen) = self.nav_history.pop() {
            self.active_screen = screen;
            self.cursor = Cursor::default();
            self.transition_at = self.now;
            self.focus = FocusState::intent(["content", "list"]);
        }
    }
    fn close_overlay(&mut self) {
        self.active_overlay = None;
        self.modals.close(&mut self.focus);
    }
    fn open_overlay(&mut self, overlay: Overlay) {
        self.close_overlay();
        self.overlay_cursor = Cursor::default();
        if overlay == Overlay::CommandPalette {
            self.command_filter.clear();
        }
        let _ = self.modals.open("overlay", &mut self.focus);
        self.focus = match overlay {
            Overlay::Search | Overlay::Queue | Overlay::CommandPalette | Overlay::Help => {
                FocusState::intent(["overlay", "list"])
            }
            Overlay::ContextMenu(_) => FocusState::intent(["overlay", "action0"]),
            _ => FocusState::intent(["overlay", "close"]),
        };
        self.active_overlay = Some(overlay);
        self.transition_at = self.now;
    }
    fn send(&mut self, command: EngineCommand) -> bool {
        if let Some(tx) = &self.engine_tx {
            if tx.try_send(command).is_ok() {
                return true;
            }
            self.toasts.push(
                Toast::error("Audio engine is busy or disconnected. Try again."),
                self.now,
            );
        } else if !self.demo {
            self.toasts.push(
                Toast::warning("Audio engine unavailable. Open Settings for details."),
                self.now,
            );
        }
        false
    }
    fn queue_target(&self, index: usize) -> Option<crate::engine::QueueTarget> {
        self.player
            .queue
            .get(index)
            .map(|t| crate::engine::QueueTarget {
                index,
                id: t.id.clone(),
            })
    }
    fn play_ids(&mut self, tracks: Vec<Track>) {
        if self.demo {
            if let Some(first) = tracks.first() {
                self.player.play_track(first.clone());
                self.player.queue = tracks.into_iter().skip(1).collect();
                self.progress_secs = 0.0;
                self.progress_at = self.now;
            }
        } else {
            self.send(EngineCommand::PlayTracks(
                tracks.iter().map(|t| t.playback_id().to_string()).collect(),
            ));
        }
        self.close_overlay();
    }
    fn search_changed(&mut self) {
        self.overlay_cursor = Cursor::default();
        self.search_results.clear();
        self.search_error = None;
        self.search_loading = !self.demo && !self.search_query.trim().is_empty();
        self.search_due = self
            .search_loading
            .then_some(self.now + Duration::from_millis(300));
    }
    pub fn position_secs(&self) -> f64 {
        let extrapolate = if self.player.status == PlaybackStatus::Playing {
            self.now
                .saturating_sub(self.progress_at)
                .as_secs_f64()
                .min(2.0)
        } else {
            0.0
        };
        (self.progress_secs + extrapolate).min(self.player.duration_ms() as f64 / 1000.0)
    }
    pub fn available_commands(&self) -> Vec<CommandItem> {
        [
            (
                "Command palette",
                "Views",
                "Ctrl+K",
                Msg::OpenOverlay(Overlay::CommandPalette),
            ),
            ("Play / pause", "Playback", "Space", Msg::TogglePlay),
            ("Next track", "Playback", "n", Msg::NextTrack),
            ("Previous track", "Playback", "p", Msg::PrevTrack),
            ("Shuffle", "Playback", "s", Msg::ToggleShuffle),
            ("Repeat", "Playback", "r", Msg::CycleRepeat),
            ("Up next", "Views", "q", Msg::ToggleOverlay(Overlay::Queue)),
            ("Lyrics", "Views", "l", Msg::ToggleOverlay(Overlay::Lyrics)),
            (
                "Search Apple Music",
                "Views",
                "/",
                Msg::OpenOverlay(Overlay::Search),
            ),
            (
                "Listen Now",
                "Navigation",
                "1",
                Msg::Navigate(Screen::ListenNow),
            ),
            ("Browse", "Navigation", "2", Msg::Navigate(Screen::Browse)),
            ("Radio", "Navigation", "3", Msg::Navigate(Screen::Radio)),
            ("Library", "Navigation", "4", Msg::Navigate(Screen::Library)),
            (
                "Now playing",
                "Navigation",
                "5",
                Msg::Navigate(Screen::NowPlaying),
            ),
            ("Refresh library", "Library", "Ctrl+R", Msg::Refresh),
            (
                "Settings",
                "Views",
                ",",
                Msg::OpenOverlay(Overlay::Settings),
            ),
            ("Help", "Views", "?", Msg::OpenOverlay(Overlay::Help)),
            ("Quit Malus", "Application", "Ctrl+C", Msg::Quit),
        ]
        .into_iter()
        .map(|(title, category, shortcut, action)| CommandItem {
            title,
            category,
            shortcut: Some(shortcut),
            action,
        })
        .collect()
    }
    pub fn filtered_commands(&self) -> Vec<CommandItem> {
        let q = self.command_filter.trim().to_lowercase();
        self.available_commands()
            .into_iter()
            .filter(|c| {
                format!("{} {}", c.title, c.category)
                    .to_lowercase()
                    .contains(&q)
            })
            .collect()
    }
    pub fn update(&mut self, msg: Msg, now: Duration) {
        self.now = now;
        match msg {
            Msg::FocusChanged(f) => self.focus = f,
            Msg::Quit => self.should_quit = true,
            Msg::Navigate(screen) => {
                if screen == Screen::Browse
                    && self.catalog.is_empty()
                    && !self.demo
                    && !self.catalog_loading
                {
                    self.catalog_loading = self.send(EngineCommand::FetchCatalog);
                }
                if screen == Screen::Radio && self.library.radio_stations.is_empty() && !self.demo {
                    self.send(EngineCommand::FetchRadio);
                }
                self.navigate_to(screen);
            }
            Msg::NavigateBack => self.navigate_back(),
            Msg::SelectLibrarySubTab(tab) => {
                self.library_subtab = tab;
                self.cursor = Cursor::default();
            }
            Msg::OpenAlbum(id) => self.navigate_to(Screen::AlbumDetail(id)),
            Msg::OpenArtist(id) => self.navigate_to(Screen::ArtistDetail(id)),
            Msg::OpenPlaylist(id) => {
                if !self.demo {
                    self.send(EngineCommand::FetchPlaylist(id.clone()));
                }
                self.navigate_to(Screen::PlaylistDetail(id));
            }
            Msg::OpenOverlay(o) => self.open_overlay(o),
            Msg::CloseOverlay => self.close_overlay(),
            Msg::ToggleOverlay(o) => {
                if self.active_overlay.as_ref() == Some(&o) {
                    self.close_overlay()
                } else {
                    self.open_overlay(o)
                }
            }
            Msg::OpenContextMenu(id) => self.open_overlay(Overlay::ContextMenu(id)),
            Msg::PlayTrack(id) => {
                if let Some(t) = self.find_track(&id).cloned() {
                    self.play_ids(vec![t]);
                }
            }
            Msg::PlayAlbum(id) => {
                if !self.demo && id.starts_with("l.") {
                    self.send(EngineCommand::PlayAlbum(id));
                    return;
                }
                let tracks = self
                    .library
                    .find_album(&id)
                    .map(|a| {
                        a.track_ids
                            .iter()
                            .filter_map(|id| self.find_track(id).cloned())
                            .collect()
                    })
                    .unwrap_or_default();
                self.play_ids(tracks);
            }
            Msg::PlayPlaylist(id) => {
                if self.demo {
                    let tracks = self
                        .library
                        .find_playlist(&id)
                        .map(|p| {
                            p.track_ids
                                .iter()
                                .filter_map(|id| self.find_track(id).cloned())
                                .collect()
                        })
                        .unwrap_or_default();
                    self.play_ids(tracks);
                } else {
                    self.send(EngineCommand::PlayPlaylist(id));
                }
            }
            Msg::PlayStation(id) => {
                if !self.demo {
                    self.send(EngineCommand::PlayStation(id));
                }
            }
            Msg::PlayNext(ref id) | Msg::AddToQueue(ref id) => {
                let next = matches!(msg, Msg::PlayNext(_));
                if let Some(t) = self.find_track(id).cloned() {
                    if self.demo {
                        if next {
                            self.player.queue.insert(0, t);
                        } else {
                            self.player.queue.push(t);
                        }
                    } else {
                        self.send(EngineCommand::Enqueue(t.playback_id().to_string(), next));
                    }
                }
                self.close_overlay();
            }
            Msg::TogglePlay => {
                if self.demo {
                    self.player.toggle_play();
                    self.progress_at = now;
                    self.progress_secs = self.player.elapsed_secs as f64;
                } else {
                    self.send(EngineCommand::TogglePlay);
                }
            }
            Msg::NextTrack => {
                if self.demo {
                    self.player.next_track();
                    self.progress_at = now;
                    self.progress_secs = 0.0;
                } else {
                    self.send(EngineCommand::Next);
                }
            }
            Msg::PrevTrack => {
                if self.demo {
                    self.player.prev_track();
                    self.progress_at = now;
                    self.progress_secs = 0.0;
                } else {
                    self.send(EngineCommand::Prev);
                }
            }
            Msg::ToggleShuffle => {
                if self.demo {
                    self.player.toggle_shuffle();
                } else {
                    self.send(EngineCommand::SetShuffle(!self.player.shuffle));
                }
            }
            Msg::CycleRepeat => {
                if self.demo {
                    self.player.cycle_repeat();
                } else {
                    self.send(EngineCommand::SetRepeat(self.player.repeat.next()));
                }
            }
            Msg::VolumeUp => self.update(
                Msg::SetVolume(self.player.volume.saturating_add(5).min(100)),
                now,
            ),
            Msg::VolumeDown => {
                self.update(Msg::SetVolume(self.player.volume.saturating_sub(5)), now)
            }
            Msg::SetVolume(v) => {
                self.player.volume = v.min(100);
                if !self.demo {
                    self.send(EngineCommand::SetVolume(self.player.volume as f64 / 100.0));
                }
            }
            Msg::EngineSeek(s) => {
                if s.is_finite() {
                    let s = s.clamp(0.0, self.player.duration_ms() as f64 / 1000.0);
                    if self.demo {
                        self.progress_secs = s;
                        self.progress_at = now;
                        self.player.elapsed_secs = s as u64;
                    } else {
                        self.send(EngineCommand::Seek(s));
                    }
                }
            }
            Msg::JumpQueue(i) => {
                if self.demo {
                    if i < self.player.queue.len() {
                        let t = self.player.queue.remove(i);
                        self.player.play_track(t);
                        self.progress_secs = 0.0;
                        self.progress_at = now;
                    }
                } else {
                    if let Some(target) = self.queue_target(i) {
                        self.send(EngineCommand::JumpQueue(target));
                    }
                }
            }
            Msg::RemoveFromQueue(i) => {
                if self.demo {
                    self.player.remove_queue_item(i);
                } else {
                    if let Some(target) = self.queue_target(i) {
                        self.send(EngineCommand::RemoveQueue(target));
                    }
                }
            }
            Msg::MoveQueue(a, b) => {
                if self.active_overlay == Some(Overlay::Queue) {
                    self.overlay_cursor.selected = b.min(self.player.queue.len().saturating_sub(1));
                }
                if self.demo {
                    self.player.move_queue_item(a, b);
                } else {
                    if let (Some(a), Some(b)) = (self.queue_target(a), self.queue_target(b)) {
                        self.send(EngineCommand::MoveQueue(a, b));
                    }
                }
            }
            Msg::ClearQueue => {
                if self.demo {
                    self.player.clear_queue();
                } else {
                    self.send(EngineCommand::ClearQueue);
                }
            }
            Msg::MoveSelection(kind, delta, rows) => {
                let len = self.list_len(kind);
                let c = if kind == ListKind::Main {
                    &mut self.cursor
                } else {
                    &mut self.overlay_cursor
                };
                c.move_to(c.selected.saturating_add_signed(delta as isize), rows, len);
            }
            Msg::SelectRow(kind, i, rows) => {
                let len = self.list_len(kind);
                let c = if kind == ListKind::Main {
                    &mut self.cursor
                } else {
                    &mut self.overlay_cursor
                };
                c.move_to(i, rows, len);
            }
            Msg::ActivateRow(kind, i, rows) => {
                self.update(Msg::SelectRow(kind, i, rows), now);
                if let Some(action) = self.selection_action(kind) {
                    self.update(action, now);
                }
            }
            Msg::SearchInput(ch) => {
                if !ch.is_control() {
                    self.search_query.push(ch);
                    self.search_changed();
                }
            }
            Msg::SearchPaste(text) => {
                self.search_query
                    .extend(text.chars().filter(|c| !c.is_control()).take(512));
                self.search_changed();
            }
            Msg::SearchBackspace => {
                self.search_query.pop();
                self.search_changed();
            }
            Msg::SearchClear => {
                self.search_query.clear();
                self.search_changed();
            }
            Msg::CommandInput(ch) => {
                if !ch.is_control() {
                    self.command_filter.push(ch);
                    self.overlay_cursor = Cursor::default();
                }
            }
            Msg::CommandBackspace => {
                self.command_filter.pop();
                self.overlay_cursor = Cursor::default();
            }
            Msg::ExecuteCommand(i) => {
                if let Some(cmd) = self.filtered_commands().get(i) {
                    let action = cmd.action.clone();
                    self.close_overlay();
                    self.update(action, now);
                }
            }
            Msg::Reconnect => {
                if !self.demo {
                    self.reconnect_requested = true;
                }
            }
            Msg::Refresh => {
                if !self.demo && self.engine_tx.is_none() {
                    self.reconnect_requested = true;
                } else if !self.demo {
                    self.library_loading = self.send(EngineCommand::FetchLibrary);
                }
            }
            Msg::Tick => {
                if self.search_due.is_some_and(|d| now >= d) {
                    self.search_due = None;
                    self.search_loading =
                        self.send(EngineCommand::Search(self.search_query.trim().to_string()));
                }
                if self.demo
                    && self.player.status == PlaybackStatus::Playing
                    && now.saturating_sub(self.progress_at) >= Duration::from_secs(1)
                {
                    self.player.elapsed_secs = self.position_secs() as u64;
                    self.progress_secs = self.player.elapsed_secs as f64;
                    self.progress_at = now;
                }
            }
            Msg::EngineConnected(name) => {
                self.browser_name = Some(name);
                self.connecting = false;
                self.engine_error = None;
            }
            Msg::EngineDisconnected => {
                self.reconnect_requested = true;
                self.engine_tx = None;
                self.browser_name = None;
                self.connecting = false;
                self.library_loading = false;
                self.player.status = PlaybackStatus::Stopped;
                self.engine_error = Some("Connection lost. Reconnecting…".into());
            }
            Msg::EngineError(e) => {
                self.connecting = false;
                self.library_loading = false;
                self.catalog_loading = false;
                self.search_loading = false;
                self.engine_error = Some(e.clone());
                self.toasts.push(Toast::error(e), now);
            }
            Msg::EngineMusicKitEvent(event) => self.apply_engine_event(event),
            Msg::Noop => {}
        }
    }
    fn apply_engine_event(&mut self, event: MusicKitEvent) {
        match event {
            MusicKitEvent::AuthStatus { is_authorized } => {
                let changed = self.is_authorized != is_authorized;
                self.is_authorized = is_authorized;
                if is_authorized && changed {
                    self.library_loading = self.send(EngineCommand::FetchLibrary);
                } else if !is_authorized {
                    self.library_loading = false;
                }
            }
            MusicKitEvent::PlaybackState {
                is_playing,
                raw_state,
            } => {
                self.progress_secs = self.position_secs();
                self.progress_at = self.now;
                self.player.status = if matches!(raw_state, Some(0 | 4 | 5 | 10)) {
                    PlaybackStatus::Stopped
                } else if is_playing {
                    PlaybackStatus::Playing
                } else {
                    PlaybackStatus::Paused
                };
            }
            MusicKitEvent::PlaybackTime {
                current_playback_time,
                current_playback_duration,
            } => {
                if current_playback_time.is_finite() {
                    self.progress_secs = current_playback_time.max(0.0);
                    self.progress_at = self.now;
                    self.player.elapsed_secs = self.progress_secs as u64;
                }
                if let Some(dur) = current_playback_duration.filter(|d| d.is_finite() && *d > 0.0)
                    && let Some(t) = &mut self.player.current_track
                {
                    t.duration_secs = dur as u64;
                }
            }
            MusicKitEvent::NowPlaying {
                id,
                title,
                artist_name,
                album_name,
                artwork_url,
                duration,
            } => {
                if self
                    .player
                    .current_track
                    .as_ref()
                    .is_none_or(|t| t.id != id)
                {
                    if let Some(t) = self.player.current_track.take() {
                        self.player.history.push(t);
                    }
                    self.progress_secs = 0.0;
                    self.progress_at = self.now;
                    self.player.elapsed_secs = 0;
                }
                let mut track = self.find_track(&id).cloned().unwrap_or_else(|| {
                    Track::new(
                        &id,
                        &title,
                        &artist_name,
                        album_name.clone().unwrap_or_default(),
                        duration.unwrap_or(0.0) as u64,
                        1,
                        crate::model::AudioFormat::Standard,
                    )
                });
                track.id = id;
                track.title = title;
                track.artist = artist_name;
                track.artwork_url = artwork_url.or(track.artwork_url);
                self.player.current_track = Some(track);
            }
            MusicKitEvent::LibraryLoaded { tracks } => {
                self.library.replace_tracks(tracks);
                self.library_loading = false;
                self.engine_error = None;
                self.cursor.selected = self
                    .cursor
                    .selected
                    .min(self.list_len(ListKind::Main).saturating_sub(1));
            }
            MusicKitEvent::SearchResults { query, tracks } => {
                if query == self.search_query.trim() {
                    self.search_results = tracks;
                    self.search_loading = false;
                }
            }
            MusicKitEvent::CatalogLoaded { tracks } => {
                self.catalog = tracks;
                self.catalog_loading = false;
            }
            MusicKitEvent::PlaylistsLoaded { playlists } => self.library.playlists = playlists,
            MusicKitEvent::PlaylistLoaded { id, tracks } => {
                if let Some(p) = self.library.playlists.iter_mut().find(|p| p.id == id) {
                    p.track_ids = tracks.iter().map(|t| t.id.clone()).collect();
                }
                for t in tracks {
                    if self.find_track(&t.id).is_none() {
                        self.resource_tracks.insert(t.id.clone(), t);
                    }
                }
            }
            MusicKitEvent::RadioLoaded { stations } => self.library.radio_stations = stations,
            MusicKitEvent::QueueChanged {
                tracks,
                shuffle,
                repeat,
                volume,
            } => {
                self.player.queue = tracks;
                self.player.shuffle = shuffle;
                self.player.repeat = match repeat {
                    1 => crate::model::RepeatMode::One,
                    2 => crate::model::RepeatMode::All,
                    _ => crate::model::RepeatMode::Off,
                };
                if volume.is_finite() {
                    self.player.volume = (volume.clamp(0.0, 1.0) * 100.0).round() as u8;
                }
                if self.active_overlay == Some(Overlay::Queue) {
                    self.overlay_cursor.selected = self
                        .overlay_cursor
                        .selected
                        .min(self.player.queue.len().saturating_sub(1));
                }
            }
            MusicKitEvent::Error { message } => {
                self.toasts.push(Toast::error(message), self.now);
            }
            MusicKitEvent::RequestFailed { request, message } => {
                use crate::engine::musickit::DataRequest;
                match request {
                    DataRequest::Search(q) => {
                        if q != self.search_query.trim() {
                            return;
                        }
                        self.search_loading = false;
                        self.search_error = Some(message.clone());
                    }
                    DataRequest::Library => {
                        self.library_loading = false;
                        self.engine_error = Some(message.clone());
                    }
                    DataRequest::Catalog => {
                        self.catalog_loading = false;
                        self.engine_error = Some(message.clone());
                    }
                    _ => {}
                }
                self.toasts.push(Toast::error(message), self.now);
            }
            MusicKitEvent::Disconnected => self.update(Msg::EngineDisconnected, self.now),
        }
    }
}
