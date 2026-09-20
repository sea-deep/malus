//! Native Apple Music gradient station and genre card component.

use malus_model::{MediaRef, PageRoute};
use relm4::gtk::glib;
use relm4::gtk::{self, prelude::*};
use relm4::prelude::*;

use crate::design::tokens::*;

pub struct StationCard {
    pub id: String,
    pub title: String,
    pub genre_label: String,
    pub subtitle: Option<String>,
    pub bg_color: Option<String>,
    pub entity: Option<MediaRef>,
}

#[derive(Debug, Clone)]
pub struct StationCardInit {
    pub id: String,
    pub title: String,
    pub genre_label: String,
    pub subtitle: Option<String>,
    pub bg_color: Option<String>,
    pub entity: Option<MediaRef>,
}

#[derive(Debug)]
pub enum StationCardInput {
    Clicked,
    PlayClicked,
}

#[derive(Debug, Clone)]
pub enum StationCardOutput {
    Play(MediaRef),
    Navigate(PageRoute),
}

#[relm4::component(pub)]
impl Component for StationCard {
    type Init = StationCardInit;
    type Input = StationCardInput;
    type Output = StationCardOutput;
    type CommandOutput = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: SPACING_XS,
            add_css_class: "station-card",
            set_cursor_from_name: Some("pointer"),
            set_width_request: STATION_CARD_SIZE,
            set_halign: gtk::Align::Center,
            set_valign: gtk::Align::Start,

            // Gradient canvas with Apple logo and bold genre typography
            #[name(canvas_overlay)]
            gtk::Overlay {
                set_width_request: STATION_CARD_SIZE,
                set_height_request: STATION_CARD_SIZE,
                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::Center,

                #[name(canvas_box)]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_width_request: STATION_CARD_SIZE,
                    set_height_request: STATION_CARD_SIZE,
                    add_css_class: "station-card-canvas",

                    // Top: Malus logo badge
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_halign: gtk::Align::End,

                        gtk::Image {
                            set_icon_name: Some("malus-logo-symbolic"),
                            set_pixel_size: 18,
                            add_css_class: "station-malus-logo",
                        },
                    },

                    // Vexpand spacer
                    gtk::Box {
                        set_vexpand: true,
                    },

                    // Bottom: Large bold genre name
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_halign: gtk::Align::Start,

                        gtk::Label {
                            set_xalign: 0.0,
                            set_wrap: true,
                            set_wrap_mode: gtk::pango::WrapMode::WordChar,
                            set_lines: 2,
                            set_max_width_chars: 12,
                            add_css_class: "station-genre-label",
                            #[watch]
                            set_text: &model.genre_label,
                        },
                    },
                },

                // Centered circular play button on hover
                add_overlay = &gtk::Button {
                    add_css_class: "card-play-overlay",
                    set_icon_name: ICON_PLAY,
                    set_tooltip_text: Some("Play Station"),
                    set_focus_on_click: false,
                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::Center,
                    connect_clicked => StationCardInput::PlayClicked,
                },
            },

            // Metadata below card
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 2,
                set_height_request: 48,

                gtk::Label {
                    set_xalign: 0.0,
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                    set_lines: 1,
                    set_max_width_chars: 18,
                    add_css_class: "card-title",
                    #[watch]
                    set_text: &model.title,
                },

                gtk::Label {
                    set_xalign: 0.0,
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                    set_lines: 1,
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
        let model = Self {
            id: init.id.clone(),
            title: init.title,
            genre_label: init.genre_label,
            subtitle: init.subtitle,
            bg_color: init.bg_color.clone(),
            entity: init.entity,
        };

        let widgets = view_output!();

        // Apply authentic CSS linear gradient based on station's bgColor
        let hex = init.bg_color.as_deref().unwrap_or("#5b7bdc");
        let clean_hex = hex.trim();
        let safe_id = init.id.replace(|c: char| !c.is_alphanumeric(), "_");
        let class_name = format!("station-grad-{safe_id}");
        let css = format!(
            ".{class_name} {{ background-image: linear-gradient(145deg, {clean_hex} 0%, rgba(0, 0, 0, 0.42) 100%); background-color: {clean_hex}; }}"
        );
        let provider = gtk::CssProvider::new();
        provider.load_from_string(&css);
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
        widgets.canvas_box.add_css_class(&class_name);

        // Click gesture
        let s = sender.clone();
        let gesture = gtk::GestureClick::new();
        gesture.set_button(gtk::gdk::BUTTON_PRIMARY);
        gesture.connect_released(move |_gesture, _n_press, _x, _y| {
            s.input(StationCardInput::Clicked);
        });
        root.add_controller(gesture);

        root.set_focusable(model.entity.is_some());
        root.update_property(&[gtk::accessible::Property::Label(&model.title)]);
        let key = gtk::EventControllerKey::new();
        key.connect_key_pressed(move |_, key, _, _| {
            if key == gtk::gdk::Key::Return || key == gtk::gdk::Key::KP_Enter {
                sender.input(StationCardInput::PlayClicked);
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
            StationCardInput::Clicked | StationCardInput::PlayClicked => {
                if let Some(ref entity) = self.entity {
                    let _ = sender.output(StationCardOutput::Play(entity.clone()));
                }
            }
        }
    }
}
