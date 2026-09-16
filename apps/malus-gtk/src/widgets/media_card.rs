//! Reusable media card component for albums, playlists, artists, and stations.

use malus_model::{MediaRef, PageRoute};
use relm4::gtk::{self, prelude::*};
use relm4::prelude::*;

use crate::design::tokens::*;
use crate::services::{ArtworkService, bind_artwork};
use crate::widgets::square_artwork::SquareArtwork;

pub struct MediaCard {
    pub title: String,
    pub subtitle: Option<String>,
    pub overline: Option<String>,
    pub entity: Option<MediaRef>,
    pub open_route: Option<PageRoute>,
    pub is_circular: bool,
    pub size: i32,
    artwork_loaded: bool,
    artwork_url: Option<String>,
    artwork_service: ArtworkService,
    picture: gtk::Picture,
}

#[derive(Debug, Clone)]
pub struct MediaCardInit {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub overline: Option<String>,
    pub artwork_url: Option<String>,
    pub entity: Option<MediaRef>,
    pub open_route: Option<PageRoute>,
    pub size: i32,
    pub is_circular: bool,
    pub artwork_service: ArtworkService,
    pub eager: bool,
}

#[derive(Debug)]
pub enum MediaCardInput {
    Clicked,
    PlayClicked,
    LoadArtwork,
}

#[derive(Debug, Clone)]
pub enum MediaCardOutput {
    Navigate(PageRoute),
    Play(MediaRef),
}

#[relm4::component(pub)]
impl Component for MediaCard {
    type Init = MediaCardInit;
    type Input = MediaCardInput;
    type Output = MediaCardOutput;
    type CommandOutput = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 8,
            add_css_class: "media-card",
            set_cursor_from_name: Some("pointer"),
            set_width_request: model.size,
            set_halign: gtk::Align::Center,
            set_valign: gtk::Align::Start,

            // Artwork container with overlay play button
            #[name(artwork_overlay)]
            gtk::Overlay {
                set_width_request: model.size,
                set_height_request: model.size,
                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::Center,

                add_overlay = &gtk::Button {
                    add_css_class: "card-play-overlay",
                    set_icon_name: ICON_PLAY,
                    set_tooltip_text: Some("Play"),
                    set_halign: gtk::Align::End,
                    set_valign: gtk::Align::End,
                    set_margin_end: 8,
                    set_margin_bottom: 8,
                    #[watch]
                    set_visible: model.entity.is_some() && !model.is_circular,
                    connect_clicked => MediaCardInput::PlayClicked,
                },
            },

            // Reserve two title lines and metadata before the shelf scrollbar.
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 2,
                set_height_request: if model.overline.is_some() { 80 } else { 60 },

                gtk::Label {
                    #[watch]
                    set_xalign: if model.is_circular { 0.5 } else { 0.0 },
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                    set_max_width_chars: 20,
                    add_css_class: "card-overline",
                    #[watch]
                    set_visible: model.overline.is_some(),
                    #[watch]
                    set_text: model.overline.as_deref().unwrap_or(""),
                },

                gtk::Label {
                    #[watch]
                    set_xalign: if model.is_circular { 0.5 } else { 0.0 },
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                    set_lines: 2,
                    set_wrap: true,
                    set_wrap_mode: gtk::pango::WrapMode::WordChar,
                    set_max_width_chars: 18,
                    add_css_class: "card-title",
                    #[watch]
                    set_text: &model.title,
                },

                gtk::Label {
                    #[watch]
                    set_xalign: if model.is_circular { 0.5 } else { 0.0 },
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                    set_max_width_chars: 18,
                    add_css_class: "card-subtitle",
                    #[watch]
                    set_visible: model.subtitle.is_some(),
                    #[watch]
                    set_text: model.subtitle.as_deref().unwrap_or(""),
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let css_class = if init.is_circular {
            "card-artwork-circular"
        } else {
            "card-artwork"
        };

        let picture = gtk::Picture::new();
        let mut model = Self {
            title: init.title,
            subtitle: init.subtitle,
            overline: init.overline,
            entity: init.entity,
            open_route: init.open_route,
            is_circular: init.is_circular,
            size: init.size,
            artwork_loaded: false,
            artwork_url: init.artwork_url,
            artwork_service: init.artwork_service,
            picture,
        };

        let widgets = view_output!();
        let artwork = SquareArtwork::new(model.size, css_class);
        model.picture = artwork.picture().clone();
        widgets.artwork_overlay.set_child(Some(&artwork));
        let target_size = (model.size * 2).clamp(128, 480) as u32;

        if init.eager {
            model.artwork_loaded = true;
            bind_artwork(
                &model.picture,
                &model.artwork_service,
                model.artwork_url.clone(),
                target_size,
            );
        }

        // Card gesture click
        let s = sender.clone();
        let gesture = gtk::GestureClick::new();
        gesture.connect_released(move |_gesture, _n_press, _x, _y| {
            s.input(MediaCardInput::Clicked);
        });
        root.add_controller(gesture);

        // Focus controller: load artwork on keyboard navigation focus if deferred
        let s_focus = sender.clone();
        let focus = gtk::EventControllerFocus::new();
        focus.connect_enter(move |_| {
            s_focus.input(MediaCardInput::LoadArtwork);
        });
        root.add_controller(focus);

        root.set_focusable(model.open_route.is_some() || model.entity.is_some());
        root.update_property(&[gtk::accessible::Property::Label(&model.title)]);
        let key = gtk::EventControllerKey::new();
        key.connect_key_pressed(move |_, key, _, _| {
            if key == gtk::gdk::Key::Return || key == gtk::gdk::Key::space {
                sender.input(MediaCardInput::Clicked);
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
        root.add_controller(key);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            MediaCardInput::LoadArtwork => {
                if !self.artwork_loaded {
                    self.artwork_loaded = true;
                    let target_size = (self.size * 2).clamp(128, 480) as u32;
                    bind_artwork(
                        &self.picture,
                        &self.artwork_service,
                        self.artwork_url.clone(),
                        target_size,
                    );
                }
            }
            MediaCardInput::Clicked => {
                if !self.artwork_loaded {
                    self.artwork_loaded = true;
                    let target_size = (self.size * 2).clamp(128, 480) as u32;
                    bind_artwork(
                        &self.picture,
                        &self.artwork_service,
                        self.artwork_url.clone(),
                        target_size,
                    );
                }
                if let Some(ref route) = self.open_route {
                    let _ = sender.output(MediaCardOutput::Navigate(route.clone()));
                } else if let Some(ref entity) = self.entity {
                    let _ = sender.output(MediaCardOutput::Play(entity.clone()));
                } else {
                    // Decorative resources have no navigation or playback action.
                }
            }
            MediaCardInput::PlayClicked => {
                if let Some(ref entity) = self.entity {
                    let _ = sender.output(MediaCardOutput::Play(entity.clone()));
                }
            }
        }
    }
}
