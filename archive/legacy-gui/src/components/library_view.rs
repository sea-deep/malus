//! LibraryView component with independent tab states and pagination.

use malus_client::{
    AlbumWire, ClientError, LibraryKindWire, LibraryPageWire, MalusClient, PageWire, PlaylistWire,
    TrackWire,
};
use relm4::factory::FactoryVecDeque;
use relm4::gtk::{self, prelude::*};
use relm4::prelude::*;

use crate::factories::{
    AlbumCard, AlbumCardInit, AlbumCardOutput, PlaylistCard, PlaylistCardInit, PlaylistCardOutput,
    TrackRow, TrackRowInit, TrackRowOutput,
};
use crate::model::LibraryTab;

pub struct LibraryView {
    client: MalusClient,
    current_provider: String,
    active_tab: LibraryTab,
    generation: u64,

    // Capability flags for browse provider
    can_tracks: bool,
    can_albums: bool,
    can_playlists: bool,

    // Tracks tab state
    tracks: FactoryVecDeque<TrackRow>,
    tracks_cursor: Option<String>,
    tracks_loading: bool,
    tracks_error: Option<String>,

    // Albums tab state
    albums: FactoryVecDeque<AlbumCard>,
    albums_cursor: Option<String>,
    albums_loading: bool,
    albums_error: Option<String>,

    // Playlists tab state
    playlists: FactoryVecDeque<PlaylistCard>,
    playlists_cursor: Option<String>,
    playlists_loading: bool,
    playlists_error: Option<String>,
}

#[derive(Debug)]
pub enum LibraryViewInput {
    SwitchTab(LibraryTab),
    SetProvider {
        provider: String,
        can_tracks: bool,
        can_albums: bool,
        can_playlists: bool,
    },
    LoadInitial(LibraryTab),
    LoadMore(LibraryTab),
    TrackOutput(TrackRowOutput),
    AlbumOutput(AlbumCardOutput),
    PlaylistOutput(PlaylistCardOutput),
}

#[derive(Debug)]
pub enum LibraryViewOutput {
    PlayTrack(String),
    OpenAlbum(String),
    OpenPlaylist(String),
}

#[derive(Debug)]
pub enum LibraryCmd {
    TracksLoaded {
        generation: u64,
        result: Result<PageWire<TrackWire>, ClientError>,
        append: bool,
    },
    AlbumsLoaded {
        generation: u64,
        result: Result<PageWire<AlbumWire>, ClientError>,
        append: bool,
    },
    PlaylistsLoaded {
        generation: u64,
        result: Result<PageWire<PlaylistWire>, ClientError>,
        append: bool,
    },
}

#[relm4::component(pub)]
impl Component for LibraryView {
    type Init = MalusClient;
    type Input = LibraryViewInput;
    type Output = LibraryViewOutput;
    type CommandOutput = LibraryCmd;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 12,
            add_css_class: "content-area",

            // Tab bar switcher
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                add_css_class: "tab-switcher",

                #[name(tracks_tab_btn)]
                gtk::Button {
                    set_label: "Tracks",
                    add_css_class: "tab-btn",
                    #[watch]
                    set_visible: model.can_tracks,
                    #[watch]
                    set_css_classes: if model.active_tab == LibraryTab::Tracks {
                        &["tab-btn", "active"]
                    } else {
                        &["tab-btn"]
                    },
                    connect_clicked => LibraryViewInput::SwitchTab(LibraryTab::Tracks),
                },

                #[name(albums_tab_btn)]
                gtk::Button {
                    set_label: "Albums",
                    add_css_class: "tab-btn",
                    #[watch]
                    set_visible: model.can_albums,
                    #[watch]
                    set_css_classes: if model.active_tab == LibraryTab::Albums {
                        &["tab-btn", "active"]
                    } else {
                        &["tab-btn"]
                    },
                    connect_clicked => LibraryViewInput::SwitchTab(LibraryTab::Albums),
                },

                #[name(playlists_tab_btn)]
                gtk::Button {
                    set_label: "Playlists",
                    add_css_class: "tab-btn",
                    #[watch]
                    set_visible: model.can_playlists,
                    #[watch]
                    set_css_classes: if model.active_tab == LibraryTab::Playlists {
                        &["tab-btn", "active"]
                    } else {
                        &["tab-btn"]
                    },
                    connect_clicked => LibraryViewInput::SwitchTab(LibraryTab::Playlists),
                },
            },

            // Content stack for independent tabs
            #[name(stack)]
            gtk::Stack {
                set_vexpand: true,
                set_hexpand: true,
                #[watch]
                set_visible_child_name: model.active_tab_name(),

                // 1. Tracks tab
                #[name(tracks_page)]
                gtk::ScrolledWindow {
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 8,

                        gtk::Label {
                            set_xalign: 0.0,
                            add_css_class: "status-muted",
                            #[watch]
                            set_text: model.tracks_status(),
                            #[watch]
                            set_visible: model.tracks_status_visible(),
                        },

                        #[local_ref]
                        tracks_list -> gtk::ListBox {
                            set_selection_mode: gtk::SelectionMode::None,
                        },

                        gtk::Button {
                            set_label: "Load More",
                            set_halign: gtk::Align::Center,
                            #[watch]
                            set_visible: model.tracks_cursor.is_some(),
                            #[watch]
                            set_sensitive: !model.tracks_loading,
                            connect_clicked => LibraryViewInput::LoadMore(LibraryTab::Tracks),
                        },
                    }
                },

                // 2. Albums tab
                #[name(albums_page)]
                gtk::ScrolledWindow {
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 8,

                        gtk::Label {
                            set_xalign: 0.0,
                            add_css_class: "status-muted",
                            #[watch]
                            set_text: model.albums_status(),
                            #[watch]
                            set_visible: model.albums_status_visible(),
                        },

                        #[local_ref]
                        albums_flow -> gtk::FlowBox {
                            set_selection_mode: gtk::SelectionMode::None,
                            set_max_children_per_line: 8,
                        },

                        gtk::Button {
                            set_label: "Load More",
                            set_halign: gtk::Align::Center,
                            #[watch]
                            set_visible: model.albums_cursor.is_some(),
                            #[watch]
                            set_sensitive: !model.albums_loading,
                            connect_clicked => LibraryViewInput::LoadMore(LibraryTab::Albums),
                        },
                    }
                },

                // 3. Playlists tab
                #[name(playlists_page)]
                gtk::ScrolledWindow {
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 8,

                        gtk::Label {
                            set_xalign: 0.0,
                            add_css_class: "status-muted",
                            #[watch]
                            set_text: model.playlists_status(),
                            #[watch]
                            set_visible: model.playlists_status_visible(),
                        },

                        #[local_ref]
                        playlists_flow -> gtk::FlowBox {
                            set_selection_mode: gtk::SelectionMode::None,
                            set_max_children_per_line: 8,
                        },

                        gtk::Button {
                            set_label: "Load More",
                            set_halign: gtk::Align::Center,
                            #[watch]
                            set_visible: model.playlists_cursor.is_some(),
                            #[watch]
                            set_sensitive: !model.playlists_loading,
                            connect_clicked => LibraryViewInput::LoadMore(LibraryTab::Playlists),
                        },
                    }
                },
            }
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let tracks = FactoryVecDeque::builder()
            .launch(gtk::ListBox::default())
            .forward(sender.input_sender(), LibraryViewInput::TrackOutput);

        let albums = FactoryVecDeque::builder()
            .launch(gtk::FlowBox::default())
            .forward(sender.input_sender(), LibraryViewInput::AlbumOutput);

        let playlists = FactoryVecDeque::builder()
            .launch(gtk::FlowBox::default())
            .forward(sender.input_sender(), LibraryViewInput::PlaylistOutput);

        let model = Self {
            client: init,
            current_provider: String::new(),
            active_tab: LibraryTab::Tracks,
            generation: 1,

            can_tracks: true,
            can_albums: true,
            can_playlists: true,

            tracks,
            tracks_cursor: None,
            tracks_loading: false,
            tracks_error: None,

            albums,
            albums_cursor: None,
            albums_loading: false,
            albums_error: None,

            playlists,
            playlists_cursor: None,
            playlists_loading: false,
            playlists_error: None,
        };

        let tracks_list = model.tracks.widget();
        let albums_flow = model.albums.widget();
        let playlists_flow = model.playlists.widget();

        let widgets = view_output!();
        widgets.stack.page(&widgets.tracks_page).set_name("tracks");
        widgets.stack.page(&widgets.albums_page).set_name("albums");
        widgets
            .stack
            .page(&widgets.playlists_page)
            .set_name("playlists");
        widgets
            .stack
            .set_visible_child_name(model.active_tab_name());

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            LibraryViewInput::SwitchTab(tab) => {
                self.active_tab = tab;
                if self.is_tab_empty(tab) {
                    self.load_tab(tab, false, sender);
                }
            }
            LibraryViewInput::SetProvider {
                provider,
                can_tracks,
                can_albums,
                can_playlists,
            } => {
                if self.current_provider != provider {
                    self.current_provider = provider;
                    self.can_tracks = can_tracks;
                    self.can_albums = can_albums;
                    self.can_playlists = can_playlists;
                    self.generation = self.generation.wrapping_add(1);

                    // Invalidate old provider data
                    self.tracks.guard().clear();
                    self.tracks_cursor = None;
                    self.tracks_error = None;

                    self.albums.guard().clear();
                    self.albums_cursor = None;
                    self.albums_error = None;

                    self.playlists.guard().clear();
                    self.playlists_cursor = None;
                    self.playlists_error = None;

                    self.load_tab(self.active_tab, false, sender);
                }
            }
            LibraryViewInput::LoadInitial(tab) => {
                self.active_tab = tab;
                if self.is_tab_empty(tab) {
                    self.load_tab(tab, false, sender);
                }
            }
            LibraryViewInput::LoadMore(tab) => {
                self.load_tab(tab, true, sender);
            }
            LibraryViewInput::TrackOutput(TrackRowOutput::Play(id)) => {
                let _ = sender.output(LibraryViewOutput::PlayTrack(id));
            }
            LibraryViewInput::AlbumOutput(AlbumCardOutput::Open(id)) => {
                let _ = sender.output(LibraryViewOutput::OpenAlbum(id));
            }
            LibraryViewInput::PlaylistOutput(PlaylistCardOutput::Open(id)) => {
                let _ = sender.output(LibraryViewOutput::OpenPlaylist(id));
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
            LibraryCmd::TracksLoaded {
                generation,
                result,
                append,
            } => {
                if self.generation != generation {
                    return;
                }
                self.tracks_loading = false;
                match result {
                    Ok(page) => {
                        self.tracks_cursor = page.next_cursor;
                        let start_idx = if append { self.tracks.len() } else { 0 };
                        let mut guard = self.tracks.guard();
                        if !append {
                            guard.clear();
                        }
                        for (offset, track) in page.items.into_iter().enumerate() {
                            guard.push_back(TrackRowInit {
                                display_index: start_idx + offset + 1,
                                track,
                            });
                        }
                    }
                    Err(e) => {
                        self.tracks_error = Some(e.to_string());
                    }
                }
            }
            LibraryCmd::AlbumsLoaded {
                generation,
                result,
                append,
            } => {
                if self.generation != generation {
                    return;
                }
                self.albums_loading = false;
                match result {
                    Ok(page) => {
                        self.albums_cursor = page.next_cursor;
                        let mut guard = self.albums.guard();
                        if !append {
                            guard.clear();
                        }
                        for album in page.items {
                            guard.push_back(AlbumCardInit { album });
                        }
                    }
                    Err(e) => {
                        self.albums_error = Some(e.to_string());
                    }
                }
            }
            LibraryCmd::PlaylistsLoaded {
                generation,
                result,
                append,
            } => {
                if self.generation != generation {
                    return;
                }
                self.playlists_loading = false;
                match result {
                    Ok(page) => {
                        self.playlists_cursor = page.next_cursor;
                        let mut guard = self.playlists.guard();
                        if !append {
                            guard.clear();
                        }
                        for playlist in page.items {
                            guard.push_back(PlaylistCardInit { playlist });
                        }
                    }
                    Err(e) => {
                        self.playlists_error = Some(e.to_string());
                    }
                }
            }
        }
    }
}

impl LibraryView {
    fn is_tab_empty(&self, tab: LibraryTab) -> bool {
        match tab {
            LibraryTab::Tracks => self.tracks.is_empty(),
            LibraryTab::Albums => self.albums.is_empty(),
            LibraryTab::Playlists => self.playlists.is_empty(),
        }
    }

    fn load_tab(&mut self, tab: LibraryTab, append: bool, sender: ComponentSender<Self>) {
        let client = self.client.clone();
        let prov = self.current_provider.clone();
        let req_gen = self.generation;

        match tab {
            LibraryTab::Tracks => {
                if !self.can_tracks || self.tracks_loading {
                    return;
                }
                self.tracks_loading = true;
                self.tracks_error = None;
                let cursor = if append {
                    self.tracks_cursor.clone()
                } else {
                    None
                };

                sender.oneshot_command(async move {
                    let res = client
                        .get_library(LibraryKindWire::Tracks, Some(prov), Some(50), cursor)
                        .await;
                    let page_res = match res {
                        Ok(LibraryPageWire::Tracks(page)) => Ok(page),
                        Ok(_) => Err(ClientError::ServerError {
                            code: "unexpected_kind".into(),
                            message: "Expected tracks".into(),
                        }),
                        Err(e) => Err(e),
                    };
                    LibraryCmd::TracksLoaded {
                        generation: req_gen,
                        result: page_res,
                        append,
                    }
                });
            }
            LibraryTab::Albums => {
                if !self.can_albums || self.albums_loading {
                    return;
                }
                self.albums_loading = true;
                self.albums_error = None;
                let cursor = if append {
                    self.albums_cursor.clone()
                } else {
                    None
                };

                sender.oneshot_command(async move {
                    let res = client
                        .get_library(LibraryKindWire::Albums, Some(prov), Some(50), cursor)
                        .await;
                    let page_res = match res {
                        Ok(LibraryPageWire::Albums(page)) => Ok(page),
                        Ok(_) => Err(ClientError::ServerError {
                            code: "unexpected_kind".into(),
                            message: "Expected albums".into(),
                        }),
                        Err(e) => Err(e),
                    };
                    LibraryCmd::AlbumsLoaded {
                        generation: req_gen,
                        result: page_res,
                        append,
                    }
                });
            }
            LibraryTab::Playlists => {
                if !self.can_playlists || self.playlists_loading {
                    return;
                }
                self.playlists_loading = true;
                self.playlists_error = None;
                let cursor = if append {
                    self.playlists_cursor.clone()
                } else {
                    None
                };

                sender.oneshot_command(async move {
                    let res = client
                        .get_library(LibraryKindWire::Playlists, Some(prov), Some(50), cursor)
                        .await;
                    let page_res = match res {
                        Ok(LibraryPageWire::Playlists(page)) => Ok(page),
                        Ok(_) => Err(ClientError::ServerError {
                            code: "unexpected_kind".into(),
                            message: "Expected playlists".into(),
                        }),
                        Err(e) => Err(e),
                    };
                    LibraryCmd::PlaylistsLoaded {
                        generation: req_gen,
                        result: page_res,
                        append,
                    }
                });
            }
        }
    }

    fn tracks_status(&self) -> &str {
        if self.tracks_loading {
            "Loading tracks..."
        } else if let Some(ref err) = self.tracks_error {
            err.as_str()
        } else if self.tracks.is_empty() {
            "No tracks found in library"
        } else {
            ""
        }
    }

    fn tracks_status_visible(&self) -> bool {
        self.tracks_loading || self.tracks_error.is_some() || self.tracks.is_empty()
    }

    fn albums_status(&self) -> &str {
        if self.albums_loading {
            "Loading albums..."
        } else if let Some(ref err) = self.albums_error {
            err.as_str()
        } else if self.albums.is_empty() {
            "No albums found in library"
        } else {
            ""
        }
    }

    fn albums_status_visible(&self) -> bool {
        self.albums_loading || self.albums_error.is_some() || self.albums.is_empty()
    }

    fn playlists_status(&self) -> &str {
        if self.playlists_loading {
            "Loading playlists..."
        } else if let Some(ref err) = self.playlists_error {
            err.as_str()
        } else if self.playlists.is_empty() {
            "No playlists found in library"
        } else {
            ""
        }
    }

    fn playlists_status_visible(&self) -> bool {
        self.playlists_loading || self.playlists_error.is_some() || self.playlists.is_empty()
    }

    fn active_tab_name(&self) -> &'static str {
        match self.active_tab {
            LibraryTab::Tracks => "tracks",
            LibraryTab::Albums => "albums",
            LibraryTab::Playlists => "playlists",
        }
    }
}
