//! Categorized Search Results and Landing Page adhering to official Apple Music architecture.

use malus_client::MalusClient;
use malus_ipc::wire::{
    BROWSE_CATEGORIES, PageActionWire, PageItemWire, SearchKindWire, SearchResultsWire,
    SearchScopeWire,
};
use malus_model::{Album, Artist, MediaRef, PageRoute, Playlist, Track};
use relm4::gtk::{self, prelude::*};
use relm4::prelude::*;

use crate::design::tokens::*;
use crate::services::{
    ArtworkService, RecentSearchItem, add_recent_search, bind_artwork, clear_recent_searches,
    load_recent_searches,
};
use crate::widgets::category_card::{CategoryCard, CategoryCardInit, CategoryCardOutput};
use crate::widgets::empty_state::create_empty_state;
use crate::widgets::media_card::{MediaCard, MediaCardInit, MediaCardOutput};
use crate::widgets::media_shelf::{create_shelf_container, hook_shelf_artwork_trigger};
use crate::widgets::square_artwork::SquareArtwork;
use crate::widgets::track_row::{TrackRow, TrackRowInit, TrackRowOutput};

/// Available category tabs for filtering search output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchTab {
    #[default]
    TopResults,
    Songs,
    Artists,
    Albums,
    Playlists,
    Stations,
}

pub struct SearchPage {
    client: MalusClient,
    artwork_service: ArtworkService,
    query: String,
    results: Option<SearchResultsWire>,
    active_tab: SearchTab,
    is_loading: bool,
    is_loading_more: bool,
    error_message: Option<String>,
    card_controllers: Vec<Controller<MediaCard>>,
    track_controllers: Vec<Controller<TrackRow>>,
    category_controllers: Vec<Controller<CategoryCard>>,
    search_generation: u64,
    scope: SearchScopeWire,
    recent_searches: Vec<RecentSearchItem>,
}

#[derive(Debug)]
pub enum SearchInput {
    MediaState(malus_model::AccountMediaState),
    QueryChanged(String),
    ExecuteSearch(String),
    ExecuteSearchDebounced { query: String, generation: u64 },
    SelectTab(SearchTab),
    SetScope(SearchScopeWire),
    ClearRecentSearches,
    RemoveRecentSearch(String),
    RecordRecentSearch(RecentSearchItem),
    LoadMoreTab(SearchTab),
    ShowLanding,
    FocusSearch,
}

#[derive(Debug)]
pub enum SearchCmd {
    ResultsLoaded {
        generation: u64,
        query: String,
        scope: SearchScopeWire,
        result: Result<SearchResultsWire, String>,
    },
    MoreResultsLoaded {
        generation: u64,
        query: String,
        scope: SearchScopeWire,
        tab: SearchTab,
        result: Result<SearchResultsWire, String>,
    },
}

#[derive(Debug, Clone)]
pub enum SearchOutput {
    Play(MediaRef),
    Navigate(PageRoute),
    Action(PageActionWire),
    ViewCredits(MediaRef),
    ShowAddToPlaylist(MediaRef),
    CopyLink(String),
}

#[relm4::component(pub)]
impl Component for SearchPage {
    type Init = (MalusClient, ArtworkService);
    type Input = SearchInput;
    type Output = SearchOutput;
    type CommandOutput = SearchCmd;

    view! {
        gtk::ScrolledWindow {
            set_hscrollbar_policy: gtk::PolicyType::Never,
            set_vscrollbar_policy: gtk::PolicyType::Automatic,
            set_vexpand: true,
            set_hexpand: true,
            add_css_class: "search-scrolled-window",

            #[name(content_box)]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: SPACING_LG,
                set_margin_start: PAGE_PADDING_NORMAL,
                set_margin_end: PAGE_PADDING_NORMAL,
                set_margin_top: SPACING_LG,
                set_margin_bottom: SPACING_XXL,

                // Search Header: Search Entry Bar + Scope Switcher
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: SPACING_MD,
                    set_valign: gtk::Align::Center,

                    // Search Entry Bar
                    #[name(search_entry)]
                    gtk::SearchEntry {
                        set_placeholder_text: Some("Artists, Songs, Lyrics, and More"),
                        set_width_request: SEARCH_BAR_WIDTH_MIN,
                        set_hexpand: true,
                        set_halign: gtk::Align::Fill,
                        add_css_class: "search-page-entry",
                        connect_search_changed[sender] => move |entry| {
                            sender.input(SearchInput::QueryChanged(entry.text().to_string()));
                        },
                        connect_activate[sender] => move |entry| {
                            sender.input(SearchInput::ExecuteSearch(entry.text().to_string()));
                        },
                        connect_stop_search[sender] => move |entry| {
                            if !entry.text().is_empty() {
                                entry.set_text("");
                                sender.input(SearchInput::ShowLanding);
                            }
                        },
                    },

                    // Right: Scope Switcher [ Apple Music | Your Library ]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_halign: gtk::Align::End,
                        set_valign: gtk::Align::Center,
                        add_css_class: "search-scope-switcher",

                        #[name(catalog_toggle)]
                        gtk::ToggleButton {
                            set_label: "Apple Music",
                            add_css_class: "search-scope-btn",
                            #[watch]
                            set_active: model.scope == SearchScopeWire::Catalog,
                            connect_toggled[sender] => move |btn| {
                                if btn.is_active() {
                                    sender.input(SearchInput::SetScope(SearchScopeWire::Catalog));
                                }
                            },
                        },

                        #[name(library_toggle)]
                        gtk::ToggleButton {
                            set_label: "Your Library",
                            add_css_class: "search-scope-btn",
                            #[watch]
                            set_active: model.scope == SearchScopeWire::Library,
                            connect_toggled[sender] => move |btn| {
                                if btn.is_active() {
                                    sender.input(SearchInput::SetScope(SearchScopeWire::Library));
                                }
                            },
                        },
                    },
                },

                // Results subtitle (visible when searching)
                gtk::Label {
                    set_xalign: 0.0,
                    add_css_class: "page-subtitle",
                    #[watch]
                    set_visible: !model.query.is_empty(),
                    #[watch]
                    set_text: &format!(
                        "Results for \"{}\" in {}",
                        model.query,
                        match model.scope {
                            SearchScopeWire::Catalog => "Apple Music",
                            SearchScopeWire::Library => "Your Library",
                        }
                    ),
                },

                // Loading Spinner
                #[name(spinner)]
                gtk::Spinner {
                    set_size_request: (CONTROL_SM, CONTROL_SM),
                    set_halign: gtk::Align::Center,
                    set_margin_top: SPACING_XXL,
                    #[watch]
                    set_spinning: model.is_loading,
                    #[watch]
                    set_visible: model.is_loading,
                },

                // Dynamic Results / Landing Container
                #[name(results_container)]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: SPACING_LG,
                    #[watch]
                    set_visible: !model.is_loading,
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let (client, artwork_service) = init;
        let mut model = Self {
            client,
            artwork_service,
            query: String::new(),
            results: None,
            active_tab: SearchTab::TopResults,
            is_loading: false,
            is_loading_more: false,
            error_message: None,
            card_controllers: Vec::new(),
            track_controllers: Vec::new(),
            category_controllers: Vec::new(),
            search_generation: 0,
            scope: SearchScopeWire::Catalog,
            recent_searches: load_recent_searches(),
        };

        let mut widgets = view_output!();
        widgets
            .library_toggle
            .set_group(Some(&widgets.catalog_toggle));

        let content = widgets.content_box.clone();
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

        model.render_content(&mut widgets, sender);
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            SearchInput::MediaState(state) => {
                for row in &self.track_controllers {
                    row.emit(crate::widgets::track_row::TrackRowInput::MediaState(
                        state.clone(),
                    ));
                }
            }

            SearchInput::SelectTab(tab) => {
                if self.active_tab != tab {
                    self.active_tab = tab;
                }
            }

            SearchInput::SetScope(scope) => {
                if self.scope != scope {
                    self.scope = scope;
                    if !self.query.is_empty() {
                        self.search_generation = self.search_generation.wrapping_add(1);
                        let current_gen = self.search_generation;
                        self.is_loading = true;
                        self.error_message = None;

                        let c = self.client.clone();
                        let q = self.query.clone();
                        let kinds = vec![
                            SearchKindWire::Track,
                            SearchKindWire::Album,
                            SearchKindWire::Artist,
                            SearchKindWire::Playlist,
                            SearchKindWire::Station,
                        ];
                        sender.oneshot_command(async move {
                            let res = c
                                .search_with_scope(&q, kinds, Some(25), None, scope)
                                .await
                                .map_err(|e| e.to_string());
                            SearchCmd::ResultsLoaded {
                                generation: current_gen,
                                query: q,
                                scope,
                                result: res,
                            }
                        });
                    }
                }
            }

            SearchInput::ClearRecentSearches => {
                clear_recent_searches();
                self.recent_searches.clear();
            }

            SearchInput::RemoveRecentSearch(id) => {
                self.recent_searches.retain(|item| item.id != id);
                crate::services::recent_searches::save_recent_searches(&self.recent_searches);
            }

            SearchInput::RecordRecentSearch(item) => {
                add_recent_search(item);
                self.recent_searches = load_recent_searches();
            }

            SearchInput::LoadMoreTab(tab) => {
                let cursor = match tab {
                    SearchTab::Songs => self
                        .results
                        .as_ref()
                        .and_then(|r| r.tracks.as_ref())
                        .and_then(|p| p.next_cursor.clone()),
                    SearchTab::Artists => self
                        .results
                        .as_ref()
                        .and_then(|r| r.artists.as_ref())
                        .and_then(|p| p.next_cursor.clone()),
                    SearchTab::Albums => self
                        .results
                        .as_ref()
                        .and_then(|r| r.albums.as_ref())
                        .and_then(|p| p.next_cursor.clone()),
                    SearchTab::Playlists => self
                        .results
                        .as_ref()
                        .and_then(|r| r.playlists.as_ref())
                        .and_then(|p| p.next_cursor.clone()),
                    SearchTab::Stations => self
                        .results
                        .as_ref()
                        .and_then(|r| r.stations.as_ref())
                        .and_then(|p| p.next_cursor.clone()),
                    _ => None,
                };

                if let Some(c_cursor) = cursor {
                    self.is_loading_more = true;
                    let current_gen = self.search_generation;
                    let q = self.query.clone();
                    let scope = self.scope;
                    let c = self.client.clone();
                    let kinds = match tab {
                        SearchTab::Songs => vec![SearchKindWire::Track],
                        SearchTab::Artists => vec![SearchKindWire::Artist],
                        SearchTab::Albums => vec![SearchKindWire::Album],
                        SearchTab::Playlists => vec![SearchKindWire::Playlist],
                        SearchTab::Stations => vec![SearchKindWire::Station],
                        _ => vec![],
                    };
                    sender.oneshot_command(async move {
                        let res = c
                            .search_with_scope(&q, kinds, Some(25), Some(c_cursor), scope)
                            .await
                            .map_err(|e| e.to_string());

                        SearchCmd::MoreResultsLoaded {
                            generation: current_gen,
                            query: q,
                            scope,
                            tab,
                            result: res,
                        }
                    });
                }
            }

            SearchInput::ShowLanding => {
                self.search_generation = self.search_generation.wrapping_add(1);
                self.query = String::new();
                self.results = None;
                self.is_loading = false;
                self.is_loading_more = false;
                self.error_message = None;
                self.active_tab = SearchTab::TopResults;
                self.recent_searches = load_recent_searches();
            }

            SearchInput::FocusSearch => {}

            SearchInput::QueryChanged(q) => {
                let trimmed = q.trim().to_string();
                if trimmed == self.query {
                    return;
                }
                self.search_generation = self.search_generation.wrapping_add(1);
                let current_gen = self.search_generation;
                if trimmed.is_empty() {
                    self.query = String::new();
                    self.results = None;
                    self.is_loading = false;
                    self.is_loading_more = false;
                    self.error_message = None;
                    self.active_tab = SearchTab::TopResults;
                    self.recent_searches = load_recent_searches();
                } else if self.query != trimmed {
                    self.query = trimmed.clone();
                    self.is_loading = true;
                    self.error_message = None;

                    let s = sender.clone();
                    gtk::glib::timeout_add_local_once(
                        std::time::Duration::from_millis(250),
                        move || {
                            s.input(SearchInput::ExecuteSearchDebounced {
                                query: trimmed,
                                generation: current_gen,
                            });
                        },
                    );
                }
            }

            SearchInput::ExecuteSearchDebounced { query, generation } => {
                if generation == self.search_generation && query == self.query && !query.is_empty()
                {
                    let c = self.client.clone();
                    let scope = self.scope;
                    let kinds = vec![
                        SearchKindWire::Track,
                        SearchKindWire::Album,
                        SearchKindWire::Artist,
                        SearchKindWire::Playlist,
                        SearchKindWire::Station,
                    ];
                    sender.oneshot_command(async move {
                        let res = c
                            .search_with_scope(&query, kinds, Some(25), None, scope)
                            .await
                            .map_err(|e| e.to_string());
                        SearchCmd::ResultsLoaded {
                            generation,
                            query,
                            scope,
                            result: res,
                        }
                    });
                }
            }

            SearchInput::ExecuteSearch(q) => {
                let trimmed = q.trim().to_string();
                if !trimmed.is_empty() {
                    self.search_generation = self.search_generation.wrapping_add(1);
                    self.query = trimmed.clone();
                    self.is_loading = true;
                    self.error_message = None;
                    let generation = self.search_generation;
                    let scope = self.scope;
                    let c = self.client.clone();
                    let kinds = vec![
                        SearchKindWire::Track,
                        SearchKindWire::Album,
                        SearchKindWire::Artist,
                        SearchKindWire::Playlist,
                        SearchKindWire::Station,
                    ];
                    sender.oneshot_command(async move {
                        let res = c
                            .search_with_scope(&trimmed, kinds, Some(25), None, scope)
                            .await
                            .map_err(|e| e.to_string());
                        SearchCmd::ResultsLoaded {
                            generation,
                            query: trimmed,
                            scope,
                            result: res,
                        }
                    });
                }
            }
        }
    }

    fn update_cmd(
        &mut self,
        message: Self::CommandOutput,
        _sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match message {
            SearchCmd::ResultsLoaded {
                generation,
                query,
                result,
                ..
            } => {
                if generation == self.search_generation && query == self.query {
                    self.is_loading = false;
                    self.is_loading_more = false;
                    match result {
                        Ok(res) => {
                            self.results = Some(res);
                            self.error_message = None;
                            self.active_tab = SearchTab::TopResults;
                        }
                        Err(e) => {
                            self.results = None;
                            self.error_message = Some(e);
                        }
                    }
                }
            }

            SearchCmd::MoreResultsLoaded {
                generation,
                query,
                tab,
                result,
                ..
            } => {
                if generation == self.search_generation && query == self.query {
                    self.is_loading_more = false;
                    if let Ok(more_res) = result {
                        match tab {
                            SearchTab::Songs => {
                                if let Some(more_tracks) = more_res.tracks
                                    && let Some(ref mut tracks) =
                                        self.results.as_mut().and_then(|r| r.tracks.as_mut())
                                {
                                    tracks.items.extend(more_tracks.items);
                                    tracks.next_cursor = more_tracks.next_cursor;
                                }
                            }
                            SearchTab::Artists => {
                                if let Some(more_artists) = more_res.artists
                                    && let Some(ref mut artists) =
                                        self.results.as_mut().and_then(|r| r.artists.as_mut())
                                {
                                    artists.items.extend(more_artists.items);
                                    artists.next_cursor = more_artists.next_cursor;
                                }
                            }
                            SearchTab::Albums => {
                                if let Some(more_albums) = more_res.albums
                                    && let Some(ref mut albums) =
                                        self.results.as_mut().and_then(|r| r.albums.as_mut())
                                {
                                    albums.items.extend(more_albums.items);
                                    albums.next_cursor = more_albums.next_cursor;
                                }
                            }
                            SearchTab::Playlists => {
                                if let Some(more_pls) = more_res.playlists
                                    && let Some(ref mut playlists) =
                                        self.results.as_mut().and_then(|r| r.playlists.as_mut())
                                {
                                    playlists.items.extend(more_pls.items);
                                    playlists.next_cursor = more_pls.next_cursor;
                                }
                            }
                            SearchTab::Stations => {
                                if let Some(more_st) = more_res.stations
                                    && let Some(ref mut stations) =
                                        self.results.as_mut().and_then(|r| r.stations.as_mut())
                                {
                                    stations.items.extend(more_st.items);
                                    stations.next_cursor = more_st.next_cursor;
                                }
                            }

                            _ => {}
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
        match &message {
            SearchInput::FocusSearch => {
                widgets.search_entry.grab_focus();
                widgets.search_entry.select_region(0, -1);
            }
            SearchInput::ShowLanding => {
                if widgets.search_entry.text().as_str() != "" {
                    widgets.search_entry.set_text("");
                }
            }
            SearchInput::ExecuteSearch(q) if widgets.search_entry.text().as_str() != q => {
                widgets.search_entry.set_text(q);
            }
            SearchInput::ExecuteSearch(_) => {}
            _ => {}
        }

        let should_rerender = match &message {
            SearchInput::QueryChanged(q) => q.trim().is_empty(),
            SearchInput::ShowLanding => true,
            SearchInput::SelectTab(_) => true,
            SearchInput::SetScope(_) => self.query.is_empty(),
            SearchInput::ClearRecentSearches => true,
            SearchInput::RemoveRecentSearch(_) => true,
            SearchInput::RecordRecentSearch(_) => false,
            _ => false,
        };
        self.update(message, sender.clone(), root);
        self.update_view(widgets, sender.clone());
        if should_rerender {
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
        match &message {
            SearchCmd::ResultsLoaded {
                generation, query, ..
            } => {
                if *generation != self.search_generation || *query != self.query {
                    return;
                }
            }
            SearchCmd::MoreResultsLoaded {
                generation, query, ..
            } => {
                if *generation != self.search_generation || *query != self.query {
                    return;
                }
            }
        }
        self.update_cmd(message, sender.clone(), root);
        self.update_view(widgets, sender.clone());
        self.render_content(widgets, sender);
    }
}

impl SearchPage {
    fn render_content(&mut self, widgets: &mut SearchPageWidgets, sender: ComponentSender<Self>) {
        if self.is_loading {
            return;
        }

        self.card_controllers.clear();
        self.track_controllers.clear();
        self.category_controllers.clear();
        while let Some(child) = widgets.results_container.first_child() {
            widgets.results_container.remove(&child);
        }

        if self.query.is_empty() {
            self.render_landing_page(&widgets.results_container, &sender);
            return;
        }

        if let Some(error) = &self.error_message {
            let status =
                create_empty_state("network-error-symbolic", "Search unavailable", Some(error));
            let retry = gtk::Button::with_label("Try Again");
            retry.add_css_class("suggested-action");
            retry.set_halign(gtk::Align::Center);
            let query = self.query.clone();
            retry.connect_clicked(move |_| sender.input(SearchInput::ExecuteSearch(query.clone())));
            status.set_child(Some(&retry));
            widgets.results_container.append(&status);
            return;
        }

        let Some(ref res) = self.results else {
            return;
        };

        if res.is_empty() {
            let empty = create_empty_state(
                ICON_SEARCH,
                "No Results Found",
                Some("Check the spelling or try a different search"),
            );
            widgets.results_container.append(&empty);
            return;
        }

        // 1. Render Category Filter Tabs Bar
        self.render_tabs(&widgets.results_container, &sender);

        // 2. Render Active Tab View
        match self.active_tab {
            SearchTab::TopResults => {
                self.render_top_results_tab(&widgets.results_container, &sender);
            }
            SearchTab::Songs => {
                self.render_songs_tab(&widgets.results_container, &sender);
            }
            SearchTab::Artists => {
                self.render_artists_tab(&widgets.results_container, &sender);
            }
            SearchTab::Albums => {
                self.render_albums_tab(&widgets.results_container, &sender);
            }
            SearchTab::Playlists => {
                self.render_playlists_tab(&widgets.results_container, &sender);
            }
            SearchTab::Stations => {
                self.render_stations_tab(&widgets.results_container, &sender);
            }
        }
    }

    /// Render landing page: Recently Searched + Browse Categories grid.
    fn render_landing_page(&mut self, container: &gtk::Box, sender: &ComponentSender<Self>) {
        // 1. Recently Searched
        if !self.recent_searches.is_empty() {
            let sec_box = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(SPACING_SM)
                .build();

            let header_row = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(SPACING_MD)
                .valign(gtk::Align::Center)
                .build();

            let title_lbl = gtk::Label::builder()
                .label("Recently Searched")
                .xalign(0.0)
                .hexpand(true)
                .css_classes(vec!["shelf-title".to_string()])
                .build();
            header_row.append(&title_lbl);

            let clear_btn = gtk::Button::builder()
                .label("Clear")
                .css_classes(vec!["recent-clear-btn".to_string()])
                .build();
            let s = sender.clone();
            clear_btn.connect_clicked(move |_| {
                s.input(SearchInput::ClearRecentSearches);
            });
            header_row.append(&clear_btn);
            sec_box.append(&header_row);

            // Horizontal scrolling shelf for recent cards
            let (scrolled, shelf_box) = create_shelf_container();
            for item in &self.recent_searches {
                let card = self.create_recent_card(item, sender);
                shelf_box.append(&card);
            }
            sec_box.append(&scrolled);
            container.append(&sec_box);
        }

        // 2. Browse Categories
        let cat_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(SPACING_MD)
            .build();

        let cat_title = gtk::Label::builder()
            .label("Browse Categories")
            .xalign(0.0)
            .css_classes(vec!["shelf-title".to_string()])
            .build();
        cat_box.append(&cat_title);

        let flow = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .min_children_per_line(2)
            .max_children_per_line(6)
            .column_spacing(SPACING_MD as u32)
            .row_spacing(SPACING_MD as u32)
            .homogeneous(true)
            .hexpand(true)
            .build();

        for cat in BROWSE_CATEGORIES {
            let route = PageRoute::parse(cat.route).unwrap_or(PageRoute::Home);
            let card = CategoryCard::builder()
                .launch((
                    CategoryCardInit {
                        id: cat.id.to_string(),
                        title: cat.title.to_string(),
                        route,
                        artwork_url: cat.artwork_url.to_string(),
                        bg_color: cat.bg_color.to_string(),
                    },
                    self.artwork_service.clone(),
                ))
                .forward(sender.output_sender(), |out| match out {
                    CategoryCardOutput::Navigate(r) => SearchOutput::Navigate(r),
                });
            flow.append(card.widget());
            self.category_controllers.push(card);
        }

        cat_box.append(&flow);
        container.append(&cat_box);
    }

    /// Render recent search card.
    fn create_recent_card(
        &self,
        item: &RecentSearchItem,
        sender: &ComponentSender<Self>,
    ) -> gtk::Button {
        let btn = gtk::Button::builder()
            .css_classes(vec!["flat".to_string(), "recent-search-card".to_string()])
            .valign(gtk::Align::Center)
            .build();

        let h_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(SPACING_SM)
            .valign(gtk::Align::Center)
            .build();

        // Artwork Thumbnail
        let art_class = if item.is_circular {
            "search-lockup-artwork-circular"
        } else {
            "recent-search-thumb"
        };
        let art = SquareArtwork::new(RECENT_SEARCH_THUMB_SIZE, art_class);
        if let Some(ref url) = item.artwork_url {
            bind_artwork(art.picture(), &self.artwork_service, Some(url.clone()), 128);
        }
        h_box.append(&art);

        // Text box
        let text_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(SPACING_XXS / 2)
            .valign(gtk::Align::Center)
            .hexpand(true)
            .build();

        let title_lbl = gtk::Label::builder()
            .label(&item.title)
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .css_classes(vec!["recent-search-title".to_string()])
            .build();
        text_box.append(&title_lbl);

        if let Some(ref sub) = item.subtitle {
            let sub_lbl = gtk::Label::builder()
                .label(sub)
                .xalign(0.0)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .css_classes(vec!["recent-search-sub".to_string()])
                .build();
            text_box.append(&sub_lbl);
        }

        if item.is_library {
            let lib_badge = gtk::Label::builder()
                .label("LIBRARY")
                .xalign(0.0)
                .css_classes(vec!["recent-library-badge".to_string()])
                .build();
            text_box.append(&lib_badge);
        }

        h_box.append(&text_box);

        // Delete button
        let del_btn = gtk::Button::builder()
            .icon_name(ICON_CLOSE)
            .css_classes(vec!["flat".to_string(), "circular".to_string()])
            .valign(gtk::Align::Center)
            .halign(gtk::Align::End)
            .build();

        let s_del = sender.clone();
        let item_id = item.id.clone();
        del_btn.connect_clicked(move |_| {
            s_del.input(SearchInput::RemoveRecentSearch(item_id.clone()));
        });
        h_box.append(&del_btn);

        btn.set_child(Some(&h_box));

        let s = sender.clone();
        let route = item.route.clone();
        let media_ref = item.media_ref.clone();
        btn.connect_clicked(move |_| {
            if let Some(ref r) = route {
                let _ = s.output(SearchOutput::Navigate(r.clone()));
            } else if let Some(ref m) = media_ref {
                let _ = s.output(SearchOutput::Play(m.clone()));
            }
        });

        btn
    }

    /// Render interactive category tabs (pills) dynamically based on available results.
    fn render_tabs(&self, container: &gtk::Box, sender: &ComponentSender<Self>) {
        let Some(ref res) = self.results else { return };

        let mut available_tabs = vec![SearchTab::TopResults];
        if res.tracks.as_ref().is_some_and(|p| !p.items.is_empty()) {
            available_tabs.push(SearchTab::Songs);
        }
        if res.artists.as_ref().is_some_and(|p| !p.items.is_empty()) {
            available_tabs.push(SearchTab::Artists);
        }
        if res.albums.as_ref().is_some_and(|p| !p.items.is_empty()) {
            available_tabs.push(SearchTab::Albums);
        }
        if res.playlists.as_ref().is_some_and(|p| !p.items.is_empty()) {
            available_tabs.push(SearchTab::Playlists);
        }
        if res.stations.as_ref().is_some_and(|p| !p.items.is_empty()) {
            available_tabs.push(SearchTab::Stations);
        }

        if available_tabs.len() <= 1 {
            return;
        }

        let tabs_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(SPACING_XS)
            .css_classes(vec!["search-tabs-box".to_string()])
            .build();

        for tab in available_tabs {
            let label = match tab {
                SearchTab::TopResults => "Top Results",
                SearchTab::Songs => "Songs",
                SearchTab::Artists => "Artists",
                SearchTab::Albums => "Albums",
                SearchTab::Playlists => "Playlists",
                SearchTab::Stations => "Stations",
            };

            let mut classes = vec!["flat".to_string(), "search-tab-pill".to_string()];
            if self.active_tab == tab {
                classes.push("search-tab-pill-active".to_string());
            }

            let btn = gtk::Button::builder()
                .label(label)
                .css_classes(classes)
                .build();

            let s = sender.clone();
            btn.connect_clicked(move |_| {
                s.input(SearchInput::SelectTab(tab));
            });

            tabs_box.append(&btn);
        }

        container.append(&tabs_box);
    }

    /// Render default Top Results overview with 2x3 lockup cards and preview shelves.
    fn render_top_results_tab(&mut self, container: &gtk::Box, sender: &ComponentSender<Self>) {
        let Some(ref res) = self.results.clone() else {
            return;
        };

        // 1. Top Results 2x3 Lockup Grid
        if let Some(ref top_items) = res.top_results
            && !top_items.is_empty()
        {
            let sec_box = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(SPACING_SM)
                .build();

            let title = gtk::Label::builder()
                .label("Top Results")
                .xalign(0.0)
                .css_classes(vec!["section-title".to_string()])
                .build();
            sec_box.append(&title);

            let flow = gtk::FlowBox::builder()
                .selection_mode(gtk::SelectionMode::None)
                .min_children_per_line(1)
                .max_children_per_line(3)
                .column_spacing(SPACING_MD as u32)
                .row_spacing(SPACING_SM as u32)
                .homogeneous(true)
                .css_classes(vec!["search-lockup-grid".to_string()])
                .hexpand(true)
                .build();

            for item in top_items.iter().take(6) {
                let card = self.create_lockup_card(item, sender);
                flow.append(&card);
            }

            sec_box.append(&flow);
            container.append(&sec_box);
        }

        // 2. Songs Section (top 5 preview)
        if let Some(ref paged_tracks) = res.tracks
            && !paged_tracks.items.is_empty()
        {
            let sec_box = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(SPACING_XS)
                .build();

            let heading_btn = Self::create_heading_button("Songs", SearchTab::Songs, sender);
            sec_box.append(&heading_btn);

            for (idx, track) in paged_tracks.items.iter().take(5).enumerate() {
                let row = self.create_track_row(track, idx, sender);
                sec_box.append(row.widget());
                self.track_controllers.push(row);
            }

            container.append(&sec_box);
        }

        // 3. Artists Section (shelf preview)
        if let Some(ref paged_artists) = res.artists
            && !paged_artists.items.is_empty()
        {
            let sec_box = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(SPACING_XS)
                .build();

            let heading_btn = Self::create_heading_button("Artists", SearchTab::Artists, sender);
            sec_box.append(&heading_btn);

            let (scrolled, shelf_box) = create_shelf_container();
            let mut shelf_senders = Vec::new();
            for (idx, artist) in paged_artists.items.iter().enumerate() {
                let eager = idx < 6;
                let card = self.create_artist_card(artist, eager, sender);
                shelf_box.append(card.widget());
                if !eager {
                    shelf_senders.push((idx, card.sender().clone()));
                }
                self.card_controllers.push(card);
            }
            hook_shelf_artwork_trigger(&scrolled, ARTWORK_SHELF_SIZE as f64, shelf_senders);
            sec_box.append(&scrolled);
            container.append(&sec_box);
        }

        // 4. Albums Section (shelf preview)
        if let Some(ref paged_albums) = res.albums
            && !paged_albums.items.is_empty()
        {
            let sec_box = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(SPACING_XS)
                .build();

            let heading_btn = Self::create_heading_button("Albums", SearchTab::Albums, sender);
            sec_box.append(&heading_btn);

            let (scrolled, shelf_box) = create_shelf_container();
            let mut shelf_senders = Vec::new();
            for (idx, album) in paged_albums.items.iter().enumerate() {
                let eager = idx < 6;
                let card = self.create_album_card(album, eager, sender);
                shelf_box.append(card.widget());
                if !eager {
                    shelf_senders.push((idx, card.sender().clone()));
                }
                self.card_controllers.push(card);
            }
            hook_shelf_artwork_trigger(&scrolled, ARTWORK_SHELF_SIZE as f64, shelf_senders);
            sec_box.append(&scrolled);
            container.append(&sec_box);
        }

        // 5. Playlists Section (shelf preview)
        if let Some(ref paged_playlists) = res.playlists
            && !paged_playlists.items.is_empty()
        {
            let sec_box = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(SPACING_XS)
                .build();

            let heading_btn =
                Self::create_heading_button("Playlists", SearchTab::Playlists, sender);
            sec_box.append(&heading_btn);

            let (scrolled, shelf_box) = create_shelf_container();
            let mut shelf_senders = Vec::new();
            for (idx, playlist) in paged_playlists.items.iter().enumerate() {
                let eager = idx < 6;
                let card = self.create_playlist_card(playlist, eager, sender);
                shelf_box.append(card.widget());
                if !eager {
                    shelf_senders.push((idx, card.sender().clone()));
                }
                self.card_controllers.push(card);
            }
            hook_shelf_artwork_trigger(&scrolled, ARTWORK_SHELF_SIZE as f64, shelf_senders);
            sec_box.append(&scrolled);
            container.append(&sec_box);
        }

        // 6. Stations Section (shelf preview)
        if let Some(ref paged_stations) = res.stations
            && !paged_stations.items.is_empty()
        {
            let sec_box = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(SPACING_XS)
                .build();

            let heading_btn = Self::create_heading_button("Stations", SearchTab::Stations, sender);
            sec_box.append(&heading_btn);

            let (scrolled, shelf_box) = create_shelf_container();
            let mut shelf_senders = Vec::new();
            for (idx, station) in paged_stations.items.iter().enumerate() {
                let eager = idx < 6;
                let card = self.create_station_card(station, eager, sender);
                shelf_box.append(card.widget());
                if !eager {
                    shelf_senders.push((idx, card.sender().clone()));
                }
                self.card_controllers.push(card);
            }
            hook_shelf_artwork_trigger(&scrolled, ARTWORK_SHELF_SIZE as f64, shelf_senders);
            sec_box.append(&scrolled);
            container.append(&sec_box);
        }
    }

    /// Dedicated view for Songs tab: Full tracklist with pagination.
    fn render_songs_tab(&mut self, container: &gtk::Box, sender: &ComponentSender<Self>) {
        let Some(ref res) = self.results.clone() else {
            return;
        };
        let Some(ref paged_tracks) = res.tracks else {
            return;
        };

        let sec_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(SPACING_XS)
            .build();

        let count_str = format!("{} songs", paged_tracks.items.len());
        let header_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(SPACING_SM)
            .valign(gtk::Align::Baseline)
            .build();

        let title_lbl = gtk::Label::builder()
            .label("Songs")
            .xalign(0.0)
            .css_classes(vec!["section-title".to_string()])
            .build();
        header_row.append(&title_lbl);

        let count_lbl = gtk::Label::builder()
            .label(&count_str)
            .xalign(0.0)
            .css_classes(vec!["section-subtitle".to_string()])
            .build();
        header_row.append(&count_lbl);

        sec_box.append(&header_row);

        for (idx, track) in paged_tracks.items.iter().enumerate() {
            let row = self.create_track_row(track, idx, sender);
            sec_box.append(row.widget());
            self.track_controllers.push(row);
        }

        if paged_tracks.next_cursor.is_some() {
            let load_more_btn = gtk::Button::builder()
                .label(if self.is_loading_more {
                    "Loading..."
                } else {
                    "Load More Songs"
                })
                .css_classes(vec![
                    "suggested-action".to_string(),
                    "pill-button".to_string(),
                ])
                .halign(gtk::Align::Center)
                .margin_top(SPACING_LG)
                .sensitive(!self.is_loading_more)
                .build();
            let s = sender.clone();
            load_more_btn.connect_clicked(move |_| {
                s.input(SearchInput::LoadMoreTab(SearchTab::Songs));
            });
            sec_box.append(&load_more_btn);
        }

        container.append(&sec_box);
    }

    /// Dedicated view for Artists tab: Responsive multi-column grid with pagination.
    fn render_artists_tab(&mut self, container: &gtk::Box, sender: &ComponentSender<Self>) {
        let Some(ref res) = self.results.clone() else {
            return;
        };
        let Some(ref paged_artists) = res.artists else {
            return;
        };

        let sec_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(SPACING_MD)
            .build();

        let count_str = format!("{} artists", paged_artists.items.len());
        let header_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(SPACING_SM)
            .valign(gtk::Align::Baseline)
            .build();

        let title_lbl = gtk::Label::builder()
            .label("Artists")
            .xalign(0.0)
            .css_classes(vec!["section-title".to_string()])
            .build();
        header_row.append(&title_lbl);

        let count_lbl = gtk::Label::builder()
            .label(&count_str)
            .xalign(0.0)
            .css_classes(vec!["section-subtitle".to_string()])
            .build();
        header_row.append(&count_lbl);
        sec_box.append(&header_row);

        let flow = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .min_children_per_line(2)
            .max_children_per_line(10)
            .column_spacing(GRID_GAP)
            .row_spacing(GRID_GAP)
            .homogeneous(true)
            .build();

        for (idx, artist) in paged_artists.items.iter().enumerate() {
            let eager = idx < 12;
            let card = self.create_artist_card(artist, eager, sender);
            flow.append(card.widget());
            self.card_controllers.push(card);
        }

        sec_box.append(&flow);

        if paged_artists.next_cursor.is_some() {
            let load_more_btn = gtk::Button::builder()
                .label(if self.is_loading_more {
                    "Loading..."
                } else {
                    "Load More Artists"
                })
                .css_classes(vec![
                    "suggested-action".to_string(),
                    "pill-button".to_string(),
                ])
                .halign(gtk::Align::Center)
                .margin_top(SPACING_LG)
                .sensitive(!self.is_loading_more)
                .build();
            let s = sender.clone();
            load_more_btn.connect_clicked(move |_| {
                s.input(SearchInput::LoadMoreTab(SearchTab::Artists));
            });
            sec_box.append(&load_more_btn);
        }

        container.append(&sec_box);
    }

    /// Dedicated view for Albums tab: Responsive multi-column grid with pagination.
    fn render_albums_tab(&mut self, container: &gtk::Box, sender: &ComponentSender<Self>) {
        let Some(ref res) = self.results.clone() else {
            return;
        };
        let Some(ref paged_albums) = res.albums else {
            return;
        };

        let sec_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(SPACING_MD)
            .build();

        let count_str = format!("{} albums", paged_albums.items.len());
        let header_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(SPACING_SM)
            .valign(gtk::Align::Baseline)
            .build();

        let title_lbl = gtk::Label::builder()
            .label("Albums")
            .xalign(0.0)
            .css_classes(vec!["section-title".to_string()])
            .build();
        header_row.append(&title_lbl);

        let count_lbl = gtk::Label::builder()
            .label(&count_str)
            .xalign(0.0)
            .css_classes(vec!["section-subtitle".to_string()])
            .build();
        header_row.append(&count_lbl);
        sec_box.append(&header_row);

        let flow = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .min_children_per_line(2)
            .max_children_per_line(10)
            .column_spacing(GRID_GAP)
            .row_spacing(GRID_GAP)
            .homogeneous(true)
            .build();

        for (idx, album) in paged_albums.items.iter().enumerate() {
            let eager = idx < 12;
            let card = self.create_album_card(album, eager, sender);
            flow.append(card.widget());
            self.card_controllers.push(card);
        }

        sec_box.append(&flow);

        if paged_albums.next_cursor.is_some() {
            let load_more_btn = gtk::Button::builder()
                .label(if self.is_loading_more {
                    "Loading..."
                } else {
                    "Load More Albums"
                })
                .css_classes(vec![
                    "suggested-action".to_string(),
                    "pill-button".to_string(),
                ])
                .halign(gtk::Align::Center)
                .margin_top(SPACING_LG)
                .sensitive(!self.is_loading_more)
                .build();
            let s = sender.clone();
            load_more_btn.connect_clicked(move |_| {
                s.input(SearchInput::LoadMoreTab(SearchTab::Albums));
            });
            sec_box.append(&load_more_btn);
        }

        container.append(&sec_box);
    }

    /// Dedicated view for Playlists tab: Responsive multi-column grid with pagination.
    fn render_playlists_tab(&mut self, container: &gtk::Box, sender: &ComponentSender<Self>) {
        let Some(ref res) = self.results.clone() else {
            return;
        };
        let Some(ref paged_playlists) = res.playlists else {
            return;
        };

        let sec_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(SPACING_MD)
            .build();

        let count_str = format!("{} playlists", paged_playlists.items.len());
        let header_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(SPACING_SM)
            .valign(gtk::Align::Baseline)
            .build();

        let title_lbl = gtk::Label::builder()
            .label("Playlists")
            .xalign(0.0)
            .css_classes(vec!["section-title".to_string()])
            .build();
        header_row.append(&title_lbl);

        let count_lbl = gtk::Label::builder()
            .label(&count_str)
            .xalign(0.0)
            .css_classes(vec!["section-subtitle".to_string()])
            .build();
        header_row.append(&count_lbl);
        sec_box.append(&header_row);

        let flow = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .min_children_per_line(2)
            .max_children_per_line(10)
            .column_spacing(GRID_GAP)
            .row_spacing(GRID_GAP)
            .homogeneous(true)
            .build();

        for (idx, playlist) in paged_playlists.items.iter().enumerate() {
            let eager = idx < 12;
            let card = self.create_playlist_card(playlist, eager, sender);
            flow.append(card.widget());
            self.card_controllers.push(card);
        }

        sec_box.append(&flow);

        if paged_playlists.next_cursor.is_some() {
            let load_more_btn = gtk::Button::builder()
                .label(if self.is_loading_more {
                    "Loading..."
                } else {
                    "Load More Playlists"
                })
                .css_classes(vec![
                    "suggested-action".to_string(),
                    "pill-button".to_string(),
                ])
                .halign(gtk::Align::Center)
                .margin_top(SPACING_LG)
                .sensitive(!self.is_loading_more)
                .build();
            let s = sender.clone();
            load_more_btn.connect_clicked(move |_| {
                s.input(SearchInput::LoadMoreTab(SearchTab::Playlists));
            });
            sec_box.append(&load_more_btn);
        }

        container.append(&sec_box);
    }

    /// Dedicated view for Stations tab: Responsive multi-column grid with pagination.
    fn render_stations_tab(&mut self, container: &gtk::Box, sender: &ComponentSender<Self>) {
        let Some(ref res) = self.results.clone() else {
            return;
        };
        let Some(ref paged_stations) = res.stations else {
            return;
        };

        let sec_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(SPACING_MD)
            .build();

        let count_str = format!("{} stations", paged_stations.items.len());
        let header_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(SPACING_SM)
            .valign(gtk::Align::Baseline)
            .build();

        let title_lbl = gtk::Label::builder()
            .label("Stations")
            .xalign(0.0)
            .css_classes(vec!["section-title".to_string()])
            .build();
        header_row.append(&title_lbl);

        let count_lbl = gtk::Label::builder()
            .label(&count_str)
            .xalign(0.0)
            .css_classes(vec!["section-subtitle".to_string()])
            .build();
        header_row.append(&count_lbl);
        sec_box.append(&header_row);

        let flow = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .min_children_per_line(2)
            .max_children_per_line(10)
            .column_spacing(GRID_GAP)
            .row_spacing(GRID_GAP)
            .homogeneous(true)
            .build();

        for (idx, station) in paged_stations.items.iter().enumerate() {
            let eager = idx < 12;
            let card = self.create_station_card(station, eager, sender);
            flow.append(card.widget());
            self.card_controllers.push(card);
        }

        sec_box.append(&flow);

        if paged_stations.next_cursor.is_some() {
            let load_more_btn = gtk::Button::builder()
                .label(if self.is_loading_more {
                    "Loading..."
                } else {
                    "Load More Stations"
                })
                .css_classes(vec![
                    "suggested-action".to_string(),
                    "pill-button".to_string(),
                ])
                .halign(gtk::Align::Center)
                .margin_top(SPACING_LG)
                .sensitive(!self.is_loading_more)
                .build();
            let s = sender.clone();
            load_more_btn.connect_clicked(move |_| {
                s.input(SearchInput::LoadMoreTab(SearchTab::Stations));
            });
            sec_box.append(&load_more_btn);
        }

        container.append(&sec_box);
    }

    /// Create an authentic 64x64 lockup card for the Top Results grid.
    fn create_lockup_card(
        &self,
        item: &PageItemWire,
        sender: &ComponentSender<Self>,
    ) -> gtk::Button {
        let btn = gtk::Button::builder()
            .css_classes(vec!["flat".to_string(), "search-lockup-card".to_string()])
            .hexpand(true)
            .build();

        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(SPACING_SM)
            .valign(gtk::Align::Center)
            .hexpand(true)
            .build();

        let is_artist = matches!(item.entity, Some(MediaRef::Artist(_)));
        let art_class = if is_artist {
            "search-lockup-artwork-circular"
        } else {
            "search-lockup-artwork"
        };

        let art = SquareArtwork::new(ARTWORK_SEARCH_LOCKUP_SIZE, art_class);
        if let Some(ref artwork) = item.artwork {
            bind_artwork(
                art.picture(),
                &self.artwork_service,
                Some(artwork.url.clone()),
                128,
            );
        }
        root.append(&art);

        let text_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(SPACING_XXS / 2)
            .valign(gtk::Align::Center)
            .hexpand(true)
            .build();

        let title_label = gtk::Label::builder()
            .label(&item.title)
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .css_classes(vec!["search-lockup-title".to_string()])
            .build();
        text_box.append(&title_label);

        let sub_text = match item.entity {
            Some(MediaRef::Artist(_)) => "Artist".to_string(),
            Some(MediaRef::Song(_)) => {
                if let Some(ref s) = item.subtitle {
                    format!("Song · {s}")
                } else {
                    "Song".to_string()
                }
            }
            Some(MediaRef::Album(_)) => {
                if let Some(ref s) = item.subtitle {
                    format!("Album · {s}")
                } else {
                    "Album".to_string()
                }
            }
            Some(MediaRef::Playlist(_)) => {
                if let Some(ref s) = item.subtitle {
                    format!("Playlist · {s}")
                } else {
                    "Playlist".to_string()
                }
            }
            Some(MediaRef::Station(_)) => "Radio Station".to_string(),
            None => item.subtitle.clone().unwrap_or_default(),
        };

        let subtitle_label = gtk::Label::builder()
            .label(&sub_text)
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .css_classes(vec!["search-lockup-subtitle".to_string()])
            .build();
        text_box.append(&subtitle_label);

        root.append(&text_box);

        let play_btn = gtk::Button::builder()
            .icon_name(ICON_PLAY)
            .css_classes(vec![
                "flat".to_string(),
                "circular".to_string(),
                "search-lockup-play".to_string(),
            ])
            .valign(gtk::Align::Center)
            .halign(gtk::Align::End)
            .build();

        let s_play = sender.clone();
        let entity_clone = item.entity.clone();
        let recent_item = RecentSearchItem {
            id: item.id.clone(),
            title: item.title.clone(),
            subtitle: Some(sub_text.clone()),
            artwork_url: item.artwork.as_ref().map(|a| a.url.clone()),
            is_circular: is_artist,
            is_library: self.scope == SearchScopeWire::Library,
            route: item.open_route.clone(),
            media_ref: item.entity.clone(),
        };
        let recent_for_play = recent_item.clone();
        play_btn.connect_clicked(move |_| {
            s_play.input(SearchInput::RecordRecentSearch(recent_for_play.clone()));
            if let Some(ref mref) = entity_clone {
                let _ = s_play.output(SearchOutput::Play(mref.clone()));
            }
        });
        root.append(&play_btn);

        btn.set_child(Some(&root));

        let s = sender.clone();
        let open_route = item.open_route.clone();
        let entity = item.entity.clone();
        btn.connect_clicked(move |_| {
            s.input(SearchInput::RecordRecentSearch(recent_item.clone()));
            if let Some(ref route) = open_route {
                let _ = s.output(SearchOutput::Navigate(route.clone()));
            } else if let Some(ref mref) = entity {
                let _ = s.output(SearchOutput::Play(mref.clone()));
            }
        });

        btn
    }

    /// Create a clickable section heading with `>` chevron linking to the full category tab.
    fn create_heading_button(
        title: &str,
        target_tab: SearchTab,
        sender: &ComponentSender<Self>,
    ) -> gtk::Button {
        let btn = gtk::Button::builder()
            .css_classes(vec!["flat".to_string(), "search-heading-btn".to_string()])
            .build();

        let h_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(SPACING_XS)
            .valign(gtk::Align::Center)
            .build();

        let title_lbl = gtk::Label::builder()
            .label(title)
            .xalign(0.0)
            .css_classes(vec!["section-title".to_string()])
            .build();
        h_box.append(&title_lbl);

        let chevron = gtk::Image::from_icon_name("go-next-symbolic");
        chevron.set_pixel_size(ICON_GLYPH_SM);
        h_box.append(&chevron);

        btn.set_child(Some(&h_box));

        let s = sender.clone();
        btn.connect_clicked(move |_| {
            s.input(SearchInput::SelectTab(target_tab));
        });

        btn
    }

    fn create_track_row(
        &self,
        track: &Track,
        idx: usize,
        sender: &ComponentSender<Self>,
    ) -> Controller<TrackRow> {
        let recent = RecentSearchItem {
            id: track.id.to_string(),
            title: track.title.clone(),
            subtitle: Some(format!("Song · {}", track.artist_display())),
            artwork_url: track.artwork.as_ref().map(|a| a.url.clone()),
            is_circular: false,
            is_library: self.scope == SearchScopeWire::Library,
            route: None,
            media_ref: Some(track.id.clone()),
        };
        let s = sender.clone();
        let album_route = track
            .album
            .as_ref()
            .and_then(|a| a.id.as_ref())
            .map(|r| PageRoute::Album(r.id().to_string()));
        let artist_route = track
            .artists
            .first()
            .and_then(|a| a.id.as_ref())
            .map(|r| PageRoute::Artist(r.id().to_string()));
        TrackRow::builder()
            .launch(TrackRowInit {
                track: track.clone(),
                index: Some(idx),
                show_artwork: true,
                is_favorite: false,
                in_library: false,
                actions: Vec::new(),
                artwork_service: self.artwork_service.clone(),
                playlist_context: None,
                album_route,
                artist_route,
            })
            .forward(sender.output_sender(), move |out| {
                s.input(SearchInput::RecordRecentSearch(recent.clone()));
                match out {
                    TrackRowOutput::Play(r) => SearchOutput::Play(r),
                    TrackRowOutput::Action(a) => SearchOutput::Action(a),
                    TrackRowOutput::ViewCredits(r) => SearchOutput::ViewCredits(r),
                    TrackRowOutput::AddToPlaylist(r) => SearchOutput::ShowAddToPlaylist(r),
                    TrackRowOutput::RemoveFromPlaylist { .. } => unreachable!(),
                    TrackRowOutput::Navigate(r) => SearchOutput::Navigate(r),
                    TrackRowOutput::CopyLink(u) => SearchOutput::CopyLink(u),
                }
            })
    }

    fn create_artist_card(
        &self,
        artist: &Artist,
        eager: bool,
        sender: &ComponentSender<Self>,
    ) -> Controller<MediaCard> {
        let recent = RecentSearchItem {
            id: artist.id.to_string(),
            title: artist.name.clone(),
            subtitle: Some("Artist".to_string()),
            artwork_url: artist.artwork.as_ref().map(|a| a.url.clone()),
            is_circular: true,
            is_library: self.scope == SearchScopeWire::Library,
            route: Some(PageRoute::Artist(artist.id.id().to_string())),
            media_ref: None,
        };
        let s = sender.clone();
        MediaCard::builder()
            .launch(MediaCardInit {
                id: artist.id.to_string(),
                title: artist.name.clone(),
                subtitle: None,
                subtitle_route: None,
                overline: None,
                artwork_url: artist.artwork.as_ref().map(|a| a.url.clone()),
                entity: Some(artist.id.clone()),
                open_route: Some(PageRoute::Artist(artist.id.id().to_string())),
                size: ARTWORK_SHELF_SIZE,
                is_circular: true,
                artwork_service: self.artwork_service.clone(),
                eager,
            })
            .forward(sender.output_sender(), move |out| {
                s.input(SearchInput::RecordRecentSearch(recent.clone()));
                match out {
                    MediaCardOutput::Navigate(r) => SearchOutput::Navigate(r),
                    MediaCardOutput::Play(r) => SearchOutput::Play(r),
                    MediaCardOutput::CopyLink(u) => SearchOutput::CopyLink(u),
                }
            })
    }

    fn create_album_card(
        &self,
        album: &Album,
        eager: bool,
        sender: &ComponentSender<Self>,
    ) -> Controller<MediaCard> {
        let recent = RecentSearchItem {
            id: album.id.to_string(),
            title: album.title.clone(),
            subtitle: Some(format!("Album · {}", album.artist_display())),
            artwork_url: album.artwork.as_ref().map(|a| a.url.clone()),
            is_circular: false,
            is_library: self.scope == SearchScopeWire::Library,
            route: Some(PageRoute::Album(album.id.id().to_string())),
            media_ref: None,
        };
        let s = sender.clone();
        let subtitle_route = album
            .artists
            .first()
            .and_then(|a| a.id.as_ref())
            .map(|r| PageRoute::Artist(r.id().to_string()));
        MediaCard::builder()
            .launch(MediaCardInit {
                id: album.id.to_string(),
                title: album.title.clone(),
                subtitle: Some(album.artist_display()),
                subtitle_route,
                overline: None,
                artwork_url: album.artwork.as_ref().map(|a| a.url.clone()),
                entity: Some(album.id.clone()),
                open_route: Some(PageRoute::Album(album.id.id().to_string())),
                size: ARTWORK_SHELF_SIZE,
                is_circular: false,
                artwork_service: self.artwork_service.clone(),
                eager,
            })
            .forward(sender.output_sender(), move |out| {
                s.input(SearchInput::RecordRecentSearch(recent.clone()));
                match out {
                    MediaCardOutput::Navigate(r) => SearchOutput::Navigate(r),
                    MediaCardOutput::Play(r) => SearchOutput::Play(r),
                    MediaCardOutput::CopyLink(u) => SearchOutput::CopyLink(u),
                }
            })
    }

    fn create_playlist_card(
        &self,
        playlist: &Playlist,
        eager: bool,
        sender: &ComponentSender<Self>,
    ) -> Controller<MediaCard> {
        let recent = RecentSearchItem {
            id: playlist.id.to_string(),
            title: playlist.title.clone(),
            subtitle: playlist.curator.clone(),
            artwork_url: playlist.artwork.as_ref().map(|a| a.url.clone()),
            is_circular: false,
            is_library: self.scope == SearchScopeWire::Library,
            route: Some(PageRoute::Playlist(playlist.id.id().to_string())),
            media_ref: None,
        };
        let s = sender.clone();
        MediaCard::builder()
            .launch(MediaCardInit {
                id: playlist.id.to_string(),
                title: playlist.title.clone(),
                subtitle: playlist.curator.clone(),
                subtitle_route: None,
                overline: None,
                artwork_url: playlist.artwork.as_ref().map(|a| a.url.clone()),
                entity: Some(playlist.id.clone()),
                open_route: Some(PageRoute::Playlist(playlist.id.id().to_string())),
                size: ARTWORK_SHELF_SIZE,
                is_circular: false,
                artwork_service: self.artwork_service.clone(),
                eager,
            })
            .forward(sender.output_sender(), move |out| {
                s.input(SearchInput::RecordRecentSearch(recent.clone()));
                match out {
                    MediaCardOutput::Navigate(r) => SearchOutput::Navigate(r),
                    MediaCardOutput::Play(r) => SearchOutput::Play(r),
                    MediaCardOutput::CopyLink(u) => SearchOutput::CopyLink(u),
                }
            })
    }

    fn create_station_card(
        &self,
        station: &PageItemWire,
        eager: bool,
        sender: &ComponentSender<Self>,
    ) -> Controller<MediaCard> {
        let recent = RecentSearchItem {
            id: station.id.clone(),
            title: station.title.clone(),
            subtitle: station.subtitle.clone(),
            artwork_url: station.artwork.as_ref().map(|a| a.url.clone()),
            is_circular: false,
            is_library: self.scope == SearchScopeWire::Library,
            route: None,
            media_ref: station.entity.clone(),
        };
        let s = sender.clone();
        MediaCard::builder()
            .launch(MediaCardInit {
                id: station.id.clone(),
                title: station.title.clone(),
                subtitle: station.subtitle.clone(),
                subtitle_route: station.artist_route.clone(),
                overline: None,
                artwork_url: station.artwork.as_ref().map(|a| a.url.clone()),
                entity: station.entity.clone(),
                open_route: None,
                size: ARTWORK_SHELF_SIZE,
                is_circular: false,
                artwork_service: self.artwork_service.clone(),
                eager,
            })
            .forward(sender.output_sender(), move |out| {
                s.input(SearchInput::RecordRecentSearch(recent.clone()));
                match out {
                    MediaCardOutput::Navigate(r) => SearchOutput::Navigate(r),
                    MediaCardOutput::Play(r) => SearchOutput::Play(r),
                    MediaCardOutput::CopyLink(u) => SearchOutput::CopyLink(u),
                }
            })
    }
}
