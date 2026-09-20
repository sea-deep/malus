//! Live radio station pill banner component for Apple Music live radio.

use malus_model::MediaRef;
use relm4::gtk::glib;
use relm4::gtk::{self, prelude::*};
use relm4::prelude::*;

use crate::design::tokens::*;
use crate::services::{ArtworkService, bind_artwork};

pub struct LiveStationPill {
    pub id: String,
    pub title: String,
    pub artwork_url: Option<String>,
    pub entity: Option<MediaRef>,
    artwork_service: ArtworkService,
}

#[derive(Debug, Clone)]
pub struct LiveStationPillInit {
    pub id: String,
    pub title: String,
    pub artwork_url: Option<String>,
    pub entity: Option<MediaRef>,
    pub artwork_service: ArtworkService,
}

#[derive(Debug)]
pub enum LiveStationPillInput {
    Clicked,
    PlayClicked,
}

#[derive(Debug, Clone)]
pub enum LiveStationPillOutput {
    Play(MediaRef),
}

#[relm4::component(pub)]
impl Component for LiveStationPill {
    type Init = LiveStationPillInit;
    type Input = LiveStationPillInput;
    type Output = LiveStationPillOutput;
    type CommandOutput = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_cursor_from_name: Some("pointer"),
            set_halign: gtk::Align::Center,
            set_valign: gtk::Align::Center,

            #[name(pill_overlay)]
            gtk::Overlay {
                set_width_request: LIVE_STATION_PILL_WIDTH,
                set_height_request: LIVE_STATION_PILL_HEIGHT,
                add_css_class: "live-station-pill",

                add_overlay = &gtk::Button {
                    add_css_class: "card-play-overlay",
                    set_icon_name: ICON_PLAY,
                    set_tooltip_text: Some("Play Live"),
                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::Center,
                    connect_clicked => LiveStationPillInput::PlayClicked,
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let picture = gtk::Picture::builder()
            .can_shrink(true)
            .content_fit(gtk::ContentFit::Cover)
            .width_request(LIVE_STATION_PILL_WIDTH)
            .height_request(LIVE_STATION_PILL_HEIGHT)
            .css_classes(vec!["live-station-picture".to_string()])
            .build();

        let model = Self {
            id: init.id,
            title: init.title,
            artwork_url: init.artwork_url.clone(),
            entity: init.entity,
            artwork_service: init.artwork_service.clone(),
        };

        let widgets = view_output!();
        widgets.pill_overlay.set_child(Some(&picture));

        // Bind wide banner artwork
        bind_artwork(
            &picture,
            &model.artwork_service,
            model.artwork_url.clone(),
            800,
        );

        // Pill gesture click
        let s = sender.clone();
        let gesture = gtk::GestureClick::new();
        gesture.set_button(gtk::gdk::BUTTON_PRIMARY);
        gesture.connect_released(move |_gesture, _n_press, _x, _y| {
            s.input(LiveStationPillInput::Clicked);
        });
        root.add_controller(gesture);

        root.set_focusable(model.entity.is_some());
        root.update_property(&[gtk::accessible::Property::Label(&model.title)]);
        let key = gtk::EventControllerKey::new();
        key.connect_key_pressed(move |_, key, _, _| {
            if key == gtk::gdk::Key::Return || key == gtk::gdk::Key::KP_Enter {
                sender.input(LiveStationPillInput::PlayClicked);
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
            LiveStationPillInput::Clicked | LiveStationPillInput::PlayClicked => {
                if let Some(ref entity) = self.entity {
                    let _ = sender.output(LiveStationPillOutput::Play(entity.clone()));
                }
            }
        }
    }
}
