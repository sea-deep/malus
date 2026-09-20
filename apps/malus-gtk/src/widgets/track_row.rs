//! High quality reusable track row component.

use malus_ipc::wire::PageActionWire;
use malus_model::{MediaRef, PageRoute, Track};
use relm4::gtk::{self, prelude::*};
use relm4::prelude::*;

use crate::design::tokens::*;
use crate::model::format_time;
use crate::services::{ArtworkService, bind_artwork};
use crate::widgets::actions_menu::{ActionMenuCommand, build_action_popover_full};
use crate::widgets::square_artwork::SquareArtwork;

pub struct TrackRow {
    pub track: Track,
    pub index: Option<usize>,
    pub show_artwork: bool,
    pub is_favorite: bool,
    pub in_library: bool,
    pub actions: Vec<PageActionWire>,
    pub playlist_context: Option<(MediaRef, usize)>,
    pub album_route: Option<PageRoute>,
    pub artist_route: Option<PageRoute>,
}

#[derive(Debug, Clone)]
pub struct TrackRowInit {
    pub track: Track,
    pub index: Option<usize>,
    pub show_artwork: bool,
    pub is_favorite: bool,
    pub in_library: bool,
    pub actions: Vec<PageActionWire>,
    pub artwork_service: ArtworkService,
    pub playlist_context: Option<(MediaRef, usize)>,
    pub album_route: Option<PageRoute>,
    pub artist_route: Option<PageRoute>,
}

#[derive(Debug)]
pub enum TrackRowInput {
    PlayClicked,
    FavoriteToggled,
    MenuAction(ActionMenuCommand),
    SetFavorite(bool),
    MediaState(malus_model::AccountMediaState),
    RightClicked(f64, f64),
}

#[derive(Debug, Clone)]
pub enum TrackRowOutput {
    Play(MediaRef),
    Action(PageActionWire),
    ViewCredits(MediaRef),
    AddToPlaylist(MediaRef),
    RemoveFromPlaylist {
        playlist: MediaRef,
        track_index: usize,
        expected_track: MediaRef,
    },
    Navigate(PageRoute),
    CopyLink(String),
}

#[relm4::component(pub)]
impl Component for TrackRow {
    type Init = TrackRowInit;
    type Input = TrackRowInput;
    type Output = TrackRowOutput;
    type CommandOutput = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 12,
            add_css_class: "track-row",
            set_hexpand: true,
            set_valign: gtk::Align::Center,
            set_height_request: 48,

            // Index or Play Button
            gtk::Box {
                set_size_request: (32, -1),
                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::Center,

                gtk::Label {
                    add_css_class: "track-number",
                    #[watch]
                    set_visible: model.index.is_some(),
                    #[watch]
                    set_text: &model.index.map(|i| (i + 1).to_string()).unwrap_or_default(),
                },
            },

            // Optional Thumbnail
            #[name(art_picture)]
            SquareArtwork {
                set_side: 40,
                add_css_class: "track-artwork",
                set_valign: gtk::Align::Center,
                #[watch]
                set_visible: model.show_artwork,
            },

            // Track Title & Artist
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 2,
                set_hexpand: true,
                set_valign: gtk::Align::Center,

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 6,

                    gtk::Label {
                        set_xalign: 0.0,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        add_css_class: "track-title",
                        #[watch]
                        set_text: &model.track.title,
                    },

                    // Explicit badge
                    gtk::Label {
                        add_css_class: "badge-explicit",
                        set_text: "E",
                        #[watch]
                        set_visible: model.track.explicit.unwrap_or(false),
                    },
                },

                #[name(artist_lbl)]
                gtk::Label {
                    set_xalign: 0.0,
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                    add_css_class: "track-artist",
                    #[watch]
                    set_text: &model.track.artist_display(),
                },
            },

            // Duration
            gtk::Label {
                add_css_class: "track-duration",
                set_valign: gtk::Align::Center,
                #[watch]
                set_text: &format_time(model.track.duration_ms.unwrap_or(0)),
            },

            // Favorite Star Button
            #[name(fav_btn)]
            gtk::Button {
                add_css_class: "flat",
                add_css_class: "track-action-btn",
                set_focus_on_click: false,
                #[watch]
                set_icon_name: if model.is_favorite {
                    ICON_FAVORITE
                } else {
                    ICON_FAVORITE_OUTLINE
                },
                set_valign: gtk::Align::Center,
                #[watch]
                set_css_classes: if model.is_favorite {
                    &["flat", "track-action-btn", "favorite-active"]
                } else {
                    &["flat", "track-action-btn"]
                },
                #[watch]
                set_tooltip_text: Some(if model.is_favorite {
                    "Unfavorite"
                } else {
                    "Favorite"
                }),
                connect_clicked => TrackRowInput::FavoriteToggled,
            },

            // Context Action Menu Button
            #[name(menu_btn)]
            gtk::MenuButton {
                add_css_class: "flat",
                add_css_class: "track-action-btn",
                set_focus_on_click: false,
                set_icon_name: ICON_MORE,
                set_tooltip_text: Some("More actions"),
                set_valign: gtk::Align::Center,
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Self {
            track: init.track.clone(),
            index: init.index,
            show_artwork: init.show_artwork,
            is_favorite: init.is_favorite,
            in_library: init.in_library,
            actions: init.actions,
            playlist_context: init.playlist_context,
            album_route: init.album_route,
            artist_route: init.artist_route,
        };

        let widgets = view_output!();

        if init.show_artwork {
            let url = init.track.artwork.as_ref().map(|a| a.url.clone());
            bind_artwork(
                widgets.art_picture.picture(),
                &init.artwork_service,
                url,
                96,
            );
        }

        // Action menu popover
        let s = sender.clone();
        let popover = build_action_popover_full(
            &model.track.id,
            model.is_favorite,
            model.in_library,
            true,
            &model.actions,
            model.playlist_context.clone(),
            model.album_route.clone(),
            model.artist_route.clone(),
            move |cmd| {
                s.input(TrackRowInput::MenuAction(cmd));
            },
        );
        widgets.menu_btn.set_popover(Some(&popover));

        // Clickable artist label navigation
        if let Some(route) = model.artist_route.clone() {
            widgets.artist_lbl.add_css_class("metadata-link");
            widgets.artist_lbl.set_cursor_from_name(Some("pointer"));
            let s_art = sender.clone();
            let art_gesture = gtk::GestureClick::new();
            art_gesture.connect_released(move |g, n_press, _x, _y| {
                if n_press == 1 {
                    g.set_state(gtk::EventSequenceState::Claimed);
                    let _ = s_art.output(TrackRowOutput::Navigate(route.clone()));
                }
            });
            widgets.artist_lbl.add_controller(art_gesture);
        }

        // Primary click to play
        let s_click = sender.clone();
        let gesture = gtk::GestureClick::new();
        gesture.set_button(gtk::gdk::BUTTON_PRIMARY);
        gesture.connect_released(move |_g, n_press, _x, _y| {
            if n_press >= 1 {
                s_click.input(TrackRowInput::PlayClicked);
            }
        });
        root.add_controller(gesture);

        // Secondary (right) click context menu
        let s_rc = sender.clone();
        let rc_gesture = gtk::GestureClick::new();
        rc_gesture.set_button(gtk::gdk::BUTTON_SECONDARY);
        rc_gesture.connect_pressed(move |g, _n, x, y| {
            g.set_state(gtk::EventSequenceState::Claimed);
            s_rc.input(TrackRowInput::RightClicked(x, y));
        });
        root.add_controller(rc_gesture);

        root.set_focusable(true);
        root.update_property(&[gtk::accessible::Property::Label(&format!(
            "Play {}",
            model.track.title
        ))]);
        let key = gtk::EventControllerKey::new();
        key.connect_key_pressed(move |_, key, _, _| {
            if key == gtk::gdk::Key::Return || key == gtk::gdk::Key::KP_Enter {
                sender.input(TrackRowInput::PlayClicked);
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
            TrackRowInput::PlayClicked => {
                let _ = sender.output(TrackRowOutput::Play(self.track.id.clone()));
            }
            TrackRowInput::FavoriteToggled => {
                let next_fav = !self.is_favorite;
                self.is_favorite = next_fav;
                for action in &mut self.actions {
                    match action {
                        PageActionWire::Favorite(r) if next_fav => {
                            *action = PageActionWire::Unfavorite(r.clone());
                        }
                        PageActionWire::Unfavorite(r) if !next_fav => {
                            *action = PageActionWire::Favorite(r.clone());
                        }
                        _ => {}
                    }
                }
                let action = if !next_fav {
                    PageActionWire::Unfavorite(self.track.id.clone())
                } else {
                    PageActionWire::Favorite(self.track.id.clone())
                };
                let _ = sender.output(TrackRowOutput::Action(action));
            }
            TrackRowInput::MenuAction(cmd) => match cmd {
                ActionMenuCommand::Action(action) => {
                    let _ = sender.output(TrackRowOutput::Action(action));
                }
                ActionMenuCommand::ViewCredits(ref_id) => {
                    let _ = sender.output(TrackRowOutput::ViewCredits(ref_id));
                }
                ActionMenuCommand::AddToPlaylist(r) => {
                    let _ = sender.output(TrackRowOutput::AddToPlaylist(r));
                }
                ActionMenuCommand::RemoveFromPlaylist {
                    playlist,
                    track_index,
                    expected_track,
                } => {
                    let _ = sender.output(TrackRowOutput::RemoveFromPlaylist {
                        playlist,
                        track_index,
                        expected_track,
                    });
                }
                ActionMenuCommand::Navigate(r) => {
                    let _ = sender.output(TrackRowOutput::Navigate(r));
                }
                ActionMenuCommand::CopyLink(u) => {
                    let _ = sender.output(TrackRowOutput::CopyLink(u));
                }
            },
            TrackRowInput::MediaState(state) => {
                let matches =
                    state.reference == self.track.id || state.reference.id() == self.track.id.id();
                if !matches {
                    return;
                }
                self.is_favorite = state.favorite;
                self.in_library = state.in_library;
                for action in &mut self.actions {
                    match action {
                        PageActionWire::Favorite(r) if state.favorite => {
                            *action = PageActionWire::Unfavorite(r.clone());
                        }
                        PageActionWire::Unfavorite(r) if !state.favorite => {
                            *action = PageActionWire::Favorite(r.clone());
                        }
                        _ => {}
                    }
                }
            }
            TrackRowInput::SetFavorite(fav) => {
                self.is_favorite = fav;
            }
            TrackRowInput::RightClicked(_, _) => {}
        }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        message: Self::Input,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        if let TrackRowInput::RightClicked(x, y) = message {
            let s = sender.clone();
            let popover = build_action_popover_full(
                &self.track.id,
                self.is_favorite,
                self.in_library,
                true,
                &self.actions,
                self.playlist_context.clone(),
                self.album_route.clone(),
                self.artist_route.clone(),
                move |cmd| {
                    s.input(TrackRowInput::MenuAction(cmd));
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

        let refresh_menu = matches!(
            &message,
            TrackRowInput::SetFavorite(_) | TrackRowInput::MediaState(_)
        );
        self.update(message, sender.clone(), root);
        self.update_view(widgets, sender.clone());

        if !refresh_menu {
            return;
        }
        // Refresh only after authoritative account-state changes.
        let s = sender.clone();
        let popover = build_action_popover_full(
            &self.track.id,
            self.is_favorite,
            self.in_library,
            true,
            &self.actions,
            self.playlist_context.clone(),
            self.album_route.clone(),
            self.artist_route.clone(),
            move |cmd| {
                s.input(TrackRowInput::MenuAction(cmd));
            },
        );
        widgets.menu_btn.set_popover(Some(&popover));
    }
}
