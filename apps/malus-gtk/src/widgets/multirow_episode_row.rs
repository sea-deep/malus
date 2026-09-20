//! Compact episode row widget for multi-row horizontal episode shelves (Radio episodes).

use malus_model::MediaRef;
use relm4::gtk::glib;
use relm4::gtk::{self, prelude::*};
use relm4::prelude::*;

use crate::design::tokens::*;
use crate::services::{ArtworkService, bind_artwork};
use crate::widgets::square_artwork::SquareArtwork;

pub struct MultiRowEpisodeRow {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub artwork_url: Option<String>,
    pub entity: Option<MediaRef>,
    artwork_service: ArtworkService,
}

#[derive(Debug, Clone)]
pub struct MultiRowEpisodeRowInit {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub artwork_url: Option<String>,
    pub entity: Option<MediaRef>,
    pub artwork_service: ArtworkService,
}

#[derive(Debug)]
pub enum MultiRowEpisodeRowInput {
    PlayClicked,
}

#[derive(Debug, Clone)]
pub enum MultiRowEpisodeRowOutput {
    Play(MediaRef),
}

#[relm4::component(pub)]
impl Component for MultiRowEpisodeRow {
    type Init = MultiRowEpisodeRowInit;
    type Input = MultiRowEpisodeRowInput;
    type Output = MultiRowEpisodeRowOutput;
    type CommandOutput = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: SPACING_SM,
            add_css_class: "compact-track-row",
            set_cursor_from_name: Some("pointer"),
            set_width_request: MULTIROW_EPISODE_WIDTH,
            set_height_request: 56,
            set_valign: gtk::Align::Center,

            // Square artwork
            #[name(art_host)]
            gtk::Box {
                set_valign: gtk::Align::Center,
            },

            // Title & Subtitle (Show / Host)
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 2,
                set_hexpand: true,
                set_valign: gtk::Align::Center,

                gtk::Label {
                    set_xalign: 0.0,
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                    set_lines: 1,
                    add_css_class: "compact-track-title",
                    #[watch]
                    set_text: &model.title,
                },

                gtk::Label {
                    set_xalign: 0.0,
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                    set_lines: 1,
                    add_css_class: "compact-track-sub",
                    #[watch]
                    set_visible: model.subtitle.is_some(),
                    #[watch]
                    set_text: model.subtitle.as_deref().unwrap_or(""),
                },
            },

            // Play action button
            gtk::Button {
                set_icon_name: ICON_PLAY,
                set_tooltip_text: Some("Play Episode"),
                add_css_class: "flat",
                add_css_class: "track-action-btn",
                set_focus_on_click: false,
                set_valign: gtk::Align::Center,
                connect_clicked => MultiRowEpisodeRowInput::PlayClicked,
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Self {
            id: init.id,
            title: init.title,
            subtitle: init.subtitle,
            artwork_url: init.artwork_url.clone(),
            entity: init.entity,
            artwork_service: init.artwork_service.clone(),
        };

        let widgets = view_output!();

        // Artwork
        let art = SquareArtwork::new(MULTIROW_EPISODE_THUMB_SIZE, "compact-track-art");
        bind_artwork(
            art.picture(),
            &model.artwork_service,
            model.artwork_url.clone(),
            128,
        );
        widgets.art_host.append(&art);

        // Click gesture on root to play
        let s = sender.clone();
        let gesture = gtk::GestureClick::new();
        gesture.set_button(gtk::gdk::BUTTON_PRIMARY);
        gesture.connect_released(move |_gesture, _n_press, _x, _y| {
            s.input(MultiRowEpisodeRowInput::PlayClicked);
        });
        root.add_controller(gesture);

        root.set_focusable(model.entity.is_some());
        root.update_property(&[gtk::accessible::Property::Label(&model.title)]);
        let key = gtk::EventControllerKey::new();
        key.connect_key_pressed(move |_, key, _, _| {
            if key == gtk::gdk::Key::Return || key == gtk::gdk::Key::KP_Enter {
                sender.input(MultiRowEpisodeRowInput::PlayClicked);
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        root.add_controller(key);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            MultiRowEpisodeRowInput::PlayClicked => {
                if let Some(ref entity) = self.entity {
                    let _ = sender.output(MultiRowEpisodeRowOutput::Play(entity.clone()));
                }
            }
        }
    }
}
