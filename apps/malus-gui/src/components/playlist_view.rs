//! PlaylistDetailView component.

use malus_client::{CatalogItemWire, ClientError, MalusClient, PageWire, PlaylistWire, TrackWire};
use relm4::factory::FactoryVecDeque;
use relm4::gtk::{self, prelude::*};
use relm4::prelude::*;

use crate::artwork::ArtworkService;
use crate::components::artwork_widget::{ArtworkInit, ArtworkInput, ArtworkWidget};
use crate::factories::{TrackRow, TrackRowInit, TrackRowOutput};

pub struct PlaylistView {
    client: MalusClient,
    playlist_id: String,
    current_generation: u64,
    is_loading: bool,
    error_msg: Option<String>,
    playlist: Option<PlaylistWire>,
    next_cursor: Option<String>,
    is_loading_more: bool,

    artwork_comp: Controller<ArtworkWidget>,
    tracks: FactoryVecDeque<TrackRow>,
}

#[derive(Debug)]
pub enum PlaylistViewInput {
    LoadPlaylist { id: String, generation: u64 },
    LoadMoreTracks,
    TrackOutput(TrackRowOutput),
}

#[derive(Debug)]
pub enum PlaylistViewOutput {
    PlayTrack(String),
}

#[derive(Debug)]
pub enum PlaylistCmd {
    PlaylistLoaded {
        generation: u64,
        result: Result<(PlaylistWire, Option<PageWire<TrackWire>>), ClientError>,
    },
    MoreTracksLoaded {
        generation: u64,
        result: Result<PageWire<TrackWire>, ClientError>,
    },
}

#[relm4::component(pub)]
impl Component for PlaylistView {
    type Init = (MalusClient, ArtworkService);
    type Input = PlaylistViewInput;
    type Output = PlaylistViewOutput;
    type CommandOutput = PlaylistCmd;

    view! {
        gtk::ScrolledWindow {
            set_vexpand: true,
            set_hexpand: true,

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 16,
                add_css_class: "content-area",

                // Header
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 20,
                    set_valign: gtk::Align::Start,

                    #[local_ref]
                    art_widget -> gtk::Picture {},

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 4,
                        set_valign: gtk::Align::Center,

                        gtk::Label {
                            set_text: "PLAYLIST",
                            set_xalign: 0.0,
                            add_css_class: "section-header",
                        },

                        #[name(title_lbl)]
                        gtk::Label {
                            set_xalign: 0.0,
                            add_css_class: "view-title",
                            #[watch]
                            set_text: model.playlist_title(),
                        },

                        #[name(curator_lbl)]
                        gtk::Label {
                            set_xalign: 0.0,
                            add_css_class: "card-subtitle",
                            #[watch]
                            set_text: &model.playlist_curator(),
                        },

                        #[name(desc_lbl)]
                        gtk::Label {
                            set_xalign: 0.0,
                            add_css_class: "card-subtitle",
                            #[watch]
                            set_text: model.playlist_desc(),
                            #[watch]
                            set_visible: !model.playlist_desc().is_empty(),
                        },
                    },
                },

                // Status message
                gtk::Label {
                    set_xalign: 0.0,
                    add_css_class: "status-muted",
                    #[watch]
                    set_text: model.status_text(),
                    #[watch]
                    set_visible: model.is_loading || model.error_msg.is_some(),
                },

                // Tracklist
                #[local_ref]
                tracks_list -> gtk::ListBox {
                    set_selection_mode: gtk::SelectionMode::None,
                },

                // Load more button
                gtk::Button {
                    set_label: "Load More Tracks",
                    set_halign: gtk::Align::Center,
                    #[watch]
                    set_visible: model.next_cursor.is_some(),
                    #[watch]
                    set_sensitive: !model.is_loading_more,
                    connect_clicked => PlaylistViewInput::LoadMoreTracks,
                },
            }
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let (client, artwork_service) = init;

        let artwork_comp = ArtworkWidget::builder()
            .launch(ArtworkInit {
                url: None,
                size: (120, 120),
                css_class: "album-artwork".into(),
                service: artwork_service,
            })
            .detach();

        let tracks = FactoryVecDeque::builder()
            .launch(gtk::ListBox::default())
            .forward(sender.input_sender(), PlaylistViewInput::TrackOutput);

        let model = Self {
            client,
            playlist_id: String::new(),
            current_generation: 0,
            is_loading: false,
            error_msg: None,
            playlist: None,
            next_cursor: None,
            is_loading_more: false,
            artwork_comp,
            tracks,
        };

        let art_widget = model.artwork_comp.widget();
        let tracks_list = model.tracks.widget();

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            PlaylistViewInput::LoadPlaylist { id, generation } => {
                self.playlist_id = id.clone();
                self.current_generation = generation;
                self.is_loading = true;
                self.error_msg = None;
                self.playlist = None;
                self.next_cursor = None;
                self.tracks.guard().clear();

                let client = self.client.clone();
                let req_gen = generation;
                let pl_id = id;

                sender.oneshot_command(async move {
                    let item_res = client.get_catalog_item(&pl_id).await;
                    match item_res {
                        Ok(CatalogItemWire::Playlist(pl)) => {
                            let tracks_res = client
                                .get_collection_items(&pl_id, Some(50), None)
                                .await
                                .ok();
                            PlaylistCmd::PlaylistLoaded {
                                generation: req_gen,
                                result: Ok((pl, tracks_res)),
                            }
                        }
                        Ok(_) => PlaylistCmd::PlaylistLoaded {
                            generation: req_gen,
                            result: Err(ClientError::ServerError {
                                code: "invalid_type".into(),
                                message: "Expected playlist catalog item".into(),
                            }),
                        },
                        Err(e) => PlaylistCmd::PlaylistLoaded {
                            generation: req_gen,
                            result: Err(e),
                        },
                    }
                });
            }
            PlaylistViewInput::LoadMoreTracks => {
                if let Some(cursor) = self.next_cursor.clone() {
                    self.is_loading_more = true;
                    let client = self.client.clone();
                    let pl_id = self.playlist_id.clone();
                    let req_gen = self.current_generation;

                    sender.oneshot_command(async move {
                        let result = client
                            .get_collection_items(&pl_id, Some(50), Some(cursor))
                            .await;
                        PlaylistCmd::MoreTracksLoaded {
                            generation: req_gen,
                            result,
                        }
                    });
                }
            }
            PlaylistViewInput::TrackOutput(TrackRowOutput::Play(id)) => {
                let _ = sender.output(PlaylistViewOutput::PlayTrack(id));
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
            PlaylistCmd::PlaylistLoaded { generation, result } => {
                if self.current_generation != generation {
                    return;
                }
                self.is_loading = false;
                match result {
                    Ok((pl, tracks_page)) => {
                        let art_url = pl.artwork.as_ref().map(|a| a.url.clone());
                        self.artwork_comp.emit(ArtworkInput::SetUrl(art_url));

                        if let Some(page) = tracks_page {
                            self.next_cursor = page.next_cursor;
                            let mut guard = self.tracks.guard();
                            for (idx, track) in page.items.into_iter().enumerate() {
                                guard.push_back(TrackRowInit {
                                    display_index: idx + 1,
                                    track,
                                });
                            }
                        }
                        self.playlist = Some(pl);
                    }
                    Err(e) => {
                        self.error_msg = Some(e.to_string());
                    }
                }
            }
            PlaylistCmd::MoreTracksLoaded { generation, result } => {
                if self.current_generation != generation {
                    return;
                }
                self.is_loading_more = false;
                if let Ok(page) = result {
                    self.next_cursor = page.next_cursor;
                    let start_idx = self.tracks.len();
                    let mut guard = self.tracks.guard();
                    for (offset, track) in page.items.into_iter().enumerate() {
                        guard.push_back(TrackRowInit {
                            display_index: start_idx + offset + 1,
                            track,
                        });
                    }
                }
            }
        }
    }
}

impl PlaylistView {
    fn playlist_title(&self) -> &str {
        self.playlist
            .as_ref()
            .map(|p| p.title.as_str())
            .unwrap_or("")
    }

    fn playlist_curator(&self) -> String {
        self.playlist
            .as_ref()
            .and_then(|p| p.curator.as_ref())
            .map(|c| format!("Curated by {c}"))
            .unwrap_or_default()
    }

    fn playlist_desc(&self) -> &str {
        self.playlist
            .as_ref()
            .and_then(|p| p.description.as_deref())
            .unwrap_or("")
    }

    fn status_text(&self) -> &str {
        if self.is_loading {
            "Loading playlist..."
        } else if let Some(ref err) = self.error_msg {
            err.as_str()
        } else {
            ""
        }
    }
}
