//! Universal Apple Page and Feed component.
//!
//! Renders any canonical `PageWire`:
//! - Home (Listen Now), Browse (New), Radio, Replay
//! - Album Detail, Artist Detail, Playlist Detail
//! - Library sections (Recently Added, Albums, Artists, Songs, Playlists)
//!
//! Features:
//! - Hero header for albums/playlists/artists with metadata & action buttons
//! - Horizontal shelves for discovery sections
//! - Responsive FlowBox grids for albums and playlists
//! - Dense track list with duration and action popovers
//! - Clean loading spinner and error recovery

mod rendering;
mod sections;
use rendering::artist_route_id;

use malus_client::MalusClient;
use malus_ipc::wire::{
    PageActionWire, PageContinuationWire, PageCursorWire, PageHeaderWire, PageItemWire,
    PageSectionWire, PageWire,
};
use malus_model::{MediaRef, PageRoute, Track};
use relm4::adw::{self, prelude::*};
use relm4::gtk::{self, glib};
use relm4::prelude::*;

use crate::design::tokens::*;
use crate::services::{ArtworkService, bind_artwork};
use crate::widgets::media_card::{MediaCard, MediaCardInit, MediaCardInput, MediaCardOutput};
use crate::widgets::media_shelf::{create_shelf_container, hook_shelf_artwork_trigger};
use crate::widgets::track_row::{TrackRow, TrackRowInit, TrackRowOutput};
use crate::widgets::virtual_track_list::VirtualTrackList;

use crate::widgets::media_shelf::ShelfSenders;
use crate::widgets::{
    FeaturedBannerCard, FeaturedBannerCardInit, FeaturedBannerCardOutput, LiveStationPill,
    LiveStationPillInit, LiveStationPillOutput, MultiRowEpisodeRow, MultiRowEpisodeRowInit,
    MultiRowEpisodeRowOutput, MultiRowTrackRow, MultiRowTrackRowInit, MultiRowTrackRowOutput,
    StationCard, StationCardInit, StationCardOutput,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibrarySortMethod {
    RecentlyAdded,
    Title,
    Artist,
}

pub(crate) type SectionSpinner = std::rc::Rc<std::cell::RefCell<Option<gtk::Box>>>;
pub(crate) type DeferredSections =
    std::rc::Rc<std::cell::RefCell<Vec<(gtk::Widget, Vec<relm4::Sender<MediaCardInput>>)>>>;

#[derive(Clone)]
pub(crate) enum MountedSection {
    TrackList {
        track_list: gtk::Box,
        spinner: SectionSpinner,
        playlist_ctx: Option<MediaRef>,
        collection_ref: Option<MediaRef>,
        show_artwork: bool,
    },
    Grid {
        flow: gtk::FlowBox,
        spinner_parent: gtk::Box,
        spinner: SectionSpinner,
    },
    Shelf {
        shelf_box: gtk::Box,
        card_size: i32,
        is_artist: bool,
        shelf_senders: ShelfSenders,
    },
}

pub struct FeedPage {
    client: MalusClient,
    artwork_service: ArtworkService,
    route: PageRoute,
    generation: u64,
    artist_generation: u64,
    artist_error: Option<String>,
    continuation_error: Option<String>,
    page_data: Option<PageWire>,
    header_actions_host: Option<gtk::Box>,
    current_sort: LibrarySortMethod,
    is_loading: bool,
    error_message: Option<String>,
    card_controllers: Vec<Controller<MediaCard>>,
    track_controllers: Vec<Controller<TrackRow>>,
    station_card_controllers: Vec<Controller<StationCard>>,
    live_station_controllers: Vec<Controller<LiveStationPill>>,
    featured_banner_controllers: Vec<Controller<FeaturedBannerCard>>,
    compact_track_controllers: Vec<Controller<MultiRowTrackRow>>,
    compact_episode_controllers: Vec<Controller<MultiRowEpisodeRow>>,
    selected_artist_id: Option<String>,
    artist_detail: Option<PageWire>,
    show_artist_detail: bool,
    artist_detail_loading: bool,
    is_continuing: bool,
    mounted_sections: std::collections::HashMap<String, MountedSection>,
    deferred_sections: DeferredSections,
    active_artist_list: Option<gtk::Box>,
    active_artist_spinner: Option<gtk::Box>,
    virtual_track_list: Option<VirtualTrackList>,
    library_subtitle_lbl: Option<gtk::Label>,
    artist_search_query: String,
    selected_genre: Option<String>,
    genre_search_query: String,
    show_genre_detail: bool,
    pub(super) active_artist_split: Option<adw::OverlaySplitView>,
    pub(super) active_artist_back_box: std::rc::Rc<std::cell::RefCell<Option<gtk::Box>>>,
    pub(super) show_artist_detail_cell: Option<std::rc::Rc<std::cell::Cell<bool>>>,
    pub(super) active_genre_split: Option<adw::OverlaySplitView>,
    pub(super) active_genre_list: Option<gtk::Box>,
    pub(super) active_genre_back_box: std::rc::Rc<std::cell::RefCell<Option<gtk::Box>>>,
    pub(super) show_genre_detail_cell: Option<std::rc::Rc<std::cell::Cell<bool>>>,
    pub(super) page_cache: crate::services::PageCache,
    pub(super) skeleton_route: Option<PageRoute>,
}

#[derive(Debug)]
pub enum FeedInput {
    MediaState(malus_model::AccountMediaState),
    LoadRoute(PageRoute),
    ChangeSort(LibrarySortMethod),
    SelectArtist(String),
    ArtistSearchChanged(String),
    BackToArtistList,
    SelectGenre(Option<String>),
    GenreSearchChanged(String),
    BackToGenreList,
    Reload,
    InvalidateRoute(PageRoute),
    RetryContinuation,
}

#[derive(Debug)]
pub enum FeedCmd {
    HeaderStateLoaded {
        generation: u64,
        state: Option<malus_model::AccountMediaState>,
    },
    PageLoaded {
        generation: u64,
        route: PageRoute,
        result: Result<PageWire, String>,
    },
    ArtistDetailLoaded {
        generation: u64,
        artist_id: String,
        result: Result<PageWire, String>,
    },
    SectionContinued {
        generation: u64,
        route: PageRoute,
        section_id: String,
        result: Result<PageContinuationWire, String>,
    },
}

#[derive(Debug, Clone)]
pub enum FeedOutput {
    Navigate(PageRoute),
    Play(MediaRef),
    PlayTrack {
        track: MediaRef,
        collection: Option<MediaRef>,
        index: Option<usize>,
    },
    PlayCollection {
        reference: MediaRef,
        shuffle: bool,
    },
    Action(PageActionWire),
    ViewCredits(MediaRef),
    ShowAddToPlaylist(MediaRef),
    ShowNewPlaylistDialog {
        initial_track: Option<MediaRef>,
    },
    ShowEditPlaylistDialog {
        playlist_ref: MediaRef,
        current_name: String,
        current_desc: Option<String>,
    },
    ShowDeletePlaylistDialog {
        playlist_ref: MediaRef,
        playlist_title: String,
    },
    RemoveTrackFromPlaylist {
        playlist: MediaRef,
        track_index: usize,
        expected_track: MediaRef,
    },
    CopyLink(String),
}

#[relm4::component(pub)]
impl Component for FeedPage {
    type Init = (MalusClient, ArtworkService, PageRoute);
    type Input = FeedInput;
    type Output = FeedOutput;
    type CommandOutput = FeedCmd;

    view! {
        #[name(scrolled_window)]
        gtk::ScrolledWindow {
            set_hscrollbar_policy: gtk::PolicyType::Never,
            set_vscrollbar_policy: gtk::PolicyType::Automatic,
            set_vexpand: true,
            set_hexpand: true,
            add_css_class: "feed-scrolled-window",

            #[name(main_box)]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
                add_css_class: "feed-page-content",
                set_margin_start: PAGE_PADDING_NORMAL,
                set_margin_end: PAGE_PADDING_NORMAL,
                set_margin_top: 24,
                set_margin_bottom: 48,

                // Skeleton Container
                #[name(skeleton_container)]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 24,
                    #[watch]
                    set_visible: model.is_loading,
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 12,
                    set_halign: gtk::Align::Center,
                    set_margin_top: 32,
                    set_margin_bottom: 32,
                    add_css_class: "status-surface",
                    #[watch]
                    set_visible: model.error_message.is_some() && !model.is_loading,
                    gtk::Image { set_icon_name: Some("network-error-symbolic"), set_pixel_size: 40 },
                    gtk::Label {
                        set_text: "Unable to load this page",
                        add_css_class: "section-title",
                    },
                    gtk::Label {
                        set_wrap: true,
                        set_max_width_chars: 48,
                        set_justify: gtk::Justification::Center,
                        add_css_class: "page-subtitle",
                        #[watch]
                        set_text: model.error_message.as_deref().unwrap_or(""),
                    },
                    gtk::Button {
                        set_label: "Try Again",
                        set_halign: gtk::Align::Center,
                        add_css_class: "suggested-action",
                        connect_clicked => FeedInput::Reload,
                    },
                },
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 8,
                    #[watch]
                    set_visible: model.continuation_error.is_some(),
                    gtk::Label {
                        set_wrap: true,
                        #[watch]
                        set_text: model.continuation_error.as_deref().unwrap_or(""),
                    },
                    gtk::Button {
                        set_label: "Retry Loading More",
                        set_halign: gtk::Align::Center,
                        connect_clicked => FeedInput::RetryContinuation,
                    },
                },

                // Dynamic Sections Container
                #[name(sections_container)]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 24,
                    #[watch]
                    set_visible: !model.is_loading && model.page_data.is_some(),
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let (client, artwork_service, route) = init;
        let deferred_sections = std::rc::Rc::new(std::cell::RefCell::new(Vec::<(
            gtk::Widget,
            Vec<relm4::Sender<MediaCardInput>>,
        )>::new()));

        let model = Self {
            client: client.clone(),
            artwork_service,
            route: route.clone(),
            generation: 0,
            artist_generation: 0,
            artist_error: None,
            continuation_error: None,
            page_data: None,
            header_actions_host: None,
            current_sort: LibrarySortMethod::RecentlyAdded,
            is_loading: true,
            error_message: None,
            card_controllers: Vec::new(),
            track_controllers: Vec::new(),
            station_card_controllers: Vec::new(),
            live_station_controllers: Vec::new(),
            featured_banner_controllers: Vec::new(),
            compact_track_controllers: Vec::new(),
            compact_episode_controllers: Vec::new(),
            selected_artist_id: None,
            artist_detail: None,
            show_artist_detail: false,
            artist_detail_loading: false,
            is_continuing: false,
            mounted_sections: std::collections::HashMap::new(),
            deferred_sections: deferred_sections.clone(),
            active_artist_list: None,
            active_artist_spinner: None,
            virtual_track_list: None,
            library_subtitle_lbl: None,
            artist_search_query: String::new(),
            selected_genre: None,
            genre_search_query: String::new(),
            show_genre_detail: false,
            active_artist_split: None,
            active_artist_back_box: std::rc::Rc::new(std::cell::RefCell::new(None)),
            show_artist_detail_cell: None,
            active_genre_split: None,
            active_genre_list: None,
            active_genre_back_box: std::rc::Rc::new(std::cell::RefCell::new(None)),
            show_genre_detail_cell: None,
            page_cache: crate::services::PageCache::new(),
            skeleton_route: Some(route.clone()),
        };

        let widgets = view_output!();
        let skeleton = crate::widgets::skeleton::build_route_skeleton(&model.route);
        widgets.skeleton_container.append(&skeleton);

        let content = widgets.main_box.clone();
        let last_narrow = std::cell::Cell::new(None::<bool>);
        root.add_tick_callback(move |page, _| {
            let w = page.width();
            if w > 0 {
                let narrow = w < 600;
                if last_narrow.get() != Some(narrow) {
                    last_narrow.set(Some(narrow));
                    let padding = if narrow {
                        PAGE_PADDING_NARROW
                    } else {
                        PAGE_PADDING_NORMAL
                    };
                    content.set_margin_start(padding);
                    content.set_margin_end(padding);
                }
            }
            gtk::glib::ControlFlow::Continue
        });

        // Vertical scroll watcher: loads deferred sections as they approach the viewport
        let vadj = widgets.scrolled_window.vadjustment();
        let sections_container = widgets.sections_container.clone();
        let deferred_clone = deferred_sections;
        let check_vertical_sections = move |adj: &gtk::Adjustment| {
            let mut deferred = deferred_clone.borrow_mut();
            if deferred.is_empty() {
                return;
            }
            let max_visible_y = adj.value() + adj.page_size().max(800.0) + 500.0;
            deferred.retain(|(sec_widget, senders)| {
                if let Some(bounds) = sec_widget.compute_bounds(&sections_container)
                    && (bounds.y() as f64) <= max_visible_y
                {
                    for s in senders {
                        let _ = s.send(MediaCardInput::LoadArtwork);
                    }
                    return false;
                }
                true
            });
        };

        let check1 = check_vertical_sections.clone();
        vadj.connect_value_changed(move |adj| check1(adj));
        let check2 = check_vertical_sections.clone();
        vadj.connect_page_size_notify(move |adj| check2(adj));
        let check3 = check_vertical_sections;
        let vadj_clone = vadj.clone();
        widgets
            .scrolled_window
            .connect_map(move |_| check3(&vadj_clone));

        // Load initial page
        let c = client;
        let r = route;
        sender.oneshot_command(async move {
            let res = c.get_page(&r).await.map_err(|e| e.to_string());
            FeedCmd::PageLoaded {
                generation: 0,
                route: r,
                result: res,
            }
        });

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            FeedInput::MediaState(state) => {
                self.update_header_state(&state, &sender);
                for row in &self.track_controllers {
                    row.emit(crate::widgets::track_row::TrackRowInput::MediaState(
                        state.clone(),
                    ));
                }
                if let Some(page) = &mut self.page_data {
                    for item in page
                        .sections
                        .iter_mut()
                        .flat_map(|section| &mut section.items)
                    {
                        let matches = item.id == state.reference.id()
                            || item.entity.as_ref() == Some(&state.reference)
                            || item.entity.as_ref().map(|e| e.id()) == Some(state.reference.id());
                        if matches {
                            item.is_favorite = Some(state.favorite);
                            item.in_library = Some(state.in_library);
                            for action in &mut item.actions {
                                match action {
                                    PageActionWire::Favorite(r) if state.favorite => {
                                        *action = PageActionWire::Unfavorite(r.clone());
                                    }
                                    PageActionWire::Unfavorite(r) if !state.favorite => {
                                        *action = PageActionWire::Favorite(r.clone());
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                if let Some(list) = &self.virtual_track_list {
                    list.update_media_state(&state);
                }
            }

            FeedInput::ChangeSort(sort) => {
                self.current_sort = sort;
            }
            FeedInput::SelectArtist(id) => {
                if self.selected_artist_id.as_deref() != Some(&id) || self.artist_detail.is_none() {
                    self.selected_artist_id = Some(id.clone());
                    self.show_artist_detail = true;
                    if let Some(cell) = &self.show_artist_detail_cell {
                        cell.set(true);
                    }
                    self.artist_detail_loading = true;
                    self.artist_detail = None;
                    self.artist_error = None;
                    self.artist_generation = self.artist_generation.wrapping_add(1);
                    let generation = self.artist_generation;
                    let c = self.client.clone();
                    let target_id = id.clone();
                    sender.oneshot_command(async move {
                        let res = c
                            .get_page(&PageRoute::Artist(target_id.clone()))
                            .await
                            .map_err(|e| e.to_string());
                        FeedCmd::ArtistDetailLoaded {
                            generation,
                            artist_id: target_id,
                            result: res,
                        }
                    });
                } else {
                    self.show_artist_detail = true;
                    if let Some(cell) = &self.show_artist_detail_cell {
                        cell.set(true);
                    }
                }
            }
            FeedInput::ArtistSearchChanged(query) => {
                self.artist_search_query = query;
            }
            FeedInput::BackToArtistList => {
                self.show_artist_detail = false;
                if let Some(cell) = &self.show_artist_detail_cell {
                    cell.set(false);
                }
            }
            FeedInput::SelectGenre(genre) => {
                self.selected_genre = genre;
                self.show_genre_detail = true;
                if let Some(cell) = &self.show_genre_detail_cell {
                    cell.set(true);
                }
            }
            FeedInput::GenreSearchChanged(query) => {
                self.genre_search_query = query;
            }
            FeedInput::BackToGenreList => {
                self.show_genre_detail = false;
                if let Some(cell) = &self.show_genre_detail_cell {
                    cell.set(false);
                }
            }
            FeedInput::LoadRoute(route) => {
                if self.route != route || (self.page_data.is_none() && !self.is_loading) {
                    self.route = route;
                    self.current_sort = LibrarySortMethod::RecentlyAdded;
                    self.artist_search_query.clear();
                    self.genre_search_query.clear();
                    self.selected_genre = None;
                    self.show_genre_detail = false;
                    self.load_page(&sender);
                }
            }
            FeedInput::Reload => self.load_page(&sender),
            FeedInput::InvalidateRoute(route) => {
                self.page_cache.invalidate(&route);
                if self.route == route && !self.is_loading {
                    self.silent_refresh(&sender);
                }
            }
            FeedInput::RetryContinuation => {
                self.continuation_error = None;
                self.trigger_next_continuation(&sender);
            }
        }
    }

    fn update_cmd(
        &mut self,
        message: Self::CommandOutput,
        sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match message {
            FeedCmd::HeaderStateLoaded { generation, state } => {
                if generation == self.generation
                    && let Some(state) = state
                {
                    self.update_header_state(&state, &sender);
                }
            }
            FeedCmd::PageLoaded {
                generation,
                route,
                result,
            } => {
                if generation == self.generation && route == self.route {
                    self.is_loading = false;
                    match result {
                        Ok(page) => {
                            if let Some(reference) = page.header.as_ref().and_then(|header| {
                                header.actions.iter().find_map(|action| {
                                    if let PageActionWire::Play(reference) = action {
                                        Some(reference.clone())
                                    } else {
                                        None
                                    }
                                })
                            }) {
                                let client = self.client.clone();
                                sender.oneshot_command(async move {
                                    FeedCmd::HeaderStateLoaded {
                                        generation,
                                        state: client.get_media_state(&reference).await.ok(),
                                    }
                                });
                            }
                            self.page_data = Some(page);
                            self.error_message = None;

                            // If LibraryArtists, auto-select the first artist
                            if self.route == PageRoute::LibraryArtists
                                && let Some(first_id) = self
                                    .page_data
                                    .as_ref()
                                    .and_then(|p| p.sections.first())
                                    .and_then(|s| s.items.first())
                                    .and_then(artist_route_id)
                            {
                                self.selected_artist_id = Some(first_id.clone());
                                self.artist_detail_loading = true;
                                self.artist_generation = self.artist_generation.wrapping_add(1);
                                let generation = self.artist_generation;
                                let c = self.client.clone();
                                sender.oneshot_command(async move {
                                    let res = c
                                        .get_page(&PageRoute::Artist(first_id.clone()))
                                        .await
                                        .map_err(|e| e.to_string());
                                    FeedCmd::ArtistDetailLoaded {
                                        generation,
                                        artist_id: first_id,
                                        result: res,
                                    }
                                });
                            }
                        }
                        Err(e) => {
                            if self.page_data.is_none() {
                                self.page_data = None;
                                self.error_message = Some(format!("Failed to load page: {e}"));
                            } else {
                                tracing::warn!("Background page refresh failed: {e}");
                            }
                        }
                    }
                }
            }
            FeedCmd::ArtistDetailLoaded {
                generation,
                artist_id,
                result,
            } => {
                if generation == self.artist_generation
                    && self.selected_artist_id.as_deref() == Some(&artist_id)
                {
                    self.artist_detail_loading = false;
                    match result {
                        Ok(page) => self.artist_detail = Some(page),
                        Err(error) => self.artist_error = Some(error),
                    }
                }
            }
            FeedCmd::SectionContinued {
                generation,
                route,
                section_id,
                result,
            } => {
                if generation == self.generation && route == self.route {
                    self.is_continuing = false;
                    match result {
                        Ok(cont) => {
                            let has_more = match &cont {
                                PageContinuationWire::Section {
                                    section_id: sid,
                                    items,
                                    continuation,
                                } => {
                                    let mut more = false;
                                    if let Some(page) = &mut self.page_data
                                        && let Some(sec) = page
                                            .sections
                                            .iter_mut()
                                            .find(|s| s.id == *sid || s.id == section_id)
                                    {
                                        sec.items.extend(items.clone());
                                        sec.continuation = continuation.clone();
                                        more = sec.continuation.is_some();
                                    }
                                    more
                                }
                                PageContinuationWire::Sections {
                                    sections,
                                    continuation,
                                } => {
                                    let mut more = false;
                                    if let Some(page) = &mut self.page_data {
                                        page.sections.extend(sections.clone());
                                        page.continuation = continuation.clone();
                                        more = page.continuation.is_some();
                                    }
                                    more
                                }
                            };

                            if has_more || self.pending_continuation().is_some() {
                                self.trigger_next_continuation(&sender);
                            }
                        }
                        Err(e) => {
                            self.continuation_error =
                                Some(format!("Couldn’t load more items: {e}"));
                        }
                    }
                }
            }
        }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        message: Self::Input,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        let route_changes = matches!(&message, FeedInput::LoadRoute(route) if *route != self.route);
        let is_sort_change = matches!(&message, FeedInput::ChangeSort(_));
        let is_artist_nav = matches!(
            &message,
            FeedInput::SelectArtist(_) | FeedInput::BackToArtistList
        );
        let is_genre_nav = matches!(
            &message,
            FeedInput::SelectGenre(_) | FeedInput::BackToGenreList
        );
        self.update(message, sender.clone(), root);
        self.update_view(widgets, sender.clone());
        if self.is_loading {
            if self.skeleton_route.as_ref() != Some(&self.route) {
                while let Some(child) = widgets.skeleton_container.first_child() {
                    widgets.skeleton_container.remove(&child);
                }
                let skeleton = crate::widgets::skeleton::build_route_skeleton(&self.route);
                widgets.skeleton_container.append(&skeleton);
                self.skeleton_route = Some(self.route.clone());
            }
        } else {
            if self.skeleton_route.is_some() {
                self.skeleton_route = None;
                while let Some(child) = widgets.skeleton_container.first_child() {
                    widgets.skeleton_container.remove(&child);
                }
            }
            if self.page_data.is_some() && route_changes {
                self.render_content(widgets, sender.clone());
            }
        }
        if route_changes {
            widgets.scrolled_window.vadjustment().set_value(0.0);
        }
        if is_artist_nav && let Some(split) = self.active_artist_split.clone() {
            self.update_artist_view_inplace(&split, sender);
            return;
        }
        if (is_genre_nav || (is_sort_change && self.route == PageRoute::LibraryGenres))
            && let Some(split) = self.active_genre_split.clone()
        {
            self.update_genre_view_inplace(&split, sender);
            return;
        }
        if is_sort_change || is_artist_nav || is_genre_nav {
            self.render_content(widgets, sender);
        }
    }

    fn update_cmd_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        message: Self::CommandOutput,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        let current = match &message {
            FeedCmd::HeaderStateLoaded { generation, .. } => *generation == self.generation,
            FeedCmd::PageLoaded {
                generation, route, ..
            }
            | FeedCmd::SectionContinued {
                generation, route, ..
            } => *generation == self.generation && *route == self.route,
            FeedCmd::ArtistDetailLoaded {
                generation,
                artist_id,
                ..
            } => {
                *generation == self.artist_generation
                    && self.selected_artist_id.as_deref() == Some(artist_id)
            }
        };
        if !current {
            return;
        }
        let is_artist_loaded = matches!(&message, FeedCmd::ArtistDetailLoaded { .. });
        let continuation_failed =
            matches!(&message, FeedCmd::SectionContinued { result: Err(_), .. });
        let should_render = matches!(
            &message,
            FeedCmd::PageLoaded { result: Ok(_), .. } | FeedCmd::ArtistDetailLoaded { .. }
        );

        let cont_opt = if let FeedCmd::SectionContinued {
            result: Ok(ref cont),
            ref route,
            ..
        } = message
        {
            if *route == self.route {
                Some(cont.clone())
            } else {
                None
            }
        } else {
            None
        };

        self.update_cmd(message, sender.clone(), root);
        self.update_view(widgets, sender.clone());
        if !self.is_loading && self.skeleton_route.is_some() {
            self.skeleton_route = None;
            while let Some(child) = widgets.skeleton_container.first_child() {
                widgets.skeleton_container.remove(&child);
            }
        }
        if is_artist_loaded && let Some(split) = self.active_artist_split.clone() {
            self.update_artist_view_inplace(&split, sender);
            return;
        }
        if should_render {
            self.render_content(widgets, sender.clone());
            if self.continuation_error.is_none() {
                self.trigger_next_continuation(&sender);
            }
        } else if let Some(cont) = cont_opt {
            self.append_continuation_items(widgets, sender, &cont);
        }
        if continuation_failed {
            self.stop_continuation_spinner();
        }
    }
}

impl FeedPage {
    pub fn find_track(&self, media_ref: &MediaRef) -> Option<Track> {
        for tc in &self.track_controllers {
            if &tc.model().track.id == media_ref {
                return Some(tc.model().track.clone());
            }
        }
        for tc in &self.compact_track_controllers {
            let m = tc.model();
            if m.entity.as_ref() == Some(media_ref) || m.id == media_ref.id() {
                let mut track = Track::new(
                    media_ref.clone(),
                    &m.title,
                    m.artist.as_deref().unwrap_or_default(),
                );
                if let Some(url) = &m.artwork_url {
                    track = track.with_artwork(malus_model::Artwork::new(url.clone()));
                }
                return Some(track);
            }
        }
        if let Some(page) = &self.page_data {
            if let Some(track) = find_track_in_page_data(page, media_ref) {
                return Some(track);
            }
        }
        if let Some(artist_page) = &self.artist_detail {
            if let Some(track) = find_track_in_page_data(artist_page, media_ref) {
                return Some(track);
            }
        }
        None
    }

    fn update_header_state(
        &mut self,
        state: &malus_model::AccountMediaState,
        sender: &ComponentSender<Self>,
    ) {
        let header = self
            .page_data
            .as_mut()
            .and_then(|page| page.header.as_mut());
        let Some(header) = header else {
            return;
        };
        if !header.actions.iter().any(|action| matches!(action, PageActionWire::Play(reference) if *reference == state.reference)) { return; }
        for action in &mut header.actions {
            if matches!(
                action,
                PageActionWire::Favorite(_) | PageActionWire::Unfavorite(_)
            ) {
                *action = if state.favorite {
                    PageActionWire::Unfavorite(state.reference.clone())
                } else {
                    PageActionWire::Favorite(state.reference.clone())
                };
            }
        }
        if state.in_library {
            header
                .actions
                .retain(|action| !matches!(action, PageActionWire::AddToLibrary(_)));
        }
        let header = header.clone();
        if let Some(host) = &self.header_actions_host {
            while let Some(child) = host.first_child() {
                host.remove(&child);
            }
            host.append(&self.build_header_actions(&header, sender.clone()));
        }
    }

    fn load_page(&mut self, sender: &ComponentSender<Self>) {
        self.generation = self.generation.wrapping_add(1);
        self.artist_generation = self.artist_generation.wrapping_add(1);
        self.header_actions_host = None;
        self.error_message = None;
        self.continuation_error = None;
        self.artist_error = None;
        self.selected_artist_id = None;
        self.artist_detail = None;
        self.show_artist_detail = false;
        self.artist_detail_loading = false;
        self.is_continuing = false;
        self.mounted_sections.clear();
        self.deferred_sections.borrow_mut().clear();
        self.active_artist_list = None;
        self.active_artist_spinner = None;
        self.virtual_track_list = None;
        self.library_subtitle_lbl = None;

        // Check speculative client-side page cache
        let cached = self.page_cache.try_get(&self.route);
        if let Some(ref page) = cached {
            self.page_data = Some(page.clone());
            self.is_loading = false;
        } else {
            self.page_data = None;
            self.is_loading = true;
        }

        // Prefetch related routes
        self.page_cache.prefetch_related(&self.client, &self.route);

        let client = self.client.clone();
        let cache = self.page_cache.clone();
        let route = self.route.clone();
        let generation = self.generation;
        sender.oneshot_command(async move {
            let res = client.get_page(&route).await.map_err(|e| e.to_string());
            if let Ok(ref page) = res {
                cache.insert(route.clone(), page.clone()).await;
            }
            FeedCmd::PageLoaded {
                generation,
                result: res,
                route,
            }
        });
    }

    fn silent_refresh(&mut self, sender: &ComponentSender<Self>) {
        self.generation = self.generation.wrapping_add(1);
        let client = self.client.clone();
        let cache = self.page_cache.clone();
        let route = self.route.clone();
        let generation = self.generation;
        sender.oneshot_command(async move {
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            let res = client.get_page(&route).await.map_err(|e| e.to_string());
            if let Ok(ref page) = res {
                cache.insert(route.clone(), page.clone()).await;
            }
            FeedCmd::PageLoaded {
                generation,
                result: res,
                route,
            }
        });
    }

    fn stop_continuation_spinner(&mut self) {
        if let Some(list) = &self.virtual_track_list {
            list.set_loading(false);
        }
        if let Some(spinner) = self.active_artist_spinner.take()
            && let Some(parent) = spinner.parent()
            && let Ok(box_parent) = parent.downcast::<gtk::Box>()
        {
            box_parent.remove(&spinner);
        }
        for mounted in self.mounted_sections.values() {
            match mounted {
                MountedSection::TrackList {
                    track_list,
                    spinner,
                    ..
                } => {
                    if let Some(sp) = spinner.borrow_mut().take()
                        && sp.parent().is_some()
                    {
                        track_list.remove(&sp);
                    }
                }
                MountedSection::Grid {
                    spinner_parent,
                    spinner,
                    ..
                } => {
                    if let Some(sp) = spinner.borrow_mut().take()
                        && sp.parent().is_some()
                    {
                        spinner_parent.remove(&sp);
                    }
                }
                _ => {}
            }
        }
    }

    fn pending_continuation(&self) -> Option<(String, PageCursorWire)> {
        if let Some(page) = &self.page_data {
            for section in &page.sections {
                if let Some(cursor) = &section.continuation {
                    return Some((section.id.clone(), cursor.clone()));
                }
            }
        }
        self.page_data
            .as_ref()
            .and_then(|page| page.continuation.clone())
            .map(|cursor| (String::new(), cursor))
    }

    fn trigger_next_continuation(&mut self, sender: &ComponentSender<Self>) {
        if self.is_continuing || self.is_loading || self.continuation_error.is_some() {
            return;
        }
        if let Some((section_id, cursor)) = self.pending_continuation() {
            self.is_continuing = true;
            let c = self.client.clone();
            let r = self.route.clone();
            let generation = self.generation;
            sender.oneshot_command(async move {
                let res = c.continue_page(&r, cursor).await.map_err(|e| e.to_string());
                FeedCmd::SectionContinued {
                    generation,
                    route: r,
                    section_id,
                    result: res,
                }
            });
        }
    }
}

fn find_track_in_page_data(page: &PageWire, media_ref: &MediaRef) -> Option<Track> {
    for section in &page.sections {
        for item in &section.items {
            if item.entity.as_ref() == Some(media_ref) || item.id == media_ref.id() {
                let mut track = Track::new(
                    media_ref.clone(),
                    &item.title,
                    item.subtitle.as_deref().unwrap_or_default(),
                );
                if let Some(artwork) = &item.artwork {
                    track = track.with_artwork(artwork.clone());
                }
                if let Some(dur) = item.duration_ms {
                    track = track.with_duration_ms(dur);
                }
                return Some(track);
            }
        }
    }
    None
}
