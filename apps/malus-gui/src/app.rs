//! Root application component for Malus GUI.
//!
//! Owns:
//! - Daemon connection status & event subscription
//! - Navigation history and routing stack
//! - Authoritative SearchSession
//! - Browse provider state & capability cache
//! - Component coordination and event routing

use malus_client::{
    ClientError, ClientEvent, ConnectionStatus, MalusClient, PlayerStatusWire, ProviderInfoWire,
};
use relm4::adw::{self, prelude::*};
use relm4::gtk;
use relm4::prelude::*;

use crate::artwork::ArtworkService;
use crate::components::*;
use crate::model::{LibraryTab, NavigationHistory, ProviderCache, Route, SearchSession};

pub struct MalusApp {
    client: MalusClient,
    _artwork_service: ArtworkService,

    // Navigation & Sessions
    history: NavigationHistory,
    search_session: SearchSession,
    browse_provider: String,
    provider_cache: ProviderCache,
    connection_status: ConnectionStatus,

    // Child Components
    provider_selector: Controller<ProviderSelector>,
    search_view: Controller<SearchView>,
    library_view: Controller<LibraryView>,
    album_view: Controller<AlbumView>,
    artist_view: Controller<ArtistView>,
    playlist_view: Controller<PlaylistView>,
    player_bar: Controller<PlayerBar>,
}

#[derive(Debug)]
pub enum AppInput {
    // Navigation
    Navigate(Route),
    GoBack,
    GoForward,
    SearchEdited(String),

    // Provider & Auth
    BrowseProviderSelected(String),
    BeginAuth(String),

    // Playback
    PlayTrack(String),

    // Keyboard
    FocusSearch,
    TogglePlayback,
}

#[derive(Debug)]
pub enum AppCmd {
    ConnectionStatusChanged(ConnectionStatus),
    DaemonEvent(ClientEvent),
    InitialData {
        providers: Vec<ProviderInfoWire>,
        status: Option<PlayerStatusWire>,
    },
    AuthResult(Result<(), ClientError>),
    PlayResult(Result<(), ClientError>),
}

#[relm4::component(pub)]
impl Component for MalusApp {
    type Init = MalusClient;
    type Input = AppInput;
    type Output = ();
    type CommandOutput = AppCmd;

    view! {
        #[root]
        adw::ApplicationWindow {
            set_default_size: (1024, 700),
            set_title: Some("Malus"),

            adw::ToolbarView {
                // Top Header Box (HeaderBar + Banner)
                add_top_bar = &gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,

                    adw::HeaderBar {
                        add_css_class: "compact-toolbar",

                        #[wrap(Some)]
                        set_title_widget = &gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 12,
                            set_halign: gtk::Align::Center,

                            // Centered Search Entry
                            #[name(search_entry)]
                            gtk::SearchEntry {
                                add_css_class: "toolbar-search",
                                set_placeholder_text: Some("Search music..."),
                                set_size_request: (360, -1),
                                connect_search_changed[sender] => move |entry| {
                                    sender.input(AppInput::SearchEdited(entry.text().to_string()));
                                },
                            },
                        },

                        // Left controls: Back / Forward buttons
                        pack_start = &gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 4,

                            #[name(back_btn)]
                            gtk::Button {
                                set_icon_name: "go-previous-symbolic",
                                add_css_class: "toolbar-btn",
                                #[watch]
                                set_sensitive: model.history.can_go_back(),
                                connect_clicked => AppInput::GoBack,
                            },

                            #[name(forward_btn)]
                            gtk::Button {
                                set_icon_name: "go-next-symbolic",
                                add_css_class: "toolbar-btn",
                                #[watch]
                                set_sensitive: model.history.can_go_forward(),
                                connect_clicked => AppInput::GoForward,
                            },
                        },

                        // Right controls: Provider dropdown & Auth
                        pack_end = &gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 8,

                            #[local_ref]
                            provider_widget -> gtk::Box {},
                        },
                    },

                    // Connection warning banner
                    #[name(conn_banner)]
                    adw::Banner {
                        #[watch]
                        set_revealed: model.connection_status != ConnectionStatus::Connected,
                        #[watch]
                        set_title: model.banner_text(),
                    },
                },

                // Main content: OverlaySplitView (sidebar + content stack)
                #[name(split_view)]
                #[wrap(Some)]
                set_content = &adw::OverlaySplitView {
                    set_min_sidebar_width: 175.0,
                    set_max_sidebar_width: 210.0,

                    // Sidebar Navigation
                    #[wrap(Some)]
                    set_sidebar = &gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 4,
                        add_css_class: "sidebar",

                        gtk::Button {
                            set_label: "Search",
                            add_css_class: "nav-button",
                            #[watch]
                            set_css_classes: if *model.history.current() == Route::Search {
                                &["nav-button", "active"]
                            } else {
                                &["nav-button"]
                            },
                            connect_clicked => AppInput::Navigate(Route::Search),
                        },

                        gtk::Label {
                            set_text: "LIBRARY",
                            set_xalign: 0.0,
                            add_css_class: "sidebar-heading",
                            #[watch]
                            set_visible: model.has_any_library_cap(),
                        },

                        gtk::Button {
                            set_label: "Tracks",
                            add_css_class: "nav-button",
                            #[watch]
                            set_visible: model.has_browse_cap("library.tracks"),
                            #[watch]
                            set_css_classes: if *model.history.current() == Route::Library(LibraryTab::Tracks) {
                                &["nav-button", "active"]
                            } else {
                                &["nav-button"]
                            },
                            connect_clicked => AppInput::Navigate(Route::Library(LibraryTab::Tracks)),
                        },

                        gtk::Button {
                            set_label: "Albums",
                            add_css_class: "nav-button",
                            #[watch]
                            set_visible: model.has_browse_cap("library.albums"),
                            #[watch]
                            set_css_classes: if *model.history.current() == Route::Library(LibraryTab::Albums) {
                                &["nav-button", "active"]
                            } else {
                                &["nav-button"]
                            },
                            connect_clicked => AppInput::Navigate(Route::Library(LibraryTab::Albums)),
                        },

                        gtk::Button {
                            set_label: "Playlists",
                            add_css_class: "nav-button",
                            #[watch]
                            set_visible: model.has_browse_cap("library.playlists"),
                            #[watch]
                            set_css_classes: if *model.history.current() == Route::Library(LibraryTab::Playlists) {
                                &["nav-button", "active"]
                            } else {
                                &["nav-button"]
                            },
                            connect_clicked => AppInput::Navigate(Route::Library(LibraryTab::Playlists)),
                        },
                    },

                    // Content Stack
                    #[name(content_stack)]
                    #[wrap(Some)]
                    set_content = &gtk::Stack {
                        set_transition_type: gtk::StackTransitionType::Crossfade,
                        #[watch]
                        set_visible_child_name: model.active_route_name(),
                    },
                },

                // Bottom Player Bar
                #[local_ref]
                add_bottom_bar = player_widget -> gtk::Box {},
            },
        }
    }

    fn init(
        client: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let artwork_service = ArtworkService::new();
        let default_provider = "apple".to_string();

        let provider_selector = ProviderSelector::builder()
            .launch((Vec::new(), default_provider.clone()))
            .forward(sender.input_sender(), |out| match out {
                ProviderSelectorOutput::BrowseProviderSelected(id) => {
                    AppInput::BrowseProviderSelected(id)
                }
                ProviderSelectorOutput::BeginAuth(id) => AppInput::BeginAuth(id),
            });

        let search_view =
            SearchView::builder()
                .launch(client.clone())
                .forward(sender.input_sender(), |out| match out {
                    SearchViewOutput::PlayTrack(id) => AppInput::PlayTrack(id),
                    SearchViewOutput::OpenAlbum(id) => AppInput::Navigate(Route::AlbumDetail(id)),
                    SearchViewOutput::OpenArtist(id) => AppInput::Navigate(Route::ArtistDetail(id)),
                    SearchViewOutput::OpenPlaylist(id) => {
                        AppInput::Navigate(Route::PlaylistDetail(id))
                    }
                });

        let library_view =
            LibraryView::builder()
                .launch(client.clone())
                .forward(sender.input_sender(), |out| match out {
                    LibraryViewOutput::PlayTrack(id) => AppInput::PlayTrack(id),
                    LibraryViewOutput::OpenAlbum(id) => AppInput::Navigate(Route::AlbumDetail(id)),
                    LibraryViewOutput::OpenPlaylist(id) => {
                        AppInput::Navigate(Route::PlaylistDetail(id))
                    }
                });

        let album_view = AlbumView::builder()
            .launch((client.clone(), artwork_service.clone()))
            .forward(sender.input_sender(), |out| match out {
                AlbumViewOutput::PlayTrack(id) => AppInput::PlayTrack(id),
            });

        let artist_view = ArtistView::builder()
            .launch((client.clone(), artwork_service.clone()))
            .detach();

        let playlist_view = PlaylistView::builder()
            .launch((client.clone(), artwork_service.clone()))
            .forward(sender.input_sender(), |out| match out {
                PlaylistViewOutput::PlayTrack(id) => AppInput::PlayTrack(id),
            });

        let player_bar = PlayerBar::builder()
            .launch((client.clone(), artwork_service.clone()))
            .detach();

        let model = Self {
            client: client.clone(),
            _artwork_service: artwork_service,
            history: NavigationHistory::default(),
            search_session: SearchSession::new(default_provider.clone()),
            browse_provider: default_provider,
            provider_cache: ProviderCache::default(),
            connection_status: ConnectionStatus::Connecting,
            provider_selector,
            search_view,
            library_view,
            album_view,
            artist_view,
            playlist_view,
            player_bar,
        };

        // Long-lived event subscription command
        let client_events = client.clone();
        sender.command(move |out, shutdown| {
            shutdown
                .register(async move {
                    let (mut event_rx, mut status_rx) = client_events.subscribe_events();
                    loop {
                        tokio::select! {
                            res = status_rx.changed() => {
                                if res.is_ok() {
                                    let status = *status_rx.borrow();
                                    let _ = out.send(AppCmd::ConnectionStatusChanged(status));
                                } else {
                                    break;
                                }
                            }
                            event = event_rx.recv() => {
                                match event {
                                    Ok(ev) => {
                                        let _ = out.send(AppCmd::DaemonEvent(ev));
                                    }
                                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                        tracing::warn!("Lagged by {n} daemon events");
                                    }
                                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                })
                .drop_on_shutdown()
        });

        // Initial fetch of providers and player status
        let client_init = client.clone();
        sender.oneshot_command(async move {
            let providers = client_init.list_providers().await.unwrap_or_default();
            let status = client_init.get_status().await.ok();
            AppCmd::InitialData { providers, status }
        });

        let provider_widget = model.provider_selector.widget();
        let player_widget = model.player_bar.widget();

        let widgets = view_output!();

        widgets
            .content_stack
            .add_named(model.search_view.widget(), Some("search"));
        widgets
            .content_stack
            .add_named(model.library_view.widget(), Some("library"));
        widgets
            .content_stack
            .add_named(model.album_view.widget(), Some("album"));
        widgets
            .content_stack
            .add_named(model.artist_view.widget(), Some("artist"));
        widgets
            .content_stack
            .add_named(model.playlist_view.widget(), Some("playlist"));
        widgets
            .content_stack
            .set_visible_child_name(model.active_route_name());

        widgets.search_entry.set_key_capture_widget(Some(&root));

        // Keyboard shortcuts
        let s_entry = widgets.search_entry.clone();
        let event_ctrl = gtk::EventControllerKey::new();
        let s_key = sender.clone();
        event_ctrl.connect_key_pressed(move |_ctrl, keyval, _code, modifier| {
            if modifier.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
                if keyval == gtk::gdk::Key::k || keyval == gtk::gdk::Key::l {
                    s_entry.grab_focus();
                    return gtk::glib::Propagation::Stop;
                }
            } else if modifier.contains(gtk::gdk::ModifierType::ALT_MASK) {
                if keyval == gtk::gdk::Key::Left {
                    s_key.input(AppInput::GoBack);
                    return gtk::glib::Propagation::Stop;
                } else if keyval == gtk::gdk::Key::Right {
                    s_key.input(AppInput::GoForward);
                    return gtk::glib::Propagation::Stop;
                }
            }
            gtk::glib::Propagation::Proceed
        });
        root.add_controller(event_ctrl);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            AppInput::Navigate(route) => {
                self.history.navigate_to(route.clone());
                self.sync_active_route(&route);
            }
            AppInput::GoBack => {
                if let Some(route) = self.history.go_back().cloned() {
                    self.sync_active_route(&route);
                }
            }
            AppInput::GoForward => {
                if let Some(route) = self.history.go_forward().cloned() {
                    self.sync_active_route(&route);
                }
            }
            AppInput::SearchEdited(query) => {
                let search_gen = self.search_session.set_query(query.clone());
                if *self.history.current() != Route::Search {
                    self.history.navigate_to(Route::Search);
                    self.sync_active_route(&Route::Search);
                }
                self.search_view.emit(SearchViewInput::SearchRequest {
                    query,
                    provider: self.browse_provider.clone(),
                    generation: search_gen,
                });
            }
            AppInput::BrowseProviderSelected(id) => {
                self.browse_provider = id.clone();
                let search_gen = self.search_session.set_provider(id.clone());

                // Update LibraryView with new provider capabilities
                self.sync_library_provider();

                // If currently on search and query exists, re-trigger search
                if *self.history.current() == Route::Search && !self.search_session.query.is_empty()
                {
                    self.search_view.emit(SearchViewInput::SearchRequest {
                        query: self.search_session.query.clone(),
                        provider: self.browse_provider.clone(),
                        generation: search_gen,
                    });
                }
            }
            AppInput::BeginAuth(id) => {
                let client = self.client.clone();
                sender.oneshot_command(async move {
                    let res = client.auth_begin(&id).await;
                    AppCmd::AuthResult(res.map(|_| ()))
                });
            }
            AppInput::PlayTrack(id) => {
                let client = self.client.clone();
                sender.oneshot_command(async move {
                    let res = client.play_track(&id).await;
                    AppCmd::PlayResult(res)
                });
            }
            AppInput::FocusSearch => {
                // Focused via search entry
            }
            AppInput::TogglePlayback => {
                self.player_bar.emit(PlayerBarInput::PlayClicked);
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
            AppCmd::ConnectionStatusChanged(status) => {
                self.connection_status = status;
                if status == ConnectionStatus::Connected {
                    // Re-query authoritative state upon reconnection
                    let client = self.client.clone();
                    sender.oneshot_command(async move {
                        let providers = client.list_providers().await.unwrap_or_default();
                        let status = client.get_status().await.ok();
                        AppCmd::InitialData { providers, status }
                    });
                }
            }
            AppCmd::DaemonEvent(ev) => match ev {
                ClientEvent::StatusChanged(status) => {
                    self.player_bar.emit(PlayerBarInput::StatusChanged(status));
                }
                ClientEvent::ProviderStateChanged { .. } | ClientEvent::AuthChanged(_) => {
                    let client = self.client.clone();
                    sender.oneshot_command(async move {
                        let providers = client.list_providers().await.unwrap_or_default();
                        let status = client.get_status().await.ok();
                        AppCmd::InitialData { providers, status }
                    });
                }
                _ => {}
            },
            AppCmd::InitialData { providers, status } => {
                self.provider_cache.set_providers(providers.clone());

                if !providers.iter().any(|p| p.id == self.browse_provider)
                    && let Some(first) = providers.first()
                {
                    self.browse_provider = first.id.clone();
                    self.search_session.set_provider(first.id.clone());
                }

                self.provider_selector
                    .emit(ProviderSelectorInput::SetProviders(providers));
                self.provider_selector
                    .emit(ProviderSelectorInput::SetSelected(
                        self.browse_provider.clone(),
                    ));
                self.player_bar.emit(PlayerBarInput::UpdateProviderCache(
                    self.provider_cache.clone(),
                ));

                if let Some(s) = status {
                    self.player_bar.emit(PlayerBarInput::StatusChanged(s));
                }

                self.sync_library_provider();
            }
            AppCmd::AuthResult(res) => {
                self.provider_selector
                    .emit(ProviderSelectorInput::AuthFinished(res.is_ok()));
            }
            AppCmd::PlayResult(res) => {
                if let Err(e) = res {
                    tracing::error!("Play command failed: {e}");
                }
            }
        }
    }
}

impl MalusApp {
    fn active_route_name(&self) -> &'static str {
        match self.history.current() {
            Route::Search => "search",
            Route::Library(_) => "library",
            Route::AlbumDetail(_) => "album",
            Route::ArtistDetail(_) => "artist",
            Route::PlaylistDetail(_) => "playlist",
        }
    }

    fn sync_active_route(&self, route: &Route) {
        match route {
            Route::Search => {
                // Search session view is active
            }
            Route::Library(tab) => {
                self.library_view.emit(LibraryViewInput::LoadInitial(*tab));
            }
            Route::AlbumDetail(id) => {
                self.album_view.emit(AlbumViewInput::LoadAlbum {
                    id: id.clone(),
                    generation: 1,
                });
            }
            Route::ArtistDetail(id) => {
                self.artist_view.emit(ArtistViewInput::LoadArtist {
                    id: id.clone(),
                    generation: 1,
                });
            }
            Route::PlaylistDetail(id) => {
                self.playlist_view.emit(PlaylistViewInput::LoadPlaylist {
                    id: id.clone(),
                    generation: 1,
                });
            }
        }
    }

    fn sync_library_provider(&self) {
        self.library_view.emit(LibraryViewInput::SetProvider {
            provider: self.browse_provider.clone(),
            can_tracks: self.has_browse_cap("library.tracks"),
            can_albums: self.has_browse_cap("library.albums"),
            can_playlists: self.has_browse_cap("library.playlists"),
        });
    }

    fn has_browse_cap(&self, cap: &str) -> bool {
        self.provider_cache
            .has_capability(&self.browse_provider, cap)
    }

    fn has_any_library_cap(&self) -> bool {
        self.has_browse_cap("library.tracks")
            || self.has_browse_cap("library.albums")
            || self.has_browse_cap("library.playlists")
    }

    fn banner_text(&self) -> &str {
        match self.connection_status {
            ConnectionStatus::Connecting => "Reconnecting to Malus daemon...",
            ConnectionStatus::Disconnected => "Disconnected from Malus daemon",
            ConnectionStatus::Connected => "",
        }
    }
}
