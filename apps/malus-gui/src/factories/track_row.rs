//! Canonical dense track row factory component.

use malus_client::TrackWire;
use relm4::factory::FactoryComponent;
use relm4::gtk::{self, prelude::*};
use relm4::prelude::*;

use crate::model::format_time;

#[derive(Debug)]
pub struct TrackRowInit {
    pub display_index: usize,
    pub track: TrackWire,
}

pub struct TrackRow {
    pub display_index: usize,
    pub track: TrackWire,
}

#[derive(Debug)]
pub enum TrackRowInput {
    Clicked,
}

#[derive(Debug)]
pub enum TrackRowOutput {
    Play(String),
}

#[relm4::factory(pub)]
impl FactoryComponent for TrackRow {
    type Init = TrackRowInit;
    type Input = TrackRowInput;
    type Output = TrackRowOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::ListBox;

    view! {
        root = gtk::ListBoxRow {
            add_css_class: "track-row",

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 12,
                set_hexpand: true,
                set_valign: gtk::Align::Center,

                // 1. Track Number
                gtk::Label {
                    set_text: &self.display_index.to_string(),
                    set_width_chars: 3,
                    set_xalign: 1.0,
                    add_css_class: "track-num",
                },

                // 2. Play Button
                gtk::Button {
                    set_icon_name: "media-playback-start-symbolic",
                    add_css_class: "track-play-btn",
                    connect_clicked => TrackRowInput::Clicked,
                },

                // 3. Title & Artist Box
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_hexpand: true,
                    set_valign: gtk::Align::Center,

                    gtk::Label {
                        set_text: &self.track.title,
                        set_xalign: 0.0,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        add_css_class: "track-title",
                    },

                    gtk::Label {
                        set_text: &self.track.artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", "),
                        set_xalign: 0.0,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        add_css_class: "track-artist",
                    },
                },

                // 4. Album (if available)
                gtk::Label {
                    set_text: self.track.album.as_ref().map(|a| a.title.as_str()).unwrap_or(""),
                    set_xalign: 0.0,
                    set_hexpand: true,
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                    add_css_class: "track-album",
                    set_visible: self.track.album.is_some(),
                },

                // 5. Duration
                gtk::Label {
                    set_text: &self.track.duration_ms.map(format_time).unwrap_or_default(),
                    set_xalign: 1.0,
                    set_width_chars: 6,
                    add_css_class: "track-duration",
                },
            }
        }
    }

    fn init_model(init: Self::Init, _index: &Self::Index, _sender: FactorySender<Self>) -> Self {
        Self {
            display_index: init.display_index,
            track: init.track,
        }
    }

    fn update(&mut self, message: Self::Input, sender: FactorySender<Self>) {
        match message {
            TrackRowInput::Clicked => {
                let _ = sender.output(TrackRowOutput::Play(self.track.id.clone()));
            }
        }
    }
}
