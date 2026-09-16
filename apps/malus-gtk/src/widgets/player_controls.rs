//! Shared player widgets. Metadata and transport read the one application state.
use super::{
    actions_menu::{ActionMenuCommand, build_action_popover},
    interactive_scale::InteractiveScale,
};
use crate::{
    design::tokens::*,
    model::{format_remaining_time, format_time},
    state::{PlayerCommand, SharedPlayer},
};
use malus_ipc::wire::PageActionWire;
use malus_model::{MediaRef, RepeatMode};
use relm4::gtk::{self, prelude::*};
use std::{cell::RefCell, rc::Rc};

pub type CommandHandler = Rc<dyn Fn(PlayerCommand)>;
pub type MenuHandler = Rc<dyn Fn(ActionMenuCommand)>;

pub fn icon_button(icon: &str, tooltip: &str, class: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon);
    button.add_css_class("flat");
    button.add_css_class(class);
    button.set_tooltip_text(Some(tooltip));
    button.update_property(&[gtk::accessible::Property::Label(tooltip)]);
    button
}

pub struct Transport {
    pub root: gtk::Box,
    play: gtk::Button,
    shuffle: gtk::Button,
    repeat: gtk::Button,
}
impl Transport {
    pub fn new(player: &SharedPlayer, send: &CommandHandler, immersive: bool) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, if immersive { 18 } else { 8 });
        root.set_halign(gtk::Align::Center);
        root.set_valign(gtk::Align::Center);
        let class = if immersive {
            "np-control"
        } else {
            "player-icon-btn"
        };
        let shuffle = icon_button(ICON_SHUFFLE, "Shuffle", class);
        let previous = icon_button(ICON_PREVIOUS, "Previous", class);
        let play = icon_button(
            ICON_PLAY,
            "Play / Pause",
            if immersive {
                "np-play"
            } else {
                "player-play-btn"
            },
        );
        let next = icon_button(ICON_NEXT, "Next", class);
        let repeat = icon_button(ICON_REPEAT, "Repeat", class);
        for (button, command) in [
            (&previous, PlayerCommand::Previous),
            (&play, PlayerCommand::TogglePlay),
            (&next, PlayerCommand::Next),
        ] {
            let send = send.clone();
            button.connect_clicked(move |_| send(command.clone()));
        }
        let p = player.clone();
        let cb = send.clone();
        shuffle.connect_clicked(move |_| cb(PlayerCommand::Shuffle(!p.borrow().now.shuffle)));
        let p = player.clone();
        let cb = send.clone();
        repeat.connect_clicked(move |_| cb(PlayerCommand::Repeat(p.borrow().now.repeat.cycle())));
        shuffle.set_visible(immersive);
        repeat.set_visible(immersive);
        for b in [&shuffle, &previous, &play, &next, &repeat] {
            root.append(b);
        }
        Self {
            root,
            play,
            shuffle,
            repeat,
        }
    }
    pub fn refresh(&self, player: &SharedPlayer) {
        let state = player.borrow();
        self.root.set_sensitive(state.now.current_track.is_some());
        self.play.set_icon_name(if state.now.is_playing() {
            ICON_PAUSE
        } else {
            ICON_PLAY
        });
        self.play
            .update_property(&[gtk::accessible::Property::Label(
                if state.now.is_playing() {
                    "Pause"
                } else {
                    "Play"
                },
            )]);
        self.play.set_tooltip_text(Some(if state.now.is_playing() {
            "Pause"
        } else {
            "Play"
        }));
        self.shuffle.set_css_classes(&[
            "flat",
            "np-control",
            if state.now.shuffle {
                "control-active"
            } else {
                "control-inactive"
            },
        ]);
        let repeat_label = match state.now.repeat {
            RepeatMode::Off => "Repeat Off",
            RepeatMode::Track => "Repeat One",
            RepeatMode::All => "Repeat All",
        };
        self.repeat.set_tooltip_text(Some(repeat_label));
        self.repeat
            .update_property(&[gtk::accessible::Property::Label(repeat_label)]);
        self.shuffle
            .update_property(&[gtk::accessible::Property::Label(if state.now.shuffle {
                "Turn Shuffle Off"
            } else {
                "Turn Shuffle On"
            })]);
        self.repeat
            .set_icon_name(if state.now.repeat == RepeatMode::Track {
                "media-playlist-repeat-song-symbolic"
            } else {
                ICON_REPEAT
            });
        self.repeat.set_css_classes(&[
            "flat",
            "np-control",
            if state.now.repeat != RepeatMode::Off {
                "control-active"
            } else {
                "control-inactive"
            },
        ]);
    }
}

pub struct SeekControl {
    pub root: gtk::Box,
    pub slider: InteractiveScale,
    elapsed: gtk::Label,
    remaining: gtk::Label,
    track: RefCell<Option<MediaRef>>,
}
impl SeekControl {
    pub fn new(player: &SharedPlayer, send: &CommandHandler, immersive: bool) -> Self {
        let elapsed = gtk::Label::new(Some("0:00"));
        let remaining = gtk::Label::new(Some("-0:00"));
        for label in [&elapsed, &remaining] {
            label.add_css_class("seek-time-label");
            label.set_width_chars(7);
        }
        elapsed.set_xalign(0.0);
        remaining.set_xalign(1.0);
        let left = elapsed.clone();
        let right = remaining.clone();
        let p = player.clone();
        let cb = send.clone();
        let state = player.clone();
        let slider = InteractiveScale::new(
            1.0,
            1000.0,
            false,
            move |value| {
                left.set_text(&format_time(value as u64));
                right.set_text(&format_remaining_time(
                    value as u64,
                    p.borrow().now.duration_ms,
                ));
            },
            move |value| {
                if let Some(track) = &state.borrow().now.current_track {
                    cb(PlayerCommand::Seek {
                        track: track.id.clone(),
                        position_ms: value as u64,
                    });
                }
            },
        );
        slider.widget.add_css_class("seek-bar");
        slider.widget.set_tooltip_text(Some("Seek"));
        slider
            .widget
            .update_property(&[gtk::accessible::Property::Label("Seek")]);
        let root = gtk::Box::new(
            if immersive {
                gtk::Orientation::Vertical
            } else {
                gtk::Orientation::Horizontal
            },
            if immersive { 0 } else { 8 },
        );
        root.set_hexpand(immersive);
        if immersive {
            root.append(&slider.widget);
            let times = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            elapsed.set_hexpand(true);
            times.append(&elapsed);
            times.append(&remaining);
            root.append(&times);
        } else {
            root.append(&elapsed);
            root.append(&slider.widget);
            root.append(&remaining);
        }
        Self {
            root,
            slider,
            elapsed,
            remaining,
            track: RefCell::new(None),
        }
    }
    pub fn refresh(&self, player: &SharedPlayer) {
        let p = player.borrow();
        let id = p.now.current_track.as_ref().map(|t| t.id.clone());
        if *self.track.borrow() != id {
            self.slider.cancel();
            *self.track.borrow_mut() = id;
        }
        let duration = p.now.duration_ms;
        self.slider
            .widget
            .set_sensitive(duration > 0 && p.now.current_track.is_some());
        let value = self.slider.sync(
            p.now.extrapolated_position_ms() as f64,
            duration as f64,
            1500.0,
        ) as u64;
        self.elapsed.set_text(&format_time(value));
        self.remaining.set_text(&if duration == 0 {
            "--:--".to_string()
        } else {
            format_remaining_time(value, duration)
        });
    }
}

pub struct VolumeControl {
    pub root: gtk::Box,
    pub slider: InteractiveScale,
    icon: gtk::Image,
    fixed_endpoints: bool,
}
impl VolumeControl {
    pub fn new(send: &CommandHandler, width: i32) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        root.set_valign(gtk::Align::Center);
        let icon = gtk::Image::from_icon_name(ICON_VOLUME_HIGH);
        icon.set_pixel_size(16);
        let cb = send.clone();
        let slider = InteractiveScale::new(
            1.0,
            0.01,
            true,
            |_| {},
            move |v| cb(PlayerCommand::Volume((v * 100.0).round() as u8)),
        );
        slider.widget.set_hexpand(false);
        slider.widget.set_width_request(width);
        slider.widget.add_css_class("volume-slider");
        slider.widget.set_tooltip_text(Some("Volume"));
        slider
            .widget
            .update_property(&[gtk::accessible::Property::Label("Volume")]);
        root.set_hexpand(false);
        root.append(&icon);
        root.append(&slider.widget);
        Self {
            root,
            slider,
            icon,
            fixed_endpoints: false,
        }
    }
    /// Wide volume with speaker icons on both sides, expanding to fill width.
    /// Used in the compact Now Playing layout.
    pub fn new_wide(send: &CommandHandler) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        root.set_valign(gtk::Align::Center);
        let icon_low = gtk::Image::from_icon_name(ICON_VOLUME_MUTED);
        icon_low.set_pixel_size(16);
        icon_low.add_css_class("np-volume-icon");
        let icon = gtk::Image::from_icon_name(ICON_VOLUME_HIGH);
        icon.set_pixel_size(16);
        icon.add_css_class("np-volume-icon");
        let cb = send.clone();
        let slider = InteractiveScale::new(
            1.0,
            0.01,
            true,
            |_| {},
            move |v| cb(PlayerCommand::Volume((v * 100.0).round() as u8)),
        );
        slider.widget.set_hexpand(true);
        slider.widget.add_css_class("volume-slider");
        slider.widget.set_tooltip_text(Some("Volume"));
        slider
            .widget
            .update_property(&[gtk::accessible::Property::Label("Volume")]);
        root.set_hexpand(true);
        root.append(&icon_low);
        root.append(&slider.widget);
        root.append(&icon);
        Self {
            root,
            slider,
            icon,
            fixed_endpoints: true,
        }
    }
    pub fn refresh(&self, player: &SharedPlayer) {
        let p = player.borrow();
        self.root.set_sensitive(p.now.current_track.is_some());
        let v = self
            .slider
            .sync(f64::from(p.now.volume) / 100.0, 1.0, 0.011);
        if self.fixed_endpoints {
            return;
        }
        self.icon.set_icon_name(Some(if v == 0.0 {
            ICON_VOLUME_MUTED
        } else if v < 0.34 {
            ICON_VOLUME_LOW
        } else if v < 0.67 {
            ICON_VOLUME_MED
        } else {
            ICON_VOLUME_HIGH
        }));
    }
}

pub fn favorite_button(player: &SharedPlayer, on_menu: &MenuHandler) -> gtk::Button {
    let button = icon_button(ICON_FAVORITE_OUTLINE, "Favorite", "player-favorite-btn");
    let p = player.clone();
    let cb = on_menu.clone();
    button.connect_clicked(move |_| {
        let state = p.borrow();
        if let Some(track) = &state.now.current_track {
            cb(ActionMenuCommand::Action(if state.now.is_favorite {
                PageActionWire::Unfavorite(track.id.clone())
            } else {
                PageActionWire::Favorite(track.id.clone())
            }));
        }
    });
    button
}
pub fn refresh_favorite(button: &gtk::Button, player: &SharedPlayer) {
    let p = player.borrow();
    button.set_sensitive(p.now.current_track.is_some());
    if p.now.is_favorite {
        button.set_icon_name(ICON_FAVORITE);
        button.add_css_class("favorite-active");
    } else {
        button.set_icon_name(ICON_FAVORITE_OUTLINE);
        button.remove_css_class("favorite-active");
    }
    button.update_property(&[gtk::accessible::Property::Label(if p.now.is_favorite {
        "Unfavorite"
    } else {
        "Favorite"
    })]);
    button.set_tooltip_text(Some(if p.now.is_favorite {
        "Unfavorite"
    } else {
        "Favorite"
    }));
}
pub fn more_button(player: &SharedPlayer, on_menu: &MenuHandler) -> gtk::MenuButton {
    let button = gtk::MenuButton::new();
    button.set_icon_name(ICON_MORE);
    button.add_css_class("flat");
    button.add_css_class("np-control");
    button.set_tooltip_text(Some("More actions"));
    button.update_property(&[gtk::accessible::Property::Label("More actions")]);
    let p = player.clone();
    let cb = on_menu.clone();
    button.set_create_popup_func(move |button| {
        let state = p.borrow();
        if let Some(t) = &state.now.current_track {
            let cb = cb.clone();
            button.set_popover(Some(&build_action_popover(
                &t.id,
                state.now.is_favorite,
                state.now.in_library,
                true,
                move |a| cb(a),
            )));
        }
    });
    button
}
