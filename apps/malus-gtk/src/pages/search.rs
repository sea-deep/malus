//! Categorized Search Results Page.

use malus_client::MalusClient;
use malus_ipc::wire::{PageActionWire, SearchKindWire, SearchResultsWire};
use malus_model::{MediaRef, PageRoute};
use relm4::gtk::{self, prelude::*};
use relm4::prelude::*;

use crate::design::tokens::*;
use crate::services::ArtworkService;
use crate::widgets::empty_state::create_empty_state;
use crate::widgets::media_card::{MediaCard, MediaCardInit, MediaCardOutput};
use crate::widgets::media_shelf::{create_shelf_container, hook_shelf_artwork_trigger};
use crate::widgets::track_row::{TrackRow, TrackRowInit, TrackRowOutput};

pub struct SearchPage {
    client: MalusClient,
    artwork_service: ArtworkService,
    query: String,
    results: Option<SearchResultsWire>,
    is_loading: bool,
    error_message: Option<String>,
    card_controllers: Vec<Controller<MediaCard>>,
    track_controllers: Vec<Controller<TrackRow>>,
    search_generation: u64,
}

#[derive(Debug)]
pub enum SearchInput {
    MediaState(malus_model::AccountMediaState),
    QueryChanged(String),
    ExecuteSearch(String),
    ExecuteSearchDebounced { query: String, generation: u64 },
}

#[derive(Debug)]
pub enum SearchCmd {
    ResultsLoaded {
        generation: u64,
        query: String,
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
                set_spacing: 24,
                set_margin_start: PAGE_PADDING_NORMAL,
                set_margin_end: PAGE_PADDING_NORMAL,
                set_margin_top: 24,
                set_margin_bottom: 48,

                // Title Area
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 4,

                    gtk::Label {
                        set_xalign: 0.0,
                        add_css_class: "page-title",
                        set_text: "Search",
                    },

                    gtk::Label {
                        set_xalign: 0.0,
                        add_css_class: "page-subtitle",
                        #[watch]
                        set_visible: !model.query.is_empty(),
                        #[watch]
                        set_text: &format!("Results for \"{}\"", model.query),
                    },
                },

                // Loading Spinner
                #[name(spinner)]
                gtk::Spinner {
                    set_size_request: (32, 32),
                    set_halign: gtk::Align::Center,
                    set_margin_top: 48,
                    #[watch]
                    set_spinning: model.is_loading,
                    #[watch]
                    set_visible: model.is_loading,
                },

                // Dynamic Results Container
                #[name(results_container)]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 24,
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
            is_loading: false,
            error_message: None,
            card_controllers: Vec::new(),
            track_controllers: Vec::new(),
            search_generation: 0,
        };

        let mut widgets = view_output!();
        let content = widgets.content_box.clone();
        let last_narrow = std::cell::Cell::new(false);
        root.add_tick_callback(move |page, _| {
            let narrow = page.width() < 600;
            if last_narrow.replace(narrow) != narrow {
                let padding = if narrow {
                    PAGE_PADDING_NARROW
                } else {
                    PAGE_PADDING_NORMAL
                };
                content.set_margin_start(padding);
                content.set_margin_end(padding);
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
                    self.error_message = None;
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
                    let kinds = vec![
                        SearchKindWire::Track,
                        SearchKindWire::Album,
                        SearchKindWire::Artist,
                        SearchKindWire::Playlist,
                    ];
                    sender.oneshot_command(async move {
                        let res = c
                            .search(&query, kinds, Some(10), None)
                            .await
                            .map_err(|e| e.to_string());
                        SearchCmd::ResultsLoaded {
                            generation,
                            query,
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
                    let c = self.client.clone();
                    let kinds = vec![
                        SearchKindWire::Track,
                        SearchKindWire::Album,
                        SearchKindWire::Artist,
                        SearchKindWire::Playlist,
                    ];
                    sender.oneshot_command(async move {
                        let res = c
                            .search(&trimmed, kinds, Some(10), None)
                            .await
                            .map_err(|e| e.to_string());
                        SearchCmd::ResultsLoaded {
                            generation,
                            query: trimmed,
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
            } => {
                if generation == self.search_generation && query == self.query {
                    self.is_loading = false;
                    match result {
                        Ok(res) => {
                            self.results = Some(res);
                            self.error_message = None;
                        }
                        Err(e) => {
                            self.results = None;
                            self.error_message = Some(e);
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
        let is_empty = match &message {
            SearchInput::QueryChanged(q) => q.trim().is_empty(),
            _ => false,
        };
        self.update(message, sender.clone(), root);
        self.update_view(widgets, sender.clone());
        if is_empty {
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
        let SearchCmd::ResultsLoaded {
            generation, query, ..
        } = &message;
        if *generation != self.search_generation || *query != self.query {
            return;
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
        while let Some(child) = widgets.results_container.first_child() {
            widgets.results_container.remove(&child);
        }

        if self.query.is_empty() {
            let empty = create_empty_state(
                ICON_SEARCH,
                "Search Apple Music",
                Some("Find songs, artists, albums, and playlists"),
            );
            widgets.results_container.append(&empty);
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

        if let Some(ref res) = self.results {
            if res.is_empty() {
                let empty = create_empty_state(
                    ICON_SEARCH,
                    "No Results Found",
                    Some("Check the spelling or try a different search"),
                );
                widgets.results_container.append(&empty);
                return;
            }

            // 1. Top Songs Section
            if let Some(ref paged_tracks) = res.tracks
                && !paged_tracks.items.is_empty()
            {
                let sec_box = gtk::Box::builder()
                    .orientation(gtk::Orientation::Vertical)
                    .spacing(8)
                    .build();

                let sec_title = gtk::Label::builder()
                    .label("Songs")
                    .xalign(0.0)
                    .css_classes(vec!["section-title".to_string()])
                    .build();
                sec_box.append(&sec_title);

                for (idx, track) in paged_tracks.items.iter().enumerate() {
                    let row = TrackRow::builder()
                        .launch(TrackRowInit {
                            track: track.clone(),
                            index: Some(idx),
                            show_artwork: true,
                            is_favorite: false,
                            in_library: false,
                            actions: Vec::new(),
                            artwork_service: self.artwork_service.clone(),
                            playlist_context: None,
                        })
                        .forward(sender.output_sender(), |out| match out {
                            TrackRowOutput::Play(r) => SearchOutput::Play(r),
                            TrackRowOutput::Action(a) => SearchOutput::Action(a),
                            TrackRowOutput::ViewCredits(r) => SearchOutput::ViewCredits(r),
                            TrackRowOutput::AddToPlaylist(r) => SearchOutput::ShowAddToPlaylist(r),
                            TrackRowOutput::RemoveFromPlaylist { .. } => unreachable!(),
                        });
                    sec_box.append(row.widget());
                    self.track_controllers.push(row);
                }

                widgets.results_container.append(&sec_box);
            }

            // 2. Albums Section
            if let Some(ref paged_albums) = res.albums
                && !paged_albums.items.is_empty()
            {
                let sec_box = gtk::Box::builder()
                    .orientation(gtk::Orientation::Vertical)
                    .spacing(8)
                    .build();

                let sec_title = gtk::Label::builder()
                    .label("Albums")
                    .xalign(0.0)
                    .css_classes(vec!["section-title".to_string()])
                    .build();
                sec_box.append(&sec_title);

                let (scrolled, shelf_box) = create_shelf_container();
                let mut shelf_senders = Vec::new();
                for (idx, album) in paged_albums.items.iter().enumerate() {
                    let eager = idx < 6;
                    let card = MediaCard::builder()
                        .launch(MediaCardInit {
                            id: album.id.to_string(),
                            title: album.title.clone(),
                            subtitle: Some(album.artist_display()),
                            overline: None,
                            artwork_url: album.artwork.as_ref().map(|a| a.url.clone()),
                            entity: Some(album.id.clone()),
                            open_route: Some(PageRoute::Album(album.id.id().to_string())),
                            size: ARTWORK_SHELF_SIZE,
                            is_circular: false,
                            artwork_service: self.artwork_service.clone(),
                            eager,
                        })
                        .forward(sender.output_sender(), |out| match out {
                            MediaCardOutput::Navigate(r) => SearchOutput::Navigate(r),
                            MediaCardOutput::Play(r) => SearchOutput::Play(r),
                        });
                    shelf_box.append(card.widget());
                    if !eager {
                        shelf_senders.push((idx, card.sender().clone()));
                    }
                    self.card_controllers.push(card);
                }

                hook_shelf_artwork_trigger(&scrolled, ARTWORK_SHELF_SIZE as f64, shelf_senders);

                sec_box.append(&scrolled);
                widgets.results_container.append(&sec_box);
            }

            // 3. Artists Section
            if let Some(ref paged_artists) = res.artists
                && !paged_artists.items.is_empty()
            {
                let sec_box = gtk::Box::builder()
                    .orientation(gtk::Orientation::Vertical)
                    .spacing(8)
                    .build();

                let sec_title = gtk::Label::builder()
                    .label("Artists")
                    .xalign(0.0)
                    .css_classes(vec!["section-title".to_string()])
                    .build();
                sec_box.append(&sec_title);

                let (scrolled, shelf_box) = create_shelf_container();
                let mut shelf_senders = Vec::new();
                for (idx, artist) in paged_artists.items.iter().enumerate() {
                    let eager = idx < 6;
                    let card = MediaCard::builder()
                        .launch(MediaCardInit {
                            id: artist.id.to_string(),
                            title: artist.name.clone(),
                            subtitle: None,
                            overline: None,
                            artwork_url: artist.artwork.as_ref().map(|a| a.url.clone()),
                            entity: Some(artist.id.clone()),
                            open_route: Some(PageRoute::Artist(artist.id.id().to_string())),
                            size: ARTWORK_SHELF_SIZE,
                            is_circular: true,
                            artwork_service: self.artwork_service.clone(),
                            eager,
                        })
                        .forward(sender.output_sender(), |out| match out {
                            MediaCardOutput::Navigate(r) => SearchOutput::Navigate(r),
                            MediaCardOutput::Play(r) => SearchOutput::Play(r),
                        });
                    shelf_box.append(card.widget());
                    if !eager {
                        shelf_senders.push((idx, card.sender().clone()));
                    }
                    self.card_controllers.push(card);
                }

                hook_shelf_artwork_trigger(&scrolled, ARTWORK_SHELF_SIZE as f64, shelf_senders);

                sec_box.append(&scrolled);
                widgets.results_container.append(&sec_box);
            }

            // 4. Playlists Section
            if let Some(ref paged_playlists) = res.playlists
                && !paged_playlists.items.is_empty()
            {
                let sec_box = gtk::Box::builder()
                    .orientation(gtk::Orientation::Vertical)
                    .spacing(8)
                    .build();

                let sec_title = gtk::Label::builder()
                    .label("Playlists")
                    .xalign(0.0)
                    .css_classes(vec!["section-title".to_string()])
                    .build();
                sec_box.append(&sec_title);

                let (scrolled, shelf_box) = create_shelf_container();
                let mut shelf_senders = Vec::new();
                for (idx, playlist) in paged_playlists.items.iter().enumerate() {
                    let eager = idx < 6;
                    let card = MediaCard::builder()
                        .launch(MediaCardInit {
                            id: playlist.id.to_string(),
                            title: playlist.title.clone(),
                            subtitle: playlist.curator.clone(),
                            overline: None,
                            artwork_url: playlist.artwork.as_ref().map(|a| a.url.clone()),
                            entity: Some(playlist.id.clone()),
                            open_route: Some(PageRoute::Playlist(playlist.id.id().to_string())),
                            size: ARTWORK_SHELF_SIZE,
                            is_circular: false,
                            artwork_service: self.artwork_service.clone(),
                            eager,
                        })
                        .forward(sender.output_sender(), |out| match out {
                            MediaCardOutput::Navigate(r) => SearchOutput::Navigate(r),
                            MediaCardOutput::Play(r) => SearchOutput::Play(r),
                        });
                    shelf_box.append(card.widget());
                    if !eager {
                        shelf_senders.push((idx, card.sender().clone()));
                    }
                    self.card_controllers.push(card);
                }

                hook_shelf_artwork_trigger(&scrolled, ARTWORK_SHELF_SIZE as f64, shelf_senders);

                sec_box.append(&scrolled);
                widgets.results_container.append(&sec_box);
            }
        }
    }
}
