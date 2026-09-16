//! High quality reusable track row component.

use malus_ipc::wire::PageActionWire;
use malus_model::{MediaRef, Track};
use relm4::gtk::{self, prelude::*};
use relm4::prelude::*;

use crate::design::tokens::*;
use crate::model::format_time;
use crate::services::{ArtworkService, bind_artwork};
use crate::widgets::actions_menu::{ActionMenuCommand, build_action_popover_full};

pub struct TrackRow {
    pub track: Track,
    pub index: Option<usize>,
    pub show_artwork: bool,
    pub is_favorite: bool,
    pub in_library: bool,
    pub actions: Vec<PageActionWire>,
    pub playlist_context: Option<(MediaRef, usize)>,
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
}

#[derive(Debug)]
pub enum TrackRowInput {
    PlayClicked,
    FavoriteToggled,
    MenuAction(ActionMenuCommand),
    SetFavorite(bool),
    MediaState(malus_model::AccountMediaState),
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
            gtk::Picture {
                set_can_shrink: true,
                set_content_fit: gtk::ContentFit::Cover,
                set_size_request: (40, 40),
                set_valign: gtk::Align::Center,
                add_css_class: "track-artwork",
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
            gtk::Button {
                add_css_class: "flat",
                add_css_class: "track-action-btn",
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
        };

        let widgets = view_output!();

        if init.show_artwork {
            let url = init.track.artwork.as_ref().map(|a| a.url.clone());
            bind_artwork(&widgets.art_picture, &init.artwork_service, url, 96);
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
            move |cmd| {
                s.input(TrackRowInput::MenuAction(cmd));
            },
        );
        widgets.menu_btn.set_popover(Some(&popover));

        // Click / double-click gesture to play
        let s_click = sender.clone();
        let gesture = gtk::GestureClick::new();
        gesture.connect_released(move |_g, n_press, _x, _y| {
            if n_press >= 1 {
                s_click.input(TrackRowInput::PlayClicked);
            }
        });
        root.add_controller(gesture);
        root.set_focusable(true);
        root.update_property(&[gtk::accessible::Property::Label(&format!(
            "Play {}",
            model.track.title
        ))]);
        let key = gtk::EventControllerKey::new();
        key.connect_key_pressed(move |_, key, _, _| {
            if key == gtk::gdk::Key::Return || key == gtk::gdk::Key::space {
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
                let action = if self.is_favorite {
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
            },
            TrackRowInput::MediaState(state) => {
                if state.reference != self.track.id {
                    return;
                }
                self.is_favorite = state.favorite;
                self.in_library = state.in_library;
            }
            TrackRowInput::SetFavorite(fav) => {
                self.is_favorite = fav;
            }
        }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        message: Self::Input,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
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
            move |cmd| {
                s.input(TrackRowInput::MenuAction(cmd));
            },
        );
        widgets.menu_btn.set_popover(Some(&popover));
    }
}
