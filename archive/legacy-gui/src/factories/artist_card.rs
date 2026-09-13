//! Artist grid card factory component.

use malus_client::ArtistWire;
use relm4::factory::FactoryComponent;
use relm4::gtk::{self, prelude::*};
use relm4::prelude::*;

#[derive(Debug)]
pub struct ArtistCardInit {
    pub artist: ArtistWire,
}

pub struct ArtistCard {
    pub artist: ArtistWire,
}

#[derive(Debug)]
pub enum ArtistCardInput {
    Clicked,
}

#[derive(Debug)]
pub enum ArtistCardOutput {
    Open(String),
}

#[relm4::factory(pub)]
impl FactoryComponent for ArtistCard {
    type Init = ArtistCardInit;
    type Input = ArtistCardInput;
    type Output = ArtistCardOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::FlowBox;

    view! {
        root = gtk::FlowBoxChild {
            gtk::Button {
                add_css_class: "grid-card",
                connect_clicked => ArtistCardInput::Clicked,

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 6,
                    set_size_request: (130, 150),

                    gtk::Image {
                        set_icon_name: Some("avatar-default-symbolic"),
                        set_pixel_size: 64,
                        set_valign: gtk::Align::Center,
                        set_vexpand: true,
                        add_css_class: "artist-artwork",
                    },

                    gtk::Label {
                        set_text: &self.artist.name,
                        set_xalign: 0.5,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        add_css_class: "card-title",
                    },
                }
            }
        }
    }

    fn init_model(init: Self::Init, _index: &Self::Index, _sender: FactorySender<Self>) -> Self {
        Self {
            artist: init.artist,
        }
    }

    fn update(&mut self, message: Self::Input, sender: FactorySender<Self>) {
        match message {
            ArtistCardInput::Clicked => {
                let _ = sender.output(ArtistCardOutput::Open(self.artist.id.clone()));
            }
        }
    }
}
