use malus_ipc::wire::PageActionWire;
use malus_model::{MediaRef, PageRoute};
use relm4::gtk::glib;
use relm4::gtk::{self, prelude::*};
use relm4::prelude::*;

use crate::design::tokens::*;
use crate::services::{ArtworkService, bind_artwork};
use crate::widgets::actions_menu::ActionMenuCommand;
use crate::widgets::square_artwork::SquareArtwork;

pub struct MultiRowTrackRow {
    pub id: String,
    pub title: String,
    pub artist: Option<String>,
    pub artwork_url: Option<String>,
    pub entity: Option<MediaRef>,
    pub is_explicit: bool,
    pub is_favorite: bool,
    pub in_library: bool,
    pub actions: Vec<PageActionWire>,
    pub album_route: Option<PageRoute>,
    pub artist_route: Option<PageRoute>,
    artwork_service: ArtworkService,
}

#[derive(Debug, Clone)]
pub struct MultiRowTrackRowInit {
    pub id: String,
    pub title: String,
    pub artist: Option<String>,
    pub artwork_url: Option<String>,
    pub entity: Option<MediaRef>,
    pub is_explicit: bool,
    pub is_favorite: bool,
    pub in_library: bool,
    pub actions: Vec<PageActionWire>,
    pub album_route: Option<PageRoute>,
    pub artist_route: Option<PageRoute>,
    pub artwork_service: ArtworkService,
}

#[derive(Debug)]
pub enum MultiRowTrackRowInput {
    PlayClicked,
    FavoriteClicked,
    MenuAction(ActionMenuCommand),
    RightClicked(f64, f64),
}

#[derive(Debug, Clone)]
pub enum MultiRowTrackRowOutput {
    Play(MediaRef),
    Action(PageActionWire),
    Navigate(PageRoute),
    CopyLink(String),
}

#[relm4::component(pub)]
impl Component for MultiRowTrackRow {
    type Init = MultiRowTrackRowInit;
    type Input = MultiRowTrackRowInput;
    type Output = MultiRowTrackRowOutput;
    type CommandOutput = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: SPACING_SM,
            add_css_class: "compact-track-row",
            set_cursor_from_name: Some("pointer"),
            set_width_request: MULTIROW_TRACK_WIDTH,
            set_height_request: 48,
            set_valign: gtk::Align::Center,

            // Square artwork
            #[name(art_host)]
            gtk::Box {
                set_valign: gtk::Align::Center,
            },

            // Title & Artist
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 1,
                set_hexpand: true,
                set_valign: gtk::Align::Center,

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: SPACING_XXS,

                    gtk::Label {
                        set_xalign: 0.0,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        set_lines: 1,
                        add_css_class: "compact-track-title",
                        #[watch]
                        set_text: &model.title,
                    },

                    gtk::Label {
                        set_label: "E",
                        add_css_class: "badge-explicit",
                        set_valign: gtk::Align::Center,
                        #[watch]
                        set_visible: model.is_explicit,
                    },
                },

                #[name(artist_lbl)]
                gtk::Label {
                    set_xalign: 0.0,
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                    set_lines: 1,
                    add_css_class: "compact-track-sub",
                    #[watch]
                    set_visible: model.artist.is_some(),
                    #[watch]
                    set_text: model.artist.as_deref().unwrap_or(""),
                },
            },

            // Action buttons (Favorite & More)
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: SPACING_XXS,
                set_valign: gtk::Align::Center,

                gtk::Button {
                    #[watch]
                    set_icon_name: if model.is_favorite { ICON_FAVORITE } else { ICON_FAVORITE_OUTLINE },
                    set_tooltip_text: Some(if model.is_favorite { "Unfavorite" } else { "Favorite" }),
                    add_css_class: "flat",
                    add_css_class: "track-action-btn",
                    set_focus_on_click: false,
                    #[watch]
                    set_css_classes: if model.is_favorite {
                        &["flat", "track-action-btn", "favorite-active"]
                    } else {
                        &["flat", "track-action-btn"]
                    },
                    connect_clicked => MultiRowTrackRowInput::FavoriteClicked,
                },

                #[name(more_btn)]
                gtk::MenuButton {
                    set_icon_name: ICON_MORE,
                    set_tooltip_text: Some("More actions"),
                    add_css_class: "flat",
                    add_css_class: "track-action-btn",
                    set_focus_on_click: false,
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
            id: init.id,
            title: init.title,
            artist: init.artist,
            artwork_url: init.artwork_url.clone(),
            entity: init.entity,
            is_explicit: init.is_explicit,
            is_favorite: init.is_favorite,
            in_library: init.in_library,
            actions: init.actions.clone(),
            album_route: init.album_route,
            artist_route: init.artist_route,
            artwork_service: init.artwork_service.clone(),
        };

        let widgets = view_output!();

        // Artwork
        let art = SquareArtwork::new(MULTIROW_TRACK_THUMB_SIZE, "compact-track-art");
        bind_artwork(
            art.picture(),
            &model.artwork_service,
            model.artwork_url.clone(),
            128,
        );
        widgets.art_host.append(&art);

        // Clickable artist navigation
        if let Some(route) = model.artist_route.clone() {
            widgets.artist_lbl.add_css_class("metadata-link");
            widgets.artist_lbl.set_cursor_from_name(Some("pointer"));
            let s_art = sender.clone();
            let art_gesture = gtk::GestureClick::new();
            art_gesture.connect_released(move |g, n_press, _x, _y| {
                if n_press == 1 {
                    g.set_state(gtk::EventSequenceState::Claimed);
                    let _ = s_art.output(MultiRowTrackRowOutput::Navigate(route.clone()));
                }
            });
            widgets.artist_lbl.add_controller(art_gesture);
        }

        // More popover menu
        if let Some(ref entity) = model.entity {
            let s = sender.clone();
            let popover = crate::widgets::actions_menu::build_action_popover_full(
                entity,
                model.is_favorite,
                model.in_library,
                false,
                &model.actions,
                None,
                model.album_route.clone(),
                model.artist_route.clone(),
                move |cmd| {
                    s.input(MultiRowTrackRowInput::MenuAction(cmd));
                },
            );
            widgets.more_btn.set_popover(Some(&popover));
        }

        // Primary click gesture on root to play
        let s = sender.clone();
        let gesture = gtk::GestureClick::new();
        gesture.set_button(gtk::gdk::BUTTON_PRIMARY);
        gesture.connect_released(move |_gesture, _n_press, _x, _y| {
            s.input(MultiRowTrackRowInput::PlayClicked);
        });
        root.add_controller(gesture);

        // Secondary (right) click gesture on root for context menu
        let s_rc = sender.clone();
        let rc_gesture = gtk::GestureClick::new();
        rc_gesture.set_button(gtk::gdk::BUTTON_SECONDARY);
        rc_gesture.connect_pressed(move |g, _n, x, y| {
            g.set_state(gtk::EventSequenceState::Claimed);
            s_rc.input(MultiRowTrackRowInput::RightClicked(x, y));
        });
        root.add_controller(rc_gesture);

        root.set_focusable(model.entity.is_some());
        root.update_property(&[gtk::accessible::Property::Label(&model.title)]);
        let key = gtk::EventControllerKey::new();
        key.connect_key_pressed(move |_, key, _, _| {
            if key == gtk::gdk::Key::Return || key == gtk::gdk::Key::KP_Enter {
                sender.input(MultiRowTrackRowInput::PlayClicked);
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
            MultiRowTrackRowInput::PlayClicked => {
                if let Some(ref entity) = self.entity {
                    let _ = sender.output(MultiRowTrackRowOutput::Play(entity.clone()));
                }
            }
            MultiRowTrackRowInput::FavoriteClicked => {
                if let Some(ref entity) = self.entity {
                    self.is_favorite = !self.is_favorite;
                    let action = if self.is_favorite {
                        PageActionWire::Favorite(entity.clone())
                    } else {
                        PageActionWire::Unfavorite(entity.clone())
                    };
                    let _ = sender.output(MultiRowTrackRowOutput::Action(action));
                }
            }
            MultiRowTrackRowInput::MenuAction(cmd) => match cmd {
                ActionMenuCommand::Action(a) => {
                    let _ = sender.output(MultiRowTrackRowOutput::Action(a));
                }
                ActionMenuCommand::Navigate(r) => {
                    let _ = sender.output(MultiRowTrackRowOutput::Navigate(r));
                }
                ActionMenuCommand::CopyLink(u) => {
                    let _ = sender.output(MultiRowTrackRowOutput::CopyLink(u));
                }
                _ => {}
            },
            MultiRowTrackRowInput::RightClicked(_, _) => {}
        }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        message: Self::Input,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        if let MultiRowTrackRowInput::RightClicked(x, y) = message
            && let Some(ref entity) = self.entity
        {
            let s = sender.clone();
            let popover = crate::widgets::actions_menu::build_action_popover_full(
                entity,
                self.is_favorite,
                self.in_library,
                false,
                &self.actions,
                None,
                self.album_route.clone(),
                self.artist_route.clone(),
                move |cmd| {
                    s.input(MultiRowTrackRowInput::MenuAction(cmd));
                },
            );
            popover.set_parent(root);
            popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            popover.connect_closed(|p| {
                p.unparent();
            });
            popover.popup();
            return;
        }

        self.update(message, sender.clone(), root);
        self.update_view(widgets, sender);
    }
}
