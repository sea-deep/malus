//! Async artwork picture component.
//!
//! Loads decoded RGBA image buffers from `ArtworkService` and creates
//! `gdk::MemoryTexture` on the GTK main thread.

use relm4::gtk::{self, prelude::*};
use relm4::prelude::*;
use std::sync::Arc;

use crate::services::{ArtworkService, DecodedImage};

pub struct ArtworkWidget {
    url: Option<String>,
    generation: u64,
    size: (i32, i32),
    service: ArtworkService,
    texture: Option<gtk::gdk::MemoryTexture>,
}

#[derive(Debug)]
pub enum ArtworkInput {
    SetUrl(Option<String>),
    SetSize(i32, i32),
}

#[derive(Debug)]
pub enum ArtworkCmd {
    Loaded {
        generation: u64,
        image: Option<Arc<DecodedImage>>,
    },
}

#[derive(Debug, Clone)]
pub struct ArtworkInit {
    pub url: Option<String>,
    pub size: (i32, i32),
    pub css_class: String,
    pub service: ArtworkService,
}

#[relm4::component(pub)]
impl Component for ArtworkWidget {
    type Init = ArtworkInit;
    type Input = ArtworkInput;
    type Output = ();
    type CommandOutput = ArtworkCmd;

    view! {
        #[root]
        gtk::Picture {
            set_can_shrink: true,
            set_content_fit: gtk::ContentFit::Cover,
            #[watch]
            set_size_request: (model.size.0, model.size.1),
            #[watch]
            set_paintable: model.texture.as_ref(),
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.add_css_class(&init.css_class);
        let model = Self {
            url: init.url.clone(),
            generation: 1,
            size: init.size,
            service: init.service,
            texture: None,
        };

        if let Some(url) = init.url {
            let service = model.service.clone();
            let req_gen = model.generation;
            sender.oneshot_command(async move {
                let img = service.load(&url).await;
                ArtworkCmd::Loaded {
                    generation: req_gen,
                    image: img,
                }
            });
        }

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            ArtworkInput::SetUrl(new_url) => {
                if self.url != new_url {
                    self.url = new_url.clone();
                    self.generation = self.generation.wrapping_add(1);
                    self.texture = None;

                    if let Some(url) = new_url {
                        let service = self.service.clone();
                        let req_gen = self.generation;
                        sender.oneshot_command(async move {
                            let img = service.load(&url).await;
                            ArtworkCmd::Loaded {
                                generation: req_gen,
                                image: img,
                            }
                        });
                    }
                }
            }
            ArtworkInput::SetSize(w, h) => {
                self.size = (w, h);
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
            ArtworkCmd::Loaded { generation, image } => {
                if self.generation == generation {
                    if let Some(img) = image {
                        let bytes = gtk::glib::Bytes::from(&img.rgba);
                        let texture = gtk::gdk::MemoryTexture::new(
                            img.width as i32,
                            img.height as i32,
                            gtk::gdk::MemoryFormat::R8g8b8a8,
                            &bytes,
                            img.stride,
                        );
                        self.texture = Some(texture);
                    } else {
                        self.texture = None;
                    }
                }
            }
        }
    }
}
