//! AlbumDetailView component.
//!
//! Loads and displays album metadata and track list.
//! Individual tracks can play. No "Play Album" button until backend supports it.

use malus_client::{AlbumWire, CatalogItemWire, ClientError, MalusClient, PageWire, TrackWire};
use relm4::factory::FactoryVecDeque;
use relm4::gtk::{self, prelude::*};
use relm4::prelude::*;

use crate::artwork::ArtworkService;
use crate::components::artwork_widget::{ArtworkInit, ArtworkInput, ArtworkWidget};
use crate::factories::{TrackRow, TrackRowInit, TrackRowOutput};

pub struct AlbumView {
    client: MalusClient,
    album_id: String,
    current_generation: u64,
    is_loading: bool,
    error_msg: Option<String>,
    album: Option<AlbumWire>,
    next_cursor: Option<String>,
    is_loading_more: bool,

    artwork_comp: Controller<ArtworkWidget>,
    tracks: FactoryVecDeque<TrackRow>,
}

#[derive(Debug)]
pub enum AlbumViewInput {
    LoadAlbum { id: String, generation: u64 },
    LoadMoreTracks,
    TrackOutput(TrackRowOutput),
}

#[derive(Debug)]
pub enum AlbumViewOutput {
    PlayTrack(String),
}

#[derive(Debug)]
pub enum AlbumCmd {
    AlbumLoaded {
        generation: u64,
        result: Result<(AlbumWire, Option<PageWire<TrackWire>>), ClientError>,
    },
    MoreTracksLoaded {
        generation: u64,
        result: Result<PageWire<TrackWire>, ClientError>,
    },
}

#[relm4::component(pub)]
impl Component for AlbumView {
    type Init = (MalusClient, ArtworkService);
    type Input = AlbumViewInput;
    type Output = AlbumViewOutput;
    type CommandOutput = AlbumCmd;

    view! {
        gtk::ScrolledWindow {
            set_vexpand: true,
            set_hexpand: true,

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 16,
                add_css_class: "content-area",

                // Header metadata box
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 20,
                    set_valign: gtk::Align::Start,

                    // Artwork
                    #[local_ref]
                    art_widget -> gtk::Picture {},

                    // Metadata
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 4,
                        set_valign: gtk::Align::Center,

                        gtk::Label {
                            set_text: "ALBUM",
                            set_xalign: 0.0,
                            add_css_class: "section-header",
                        },

                        #[name(title_lbl)]
                        gtk::Label {
                            set_xalign: 0.0,
                            add_css_class: "view-title",
                            #[watch]
                            set_text: model.album_title(),
                        },

                        #[name(artist_lbl)]
                        gtk::Label {
                            set_xalign: 0.0,
                            add_css_class: "card-title",
                            #[watch]
                            set_text: &model.album_artist(),
                        },

                        #[name(meta_lbl)]
                        gtk::Label {
                            set_xalign: 0.0,
                            add_css_class: "card-subtitle",
                            #[watch]
                            set_text: &model.album_meta(),
                        },
                    },
                },

                // Status message (if loading or error)
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
                    connect_clicked => AlbumViewInput::LoadMoreTracks,
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
            .forward(sender.input_sender(), AlbumViewInput::TrackOutput);

        let model = Self {
            client,
            album_id: String::new(),
            current_generation: 0,
            is_loading: false,
            error_msg: None,
            album: None,
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
            AlbumViewInput::LoadAlbum { id, generation } => {
                self.album_id = id.clone();
                self.current_generation = generation;
                self.is_loading = true;
                self.error_msg = None;
                self.album = None;
                self.next_cursor = None;
                self.tracks.guard().clear();

                let client = self.client.clone();
                let req_gen = generation;
                let album_id = id;

                sender.oneshot_command(async move {
                    let item_res = client.get_catalog_item(&album_id).await;
                    match item_res {
                        Ok(CatalogItemWire::Album(album)) => {
                            let tracks_res = client
                                .get_collection_items(&album_id, Some(50), None)
                                .await
                                .ok();
                            AlbumCmd::AlbumLoaded {
                                generation: req_gen,
                                result: Ok((album, tracks_res)),
                            }
                        }
                        Ok(_) => AlbumCmd::AlbumLoaded {
                            generation: req_gen,
                            result: Err(ClientError::ServerError {
                                code: "invalid_type".into(),
                                message: "Expected album catalog item".into(),
                            }),
                        },
                        Err(e) => AlbumCmd::AlbumLoaded {
                            generation: req_gen,
                            result: Err(e),
                        },
                    }
                });
            }
            AlbumViewInput::LoadMoreTracks => {
                if let Some(cursor) = self.next_cursor.clone() {
                    self.is_loading_more = true;
                    let client = self.client.clone();
                    let album_id = self.album_id.clone();
                    let req_gen = self.current_generation;

                    sender.oneshot_command(async move {
                        let result = client
                            .get_collection_items(&album_id, Some(50), Some(cursor))
                            .await;
                        AlbumCmd::MoreTracksLoaded {
                            generation: req_gen,
                            result,
                        }
                    });
                }
            }
            AlbumViewInput::TrackOutput(TrackRowOutput::Play(id)) => {
                let _ = sender.output(AlbumViewOutput::PlayTrack(id));
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
            AlbumCmd::AlbumLoaded { generation, result } => {
                if self.current_generation != generation {
                    return;
                }
                self.is_loading = false;
                match result {
                    Ok((album, tracks_page)) => {
                        let art_url = album.artwork.as_ref().map(|a| a.url.clone());
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
                        self.album = Some(album);
                    }
                    Err(e) => {
                        self.error_msg = Some(e.to_string());
                    }
                }
            }
            AlbumCmd::MoreTracksLoaded { generation, result } => {
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

impl AlbumView {
    fn album_title(&self) -> &str {
        self.album.as_ref().map(|a| a.title.as_str()).unwrap_or("")
    }

    fn album_artist(&self) -> String {
        self.album
            .as_ref()
            .map(|a| {
                a.artists
                    .iter()
                    .map(|ar| ar.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default()
    }

    fn album_meta(&self) -> String {
        if let Some(ref a) = self.album {
            let mut parts = Vec::new();
            if let Some(ref d) = a.release_date {
                parts.push(d.clone());
            }
            if let Some(c) = a.track_count {
                parts.push(format!("{c} tracks"));
            }
            parts.join(" • ")
        } else {
            String::new()
        }
    }

    fn status_text(&self) -> &str {
        if self.is_loading {
            "Loading album..."
        } else if let Some(ref err) = self.error_msg {
            err.as_str()
        } else {
            ""
        }
    }
}
