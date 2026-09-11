//! SearchView component.
//!
//! Subordinate to MalusApp search session. Does NOT own an independent query.
//! Manages results, factories for tracks/albums/artists/playlists, and async search commands.

use malus_client::{ClientError, MalusClient, SearchResultsWire};
use relm4::factory::FactoryVecDeque;
use relm4::gtk::{self, prelude::*};
use relm4::prelude::*;

use crate::factories::{
    AlbumCard, AlbumCardInit, AlbumCardOutput, ArtistCard, ArtistCardInit, ArtistCardOutput,
    PlaylistCard, PlaylistCardInit, PlaylistCardOutput, TrackRow, TrackRowInit, TrackRowOutput,
};

pub struct SearchView {
    client: MalusClient,
    current_generation: u64,
    current_query: String,
    is_loading: bool,
    error_msg: Option<String>,
    total_items: usize,

    // Collections
    tracks: FactoryVecDeque<TrackRow>,
    albums: FactoryVecDeque<AlbumCard>,
    artists: FactoryVecDeque<ArtistCard>,
    playlists: FactoryVecDeque<PlaylistCard>,
}

#[derive(Debug)]
pub enum SearchViewInput {
    SearchRequest {
        query: String,
        provider: String,
        generation: u64,
    },
    TrackOutput(TrackRowOutput),
    AlbumOutput(AlbumCardOutput),
    ArtistOutput(ArtistCardOutput),
    PlaylistOutput(PlaylistCardOutput),
}

#[derive(Debug)]
pub enum SearchViewOutput {
    PlayTrack(String),
    OpenAlbum(String),
    OpenArtist(String),
    OpenPlaylist(String),
}

#[derive(Debug)]
pub enum SearchCmd {
    Finished {
        generation: u64,
        result: Result<SearchResultsWire, ClientError>,
    },
}

#[relm4::component(pub)]
impl Component for SearchView {
    type Init = MalusClient;
    type Input = SearchViewInput;
    type Output = SearchViewOutput;
    type CommandOutput = SearchCmd;

    view! {
        gtk::ScrolledWindow {
            set_vexpand: true,
            set_hexpand: true,

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 16,
                add_css_class: "content-area",

                // Status / Empty / Loading / Error label
                #[name(status_label)]
                gtk::Label {
                    set_xalign: 0.0,
                    add_css_class: "status-muted",
                    #[watch]
                    set_text: model.status_text(),
                    #[watch]
                    set_visible: model.status_text_visible(),
                },

                // 1. Tracks Section
                #[name(tracks_box)]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 6,
                    #[watch]
                    set_visible: !model.tracks.is_empty(),

                    gtk::Label {
                        set_text: "Songs",
                        set_xalign: 0.0,
                        add_css_class: "section-header",
                    },

                    #[local_ref]
                    tracks_list -> gtk::ListBox {
                        set_selection_mode: gtk::SelectionMode::None,
                    },
                },

                // 2. Albums Section
                #[name(albums_box)]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 8,
                    #[watch]
                    set_visible: !model.albums.is_empty(),

                    gtk::Label {
                        set_text: "Albums",
                        set_xalign: 0.0,
                        add_css_class: "section-header",
                    },

                    #[local_ref]
                    albums_flow -> gtk::FlowBox {
                        set_selection_mode: gtk::SelectionMode::None,
                        set_max_children_per_line: 8,
                    },
                },

                // 3. Artists Section
                #[name(artists_box)]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 8,
                    #[watch]
                    set_visible: !model.artists.is_empty(),

                    gtk::Label {
                        set_text: "Artists",
                        set_xalign: 0.0,
                        add_css_class: "section-header",
                    },

                    #[local_ref]
                    artists_flow -> gtk::FlowBox {
                        set_selection_mode: gtk::SelectionMode::None,
                        set_max_children_per_line: 8,
                    },
                },

                // 4. Playlists Section
                #[name(playlists_box)]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 8,
                    #[watch]
                    set_visible: !model.playlists.is_empty(),

                    gtk::Label {
                        set_text: "Playlists",
                        set_xalign: 0.0,
                        add_css_class: "section-header",
                    },

                    #[local_ref]
                    playlists_flow -> gtk::FlowBox {
                        set_selection_mode: gtk::SelectionMode::None,
                        set_max_children_per_line: 8,
                    },
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
            .forward(sender.input_sender(), SearchViewInput::TrackOutput);

        let albums = FactoryVecDeque::builder()
            .launch(gtk::FlowBox::default())
            .forward(sender.input_sender(), SearchViewInput::AlbumOutput);

        let artists = FactoryVecDeque::builder()
            .launch(gtk::FlowBox::default())
            .forward(sender.input_sender(), SearchViewInput::ArtistOutput);

        let playlists = FactoryVecDeque::builder()
            .launch(gtk::FlowBox::default())
            .forward(sender.input_sender(), SearchViewInput::PlaylistOutput);

        let model = Self {
            client: init,
            current_generation: 0,
            current_query: String::new(),
            is_loading: false,
            error_msg: None,
            total_items: 0,
            tracks,
            albums,
            artists,
            playlists,
        };

        let tracks_list = model.tracks.widget();
        let albums_flow = model.albums.widget();
        let artists_flow = model.artists.widget();
        let playlists_flow = model.playlists.widget();

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            SearchViewInput::SearchRequest {
                query,
                provider,
                generation,
            } => {
                self.current_generation = generation;
                self.current_query = query.clone();
                self.error_msg = None;

                if query.trim().is_empty() {
                    self.is_loading = false;
                    self.clear_all();
                    return;
                }

                self.is_loading = true;
                let client = self.client.clone();
                let q = query.clone();
                let prov = provider.clone();
                let search_gen = generation;

                sender.oneshot_command(async move {
                    let result = client
                        .search(&q, Vec::new(), Some(prov), Some(15), None)
                        .await;
                    SearchCmd::Finished {
                        generation: search_gen,
                        result,
                    }
                });
            }
            SearchViewInput::TrackOutput(TrackRowOutput::Play(id)) => {
                let _ = sender.output(SearchViewOutput::PlayTrack(id));
            }
            SearchViewInput::AlbumOutput(AlbumCardOutput::Open(id)) => {
                let _ = sender.output(SearchViewOutput::OpenAlbum(id));
            }
            SearchViewInput::ArtistOutput(ArtistCardOutput::Open(id)) => {
                let _ = sender.output(SearchViewOutput::OpenArtist(id));
            }
            SearchViewInput::PlaylistOutput(PlaylistCardOutput::Open(id)) => {
                let _ = sender.output(SearchViewOutput::OpenPlaylist(id));
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
            SearchCmd::Finished { generation, result } => {
                // Drop stale results from previous search queries/providers
                if self.current_generation != generation {
                    return;
                }

                self.is_loading = false;
                self.clear_all();

                match result {
                    Ok(results) => {
                        let mut count = 0;

                        if let Some(t_page) = results.tracks {
                            let mut guard = self.tracks.guard();
                            for (idx, track) in t_page.items.into_iter().enumerate() {
                                count += 1;
                                guard.push_back(TrackRowInit {
                                    display_index: idx + 1,
                                    track,
                                });
                            }
                        }

                        if let Some(a_page) = results.albums {
                            let mut guard = self.albums.guard();
                            for album in a_page.items {
                                count += 1;
                                guard.push_back(AlbumCardInit { album });
                            }
                        }

                        if let Some(ar_page) = results.artists {
                            let mut guard = self.artists.guard();
                            for artist in ar_page.items {
                                count += 1;
                                guard.push_back(ArtistCardInit { artist });
                            }
                        }

                        if let Some(p_page) = results.playlists {
                            let mut guard = self.playlists.guard();
                            for playlist in p_page.items {
                                count += 1;
                                guard.push_back(PlaylistCardInit { playlist });
                            }
                        }

                        self.total_items = count;
                    }
                    Err(err) => {
                        self.error_msg = Some(err.to_string());
                    }
                }
            }
        }
    }
}

impl SearchView {
    fn clear_all(&mut self) {
        self.tracks.guard().clear();
        self.albums.guard().clear();
        self.artists.guard().clear();
        self.playlists.guard().clear();
        self.total_items = 0;
    }

    fn status_text(&self) -> &str {
        if self.is_loading {
            "Searching..."
        } else if let Some(ref err) = self.error_msg {
            err.as_str()
        } else if self.current_query.trim().is_empty() {
            "Search music to find songs, albums, and artists"
        } else if self.total_items == 0 {
            "No results found"
        } else {
            ""
        }
    }

    fn status_text_visible(&self) -> bool {
        self.is_loading
            || self.error_msg.is_some()
            || self.current_query.trim().is_empty()
            || self.total_items == 0
    }
}
