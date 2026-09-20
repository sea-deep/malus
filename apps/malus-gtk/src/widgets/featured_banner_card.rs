//! Hero featured banner card component for the Browse/New carousel.

use malus_model::{MediaRef, PageRoute};
use relm4::gtk::glib;
use relm4::gtk::{self, prelude::*};
use relm4::prelude::*;

use crate::design::tokens::*;
use crate::services::{ArtworkService, bind_artwork};

pub struct FeaturedBannerCard {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub eyebrow: Option<String>,
    pub artwork_url: Option<String>,
    pub entity: Option<MediaRef>,
    pub open_route: Option<PageRoute>,
    artwork_service: ArtworkService,
}

#[derive(Debug, Clone)]
pub struct FeaturedBannerCardInit {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub eyebrow: Option<String>,
    pub artwork_url: Option<String>,
    pub entity: Option<MediaRef>,
    pub open_route: Option<PageRoute>,
    pub artwork_service: ArtworkService,
    pub eager: bool,
}

#[derive(Debug)]
pub enum FeaturedBannerCardInput {
    Clicked,
    PlayClicked,
}

#[derive(Debug, Clone)]
pub enum FeaturedBannerCardOutput {
    Navigate(PageRoute),
    Play(MediaRef),
}

#[relm4::component(pub)]
impl Component for FeaturedBannerCard {
    type Init = FeaturedBannerCardInit;
    type Input = FeaturedBannerCardInput;
    type Output = FeaturedBannerCardOutput;
    type CommandOutput = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: SPACING_XS,
            add_css_class: "featured-banner-card",
            set_cursor_from_name: Some("pointer"),
            set_width_request: FEATURED_BANNER_WIDTH,
            set_halign: gtk::Align::Center,
            set_valign: gtk::Align::Start,

            // Header typography above the landscape artwork
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 2,

                gtk::Label {
                    set_xalign: 0.0,
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                    add_css_class: "featured-banner-eyebrow",
                    #[watch]
                    set_visible: model.eyebrow.is_some(),
                    #[watch]
                    set_text: model.eyebrow.as_deref().unwrap_or(""),
                },

                gtk::Label {
                    set_xalign: 0.0,
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                    set_lines: 1,
                    add_css_class: "featured-banner-title",
                    #[watch]
                    set_text: &model.title,
                },

                gtk::Label {
                    set_xalign: 0.0,
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                    set_lines: 1,
                    add_css_class: "featured-banner-sub",
                    #[watch]
                    set_visible: model.subtitle.is_some(),
                    #[watch]
                    set_text: model.subtitle.as_deref().unwrap_or(""),
                },
            },

            // Landscape banner artwork with hover play overlay
            #[name(art_overlay)]
            gtk::Overlay {
                set_width_request: FEATURED_BANNER_WIDTH,
                set_height_request: 170,
                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::Center,

                add_overlay = &gtk::Button {
                    add_css_class: "card-play-overlay",
                    set_icon_name: ICON_PLAY,
                    set_tooltip_text: Some("Play"),
                    set_halign: gtk::Align::End,
                    set_valign: gtk::Align::End,
                    set_margin_end: 12,
                    set_margin_bottom: 12,
                    #[watch]
                    set_visible: model.entity.is_some(),
                    connect_clicked => FeaturedBannerCardInput::PlayClicked,
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
            .width_request(FEATURED_BANNER_WIDTH)
            .height_request(170)
            .css_classes(vec!["featured-banner-picture".to_string()])
            .build();

        let model = Self {
            id: init.id,
            title: init.title,
            subtitle: init.subtitle,
            eyebrow: init.eyebrow,
            artwork_url: init.artwork_url.clone(),
            entity: init.entity,
            open_route: init.open_route,
            artwork_service: init.artwork_service.clone(),
        };

        let widgets = view_output!();
        widgets.art_overlay.set_child(Some(&picture));

        bind_artwork(
            &picture,
            &model.artwork_service,
            model.artwork_url.clone(),
            1200,
        );

        // Click gesture
        let s = sender.clone();
        let gesture = gtk::GestureClick::new();
        gesture.set_button(gtk::gdk::BUTTON_PRIMARY);
        gesture.connect_released(move |_gesture, _n_press, _x, _y| {
            s.input(FeaturedBannerCardInput::Clicked);
        });
        root.add_controller(gesture);

        root.set_focusable(model.open_route.is_some() || model.entity.is_some());
        root.update_property(&[gtk::accessible::Property::Label(&model.title)]);
        let key = gtk::EventControllerKey::new();
        key.connect_key_pressed(move |_, key, _, _| {
            if key == gtk::gdk::Key::Return || key == gtk::gdk::Key::KP_Enter {
                sender.input(FeaturedBannerCardInput::Clicked);
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
            FeaturedBannerCardInput::Clicked => {
                if let Some(ref route) = self.open_route {
                    let _ = sender.output(FeaturedBannerCardOutput::Navigate(route.clone()));
                } else if let Some(ref entity) = self.entity {
                    let _ = sender.output(FeaturedBannerCardOutput::Play(entity.clone()));
                }
            }
            FeaturedBannerCardInput::PlayClicked => {
                if let Some(ref entity) = self.entity {
                    let _ = sender.output(FeaturedBannerCardOutput::Play(entity.clone()));
                }
            }
        }
    }
}
