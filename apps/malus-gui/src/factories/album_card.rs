//! Album grid card factory component.

use malus_client::AlbumWire;
use relm4::factory::FactoryComponent;
use relm4::gtk::{self, prelude::*};
use relm4::prelude::*;

#[derive(Debug)]
pub struct AlbumCardInit {
    pub album: AlbumWire,
}

pub struct AlbumCard {
    pub album: AlbumWire,
}

#[derive(Debug)]
pub enum AlbumCardInput {
    Clicked,
}

#[derive(Debug)]
pub enum AlbumCardOutput {
    Open(String),
}

#[relm4::factory(pub)]
impl FactoryComponent for AlbumCard {
    type Init = AlbumCardInit;
    type Input = AlbumCardInput;
    type Output = AlbumCardOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::FlowBox;

    view! {
        root = gtk::FlowBoxChild {
            gtk::Button {
                add_css_class: "grid-card",
                connect_clicked => AlbumCardInput::Clicked,

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 6,
                    set_size_request: (140, 160),

                    gtk::Image {
                        set_icon_name: Some("media-optical-cd-audio-symbolic"),
                        set_pixel_size: 72,
                        set_valign: gtk::Align::Center,
                        set_vexpand: true,
                        add_css_class: "grid-artwork",
                    },

                    gtk::Label {
                        set_text: &self.album.title,
                        set_xalign: 0.0,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        add_css_class: "card-title",
                    },

                    gtk::Label {
                        set_text: &self.album.artists.iter().map(|ar| ar.name.as_str()).collect::<Vec<_>>().join(", "),
                        set_xalign: 0.0,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        add_css_class: "card-subtitle",
                        set_visible: !self.album.artists.is_empty(),
                    },
                }
            }
        }
    }

    fn init_model(init: Self::Init, _index: &Self::Index, _sender: FactorySender<Self>) -> Self {
        Self { album: init.album }
    }

    fn update(&mut self, message: Self::Input, sender: FactorySender<Self>) {
        match message {
            AlbumCardInput::Clicked => {
                let _ = sender.output(AlbumCardOutput::Open(self.album.id.clone()));
            }
        }
    }
}
