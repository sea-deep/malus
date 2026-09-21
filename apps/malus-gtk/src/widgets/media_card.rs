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
    pub subtitle_route: Option<PageRoute>,
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
    pub subtitle_route: Option<PageRoute>,
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
    RightClicked(f64, f64),
}

#[derive(Debug, Clone)]
pub enum MediaCardOutput {
    Navigate(PageRoute),
    Play(MediaRef),
    CopyLink(String),
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
                    set_focus_on_click: false,
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

                #[name(subtitle_lbl)]
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
            subtitle_route: init.subtitle_route,
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
        let target_size = (model.size * 2).clamp(160, 800) as u32;

        if init.eager {
            model.artwork_loaded = true;
            bind_artwork(
                &model.picture,
                &model.artwork_service,
                model.artwork_url.clone(),
                target_size,
            );
        }

        // Clickable subtitle navigation (e.g. artist)
        if let Some(route) = model.subtitle_route.clone() {
            widgets.subtitle_lbl.add_css_class("metadata-link");
            widgets.subtitle_lbl.set_cursor_from_name(Some("pointer"));
            let s_sub = sender.clone();
            let sub_gesture = gtk::GestureClick::new();
            sub_gesture.connect_released(move |g, n_press, _x, _y| {
                if n_press == 1 {
                    g.set_state(gtk::EventSequenceState::Claimed);
                    let _ = s_sub.output(MediaCardOutput::Navigate(route.clone()));
                }
            });
            widgets.subtitle_lbl.add_controller(sub_gesture);
        }

        // Card gesture click (primary button)
        let s = sender.clone();
        let gesture = gtk::GestureClick::new();
        gesture.set_button(gtk::gdk::BUTTON_PRIMARY);
        gesture.connect_released(move |_gesture, _n_press, _x, _y| {
            s.input(MediaCardInput::Clicked);
        });
        root.add_controller(gesture);

        // Secondary click (right-click) for context menu
        let s_rc = sender.clone();
        let rc_gesture = gtk::GestureClick::new();
        rc_gesture.set_button(gtk::gdk::BUTTON_SECONDARY);
        rc_gesture.connect_pressed(move |g, _n_press, x, y| {
            g.set_state(gtk::EventSequenceState::Claimed);
            s_rc.input(MediaCardInput::RightClicked(x, y));
        });
        root.add_controller(rc_gesture);

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
            if key == gtk::gdk::Key::Return || key == gtk::gdk::Key::KP_Enter {
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
                    let target_size = (self.size * 2).clamp(160, 800) as u32;
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
                    let target_size = (self.size * 2).clamp(160, 800) as u32;
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
            MediaCardInput::RightClicked(_, _) => {}
        }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        message: Self::Input,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        if let MediaCardInput::RightClicked(x, y) = message {
            let link = self
                .open_route
                .as_ref()
                .and_then(|r| r.web_url())
                .or_else(|| self.entity.as_ref().and_then(|e| e.web_url()));

            if self.entity.is_some() || self.open_route.is_some() || link.is_some() {
                let popover = gtk::Popover::new();
                let box_menu = gtk::Box::new(gtk::Orientation::Vertical, 2);
                box_menu.set_margin_top(4);
                box_menu.set_margin_bottom(4);
                box_menu.set_margin_start(4);
                box_menu.set_margin_end(4);

                if let Some(ref entity) = self.entity {
                    let btn = gtk::Button::with_label("Play");
                    btn.add_css_class("flat");
                    btn.set_focus_on_click(false);
                    btn.set_halign(gtk::Align::Start);
                    let s = sender.clone();
                    let ent = entity.clone();
                    let p = popover.clone();
                    btn.connect_clicked(move |_| {
                        p.popdown();
                        let _ = s.output(MediaCardOutput::Play(ent.clone()));
                    });
                    box_menu.append(&btn);
                }

                if let Some(ref route) = self.open_route {
                    let btn = gtk::Button::with_label("Open");
                    btn.add_css_class("flat");
                    btn.set_focus_on_click(false);
                    btn.set_halign(gtk::Align::Start);
                    let s = sender.clone();
                    let rt = route.clone();
                    let p = popover.clone();
                    btn.connect_clicked(move |_| {
                        p.popdown();
                        let _ = s.output(MediaCardOutput::Navigate(rt.clone()));
                    });
                    box_menu.append(&btn);
                }

                if let Some(url) = link {
                    let btn = gtk::Button::with_label("Copy Link");
                    btn.add_css_class("flat");
                    btn.set_focus_on_click(false);
                    btn.set_halign(gtk::Align::Start);
                    let s = sender.clone();
                    let p = popover.clone();
                    btn.connect_clicked(move |_| {
                        p.popdown();
                        let _ = s.output(MediaCardOutput::CopyLink(url.clone()));
                    });
                    box_menu.append(&btn);
                }

                popover.set_child(Some(&box_menu));
                popover.set_parent(root);
                popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
                popover.connect_closed(|p| {
                    p.unparent();
                });
                popover.popup();
                return;
            }
        }

        self.update(message, sender.clone(), root);
        self.update_view(widgets, sender);
    }
}
