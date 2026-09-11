//! Playlist grid card factory component.

use malus_client::PlaylistWire;
use relm4::factory::FactoryComponent;
use relm4::gtk::{self, prelude::*};
use relm4::prelude::*;

#[derive(Debug)]
pub struct PlaylistCardInit {
    pub playlist: PlaylistWire,
}

pub struct PlaylistCard {
    pub playlist: PlaylistWire,
}

#[derive(Debug)]
pub enum PlaylistCardInput {
    Clicked,
}

#[derive(Debug)]
pub enum PlaylistCardOutput {
    Open(String),
}

#[relm4::factory(pub)]
impl FactoryComponent for PlaylistCard {
    type Init = PlaylistCardInit;
    type Input = PlaylistCardInput;
    type Output = PlaylistCardOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::FlowBox;

    view! {
        root = gtk::FlowBoxChild {
            gtk::Button {
                add_css_class: "grid-card",
                connect_clicked => PlaylistCardInput::Clicked,

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 6,
                    set_size_request: (140, 160),

                    gtk::Image {
                        set_icon_name: Some("audio-x-generic-symbolic"),
                        set_pixel_size: 72,
                        set_valign: gtk::Align::Center,
                        set_vexpand: true,
                        add_css_class: "grid-artwork",
                    },

                    gtk::Label {
                        set_text: &self.playlist.title,
                        set_xalign: 0.0,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        add_css_class: "card-title",
                    },

                    gtk::Label {
                        set_text: self.playlist.curator.as_deref().unwrap_or(""),
                        set_xalign: 0.0,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        add_css_class: "card-subtitle",
                        set_visible: self.playlist.curator.is_some(),
                    },
                }
            }
        }
    }

    fn init_model(init: Self::Init, _index: &Self::Index, _sender: FactorySender<Self>) -> Self {
        Self {
            playlist: init.playlist,
        }
    }

    fn update(&mut self, message: Self::Input, sender: FactorySender<Self>) {
        match message {
            PlaylistCardInput::Clicked => {
                let _ = sender.output(PlaylistCardOutput::Open(self.playlist.id.clone()));
            }
        }
    }
}
