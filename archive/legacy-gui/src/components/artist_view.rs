//! ArtistDetailView component.
//!
//! Honest to current backend: shows artist name and artwork. No fabricated tracks.

use malus_client::{ArtistWire, CatalogItemWire, ClientError, MalusClient};
use relm4::gtk::{self, prelude::*};
use relm4::prelude::*;

use crate::artwork::ArtworkService;
use crate::components::artwork_widget::{ArtworkInit, ArtworkInput, ArtworkWidget};

pub struct ArtistView {
    client: MalusClient,
    artist_id: String,
    current_generation: u64,
    is_loading: bool,
    error_msg: Option<String>,
    artist: Option<ArtistWire>,

    artwork_comp: Controller<ArtworkWidget>,
}

#[derive(Debug)]
pub enum ArtistViewInput {
    LoadArtist { id: String, generation: u64 },
}

#[derive(Debug)]
pub enum ArtistCmd {
    ArtistLoaded {
        generation: u64,
        result: Result<ArtistWire, ClientError>,
    },
}

#[relm4::component(pub)]
impl Component for ArtistView {
    type Init = (MalusClient, ArtworkService);
    type Input = ArtistViewInput;
    type Output = ();
    type CommandOutput = ArtistCmd;

    view! {
        gtk::ScrolledWindow {
            set_vexpand: true,
            set_hexpand: true,

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 16,
                add_css_class: "content-area",

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
                            set_text: "ARTIST",
                            set_xalign: 0.0,
                            add_css_class: "section-header",
                        },

                        #[name(name_lbl)]
                        gtk::Label {
                            set_xalign: 0.0,
                            add_css_class: "view-title",
                            #[watch]
                            set_text: model.artist_name(),
                        },
                    },
                },

                gtk::Label {
                    set_xalign: 0.0,
                    add_css_class: "status-muted",
                    #[watch]
                    set_text: model.status_text(),
                    #[watch]
                    set_visible: model.is_loading || model.error_msg.is_some(),
                },
            }
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let (client, artwork_service) = init;

        let artwork_comp = ArtworkWidget::builder()
            .launch(ArtworkInit {
                url: None,
                size: (120, 120),
                css_class: "artist-artwork".into(),
                service: artwork_service,
            })
            .detach();

        let model = Self {
            client,
            artist_id: String::new(),
            current_generation: 0,
            is_loading: false,
            error_msg: None,
            artist: None,
            artwork_comp,
        };

        let art_widget = model.artwork_comp.widget();
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            ArtistViewInput::LoadArtist { id, generation } => {
                self.artist_id = id.clone();
                self.current_generation = generation;
                self.is_loading = true;
                self.error_msg = None;
                self.artist = None;

                let client = self.client.clone();
                let req_gen = generation;
                let art_id = id;

                sender.oneshot_command(async move {
                    let item_res = client.get_catalog_item(&art_id).await;
                    match item_res {
                        Ok(CatalogItemWire::Artist(art)) => ArtistCmd::ArtistLoaded {
                            generation: req_gen,
                            result: Ok(art),
                        },
                        Ok(_) => ArtistCmd::ArtistLoaded {
                            generation: req_gen,
                            result: Err(ClientError::ServerError {
                                code: "invalid_type".into(),
                                message: "Expected artist catalog item".into(),
                            }),
                        },
                        Err(e) => ArtistCmd::ArtistLoaded {
                            generation: req_gen,
                            result: Err(e),
                        },
                    }
                });
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
            ArtistCmd::ArtistLoaded { generation, result } => {
                if self.current_generation != generation {
                    return;
                }
                self.is_loading = false;
                match result {
                    Ok(artist) => {
                        let art_url = artist.artwork.as_ref().map(|a| a.url.clone());
                        self.artwork_comp.emit(ArtworkInput::SetUrl(art_url));
                        self.artist = Some(artist);
                    }
                    Err(e) => {
                        self.error_msg = Some(e.to_string());
                    }
                }
            }
        }
    }
}

impl ArtistView {
    fn artist_name(&self) -> &str {
        self.artist.as_ref().map(|a| a.name.as_str()).unwrap_or("")
    }

    fn status_text(&self) -> &str {
        if self.is_loading {
            "Loading artist..."
        } else if let Some(ref err) = self.error_msg {
            err.as_str()
        } else {
            ""
        }
    }
}
