//! Borderless artwork-first album / playlist content tile using Relm4 FactoryComponent.

use relm4::factory::{DynamicIndex, FactoryComponent, FactorySender};
use relm4::gtk::{self, prelude::*};
use relm4::prelude::*;

use crate::components::artwork_widget::{ArtworkInit, ArtworkWidget};
use crate::services::ArtworkService;

#[derive(Debug, Clone)]
pub struct AlbumTileInit {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub artwork_url: Option<String>,
    pub service: ArtworkService,
}

pub struct AlbumTile {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    artwork: Controller<ArtworkWidget>,
}

#[derive(Debug)]
pub enum AlbumTileInput {
    Clicked,
}

#[derive(Debug)]
pub enum AlbumTileOutput {
    Selected(String),
}

#[relm4::factory(pub)]
impl FactoryComponent for AlbumTile {
    type Init = AlbumTileInit;
    type Input = AlbumTileInput;
    type Output = AlbumTileOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::FlowBox;

    view! {
        root = gtk::FlowBoxChild {
            gtk::Button {
                add_css_class: "album-tile",
                set_has_frame: false,
                set_cursor_from_name: Some("pointer"),
                set_width_request: 180,
                connect_clicked => AlbumTileInput::Clicked,

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 0,

                    #[local_ref]
                    artwork_widget -> gtk::Picture {},

                    gtk::Label {
                        set_xalign: 0.0,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        add_css_class: "album-title",
                        #[watch]
                        set_text: &self.title,
                    },

                    gtk::Label {
                        set_xalign: 0.0,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        add_css_class: "album-artist",
                        #[watch]
                        set_text: &self.subtitle,
                    },
                }
            }
        }
    }

    fn init_model(init: Self::Init, _index: &DynamicIndex, _sender: FactorySender<Self>) -> Self {
        let artwork = ArtworkWidget::builder()
            .launch(ArtworkInit {
                url: init.artwork_url,
                size: (180, 180),
                css_class: "album-artwork".to_string(),
                service: init.service,
            })
            .detach();

        Self {
            id: init.id,
            title: init.title,
            subtitle: init.subtitle,
            artwork,
        }
    }

    fn init_widgets(
        &mut self,
        _index: &DynamicIndex,
        root: Self::Root,
        _returned_widget: &<Self::ParentWidget as relm4::factory::FactoryView>::ReturnedWidget,
        sender: FactorySender<Self>,
    ) -> Self::Widgets {
        let artwork_widget = self.artwork.widget();
        let widgets = view_output!();
        widgets
    }

    fn update(&mut self, message: Self::Input, sender: FactorySender<Self>) {
        match message {
            AlbumTileInput::Clicked => {
                let _ = sender.output(AlbumTileOutput::Selected(self.id.clone()));
            }
        }
    }
}
