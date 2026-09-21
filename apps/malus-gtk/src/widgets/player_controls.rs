//! Shared player widgets. Metadata and transport read the one application state.
use super::{
    actions_menu::{ActionMenuCommand, build_action_popover_full},
    interactive_scale::InteractiveScale,
};
use crate::{
    design::tokens::*,
    model::{format_remaining_time, format_time},
    state::{PlayerCommand, SharedPlayer},
};
use malus_ipc::wire::PageActionWire;
use malus_model::{MediaRef, PageRoute, PlaybackState, RepeatMode};
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
    button.set_focus_on_click(false);
    button
}

pub struct Transport {
    pub root: gtk::Box,
    pub play: gtk::Button,
    play_stack: gtk::Stack,
    play_icon: gtk::Image,
    pub shuffle: gtk::Button,
    pub repeat: gtk::Button,
    immersive: bool,
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

        let play = gtk::Button::new();
        play.add_css_class("flat");
        play.add_css_class(if immersive {
            "np-play"
        } else {
            "player-play-btn"
        });
        play.set_focus_on_click(false);
        play.set_tooltip_text(Some("Play / Pause"));
        play.update_property(&[gtk::accessible::Property::Label("Play / Pause")]);

        let play_stack = gtk::Stack::new();
        play_stack.set_transition_type(gtk::StackTransitionType::Crossfade);
        play_stack.set_transition_duration(150);

        let play_icon = gtk::Image::from_icon_name(ICON_PLAY);
        if immersive {
            play_icon.set_pixel_size(36);
        } else {
            play_icon.set_pixel_size(22);
        }

        let spinner = adw::Spinner::new();
        if immersive {
            spinner.set_size_request(36, 36);
        } else {
            spinner.set_size_request(22, 22);
        }

        play_stack.add_named(&play_icon, Some("icon"));
        play_stack.add_named(&spinner, Some("spinner"));
        play_stack.set_visible_child_name("icon");
        play.set_child(Some(&play_stack));

        let next = icon_button(ICON_NEXT, "Next", class);
        let repeat = icon_button(ICON_REPEAT, "Repeat", class);

        for (button, command) in [
            (&previous, PlayerCommand::Previous),
            (&play, PlayerCommand::TogglePlay),
            (&next, PlayerCommand::Next),
        ] {
            let cb = send.clone();
            button.connect_clicked(move |_| cb(command.clone()));
        }

        let p = player.clone();
        let cb = send.clone();
        shuffle.connect_clicked(move |_| cb(PlayerCommand::Shuffle(!p.borrow().now.shuffle)));
        let p = player.clone();
        let cb = send.clone();
        repeat.connect_clicked(move |_| cb(PlayerCommand::Repeat(p.borrow().now.repeat.cycle())));
        shuffle.set_visible(true);
        repeat.set_visible(true);
        for b in [&shuffle, &previous, &play, &next, &repeat] {
            root.append(b);
        }
        Self {
            root,
            play,
            play_stack,
            play_icon,
            shuffle,
            repeat,
            immersive,
        }
    }

    pub fn refresh(&self, player: &SharedPlayer) {
        let state = player.borrow();
        self.root
            .set_sensitive(state.now.current_track.is_some() || state.now.is_changing_track);

        if state.now.is_changing_track {
            self.play_stack.set_visible_child_name("spinner");
            self.play
                .update_property(&[gtk::accessible::Property::Label("Loading")]);
            self.play.set_tooltip_text(Some("Loading"));
        } else {
            self.play_stack.set_visible_child_name("icon");
            let is_playing = state.now.is_playing();
            self.play_icon
                .set_icon_name(Some(if is_playing { ICON_PAUSE } else { ICON_PLAY }));
            let label = if is_playing { "Pause" } else { "Play" };
            self.play
                .update_property(&[gtk::accessible::Property::Label(label)]);
            self.play.set_tooltip_text(Some(label));
        }
        let base_class = if self.immersive {
            "np-control"
        } else {
            "player-icon-btn"
        };
        self.shuffle.set_css_classes(&[
            "flat",
            base_class,
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
            base_class,
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
                let p = state.borrow();
                if p.now.is_live() {
                    return;
                }
                if let Some(track) = &p.now.current_track {
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
        root.set_hexpand(true);
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
        let is_live = p.now.is_live();
        let duration = p.now.duration_ms;
        let is_stopped = p.now.playback_state == PlaybackState::Stopped;
        self.slider.widget.set_sensitive(
            !is_stopped && !is_live && duration > 0 && p.now.current_track.is_some(),
        );
        if is_live {
            self.elapsed.set_text("LIVE");
            self.remaining.set_text("");
            self.slider.sync(0.0, 1.0, 1500.0);
        } else {
            let pos = if is_stopped {
                0
            } else {
                p.now.extrapolated_position_ms()
            };
            let value = self.slider.sync(pos as f64, duration as f64, 1500.0) as u64;
            self.elapsed.set_text(&format_time(value));
            self.remaining.set_text(&if duration == 0 {
                "--:--".to_string()
            } else {
                format_remaining_time(value, duration)
            });
        }
    }
}

pub struct VolumeControl {
    pub root: gtk::Box,
    pub slider: InteractiveScale,
    icon: gtk::Image,
    fixed_endpoints: bool,
    _popover: Option<gtk::Popover>,
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
            _popover: None,
        }
    }
    /// Compact volume icon button with a floating popover mini slider and scroll-wheel control.
    pub fn new_popover(send: &CommandHandler) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        root.set_valign(gtk::Align::Center);
        let icon = gtk::Image::from_icon_name(ICON_VOLUME_HIGH);
        icon.set_pixel_size(ICON_GLYPH_SM);

        let button = gtk::MenuButton::new();
        button.add_css_class("flat");
        button.add_css_class("player-icon-btn");
        button.set_tooltip_text(Some("Volume"));
        button.update_property(&[gtk::accessible::Property::Label("Volume")]);
        button.set_focus_on_click(false);
        button.set_always_show_arrow(false);
        button.set_direction(gtk::ArrowType::Up);
        button.set_child(Some(&icon));

        let popover = gtk::Popover::new();
        popover.set_position(gtk::PositionType::Top);
        popover.set_autohide(true);
        popover.add_css_class("volume-popover");

        let popover_box = gtk::Box::new(gtk::Orientation::Horizontal, SPACING_XS);
        popover_box.add_css_class("volume-popover-content");
        popover_box.set_margin_top(VOLUME_POPOVER_PADDING_V);
        popover_box.set_margin_bottom(VOLUME_POPOVER_PADDING_V);
        popover_box.set_margin_start(VOLUME_POPOVER_PADDING_H);
        popover_box.set_margin_end(VOLUME_POPOVER_PADDING_H);

        let icon_low = gtk::Image::from_icon_name(ICON_VOLUME_MUTED);
        icon_low.set_pixel_size(ICON_GLYPH_SM);
        icon_low.add_css_class("volume-popover-icon");

        let icon_high = gtk::Image::from_icon_name(ICON_VOLUME_HIGH);
        icon_high.set_pixel_size(ICON_GLYPH_SM);
        icon_high.add_css_class("volume-popover-icon");

        let cb = send.clone();
        let icon_update = icon.clone();
        let slider = InteractiveScale::new(
            1.0,
            0.01,
            true,
            move |v| {
                icon_update.set_icon_name(Some(if v == 0.0 {
                    ICON_VOLUME_MUTED
                } else if v < 0.34 {
                    ICON_VOLUME_LOW
                } else if v < 0.67 {
                    ICON_VOLUME_MED
                } else {
                    ICON_VOLUME_HIGH
                }));
            },
            move |v| cb(PlayerCommand::Volume((v * 100.0).round() as u8)),
        );
        slider.widget.set_hexpand(false);
        slider.widget.set_width_request(VOLUME_POPOVER_SLIDER_WIDTH);
        slider.widget.add_css_class("volume-slider");
        slider.widget.set_tooltip_text(Some("Volume"));
        slider
            .widget
            .update_property(&[gtk::accessible::Property::Label("Volume")]);

        popover_box.append(&icon_low);
        popover_box.append(&slider.widget);
        popover_box.append(&icon_high);
        popover.set_child(Some(&popover_box));
        button.set_popover(Some(&popover));

        // Scroll controller for mouse wheel adjustment on the volume button
        let slider_scroll_btn = slider.clone();
        let scroll_btn = gtk::EventControllerScroll::new(
            gtk::EventControllerScrollFlags::VERTICAL | gtk::EventControllerScrollFlags::HORIZONTAL,
        );
        scroll_btn.connect_scroll(move |_, dx, dy| {
            let delta = if dy != 0.0 { -dy } else { dx };
            let step = if delta > 0.0 {
                (delta * 0.04).max(0.02)
            } else if delta < 0.0 {
                (delta * 0.04).min(-0.02)
            } else {
                0.0
            };
            if step != 0.0 {
                let current = slider_scroll_btn.value();
                let new_value = (current + step).clamp(0.0, 1.0);
                slider_scroll_btn.set_value(new_value);
            }
            gtk::glib::Propagation::Stop
        });
        button.add_controller(scroll_btn);

        // Scroll controller inside the popover box
        let slider_scroll_pop = slider.clone();
        let scroll_pop = gtk::EventControllerScroll::new(
            gtk::EventControllerScrollFlags::VERTICAL | gtk::EventControllerScrollFlags::HORIZONTAL,
        );
        scroll_pop.connect_scroll(move |_, dx, dy| {
            let delta = if dy != 0.0 { -dy } else { dx };
            let step = if delta > 0.0 {
                (delta * 0.04).max(0.02)
            } else if delta < 0.0 {
                (delta * 0.04).min(-0.02)
            } else {
                0.0
            };
            if step != 0.0 {
                let current = slider_scroll_pop.value();
                let new_value = (current + step).clamp(0.0, 1.0);
                slider_scroll_pop.set_value(new_value);
            }
            gtk::glib::Propagation::Stop
        });
        popover_box.add_controller(scroll_pop);

        root.append(&button);

        Self {
            root,
            slider,
            icon,
            fixed_endpoints: false,
            _popover: Some(popover),
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
            _popover: None,
        }
    }
    pub fn refresh(&self, player: &SharedPlayer) {
        let p = player.borrow();
        if self._popover.is_none() {
            self.root.set_sensitive(p.now.current_track.is_some());
        }
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
        button.add_css_class("control-active");
        button.remove_css_class("control-inactive");
    } else {
        button.set_icon_name(ICON_FAVORITE_OUTLINE);
        button.remove_css_class("favorite-active");
        button.remove_css_class("control-active");
        button.add_css_class("control-inactive");
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
    button.add_css_class("player-icon-btn");
    button.add_css_class("player-more-btn");
    button.add_css_class("np-control");
    button.set_focus_on_click(false);
    button.set_always_show_arrow(false);
    button.set_direction(gtk::ArrowType::Up);
    button.set_tooltip_text(Some("More actions"));
    button.update_property(&[gtk::accessible::Property::Label("More actions")]);
    let p = player.clone();
    let cb = on_menu.clone();
    button.set_create_popup_func(move |button| {
        let state = p.borrow();
        if let Some(t) = &state.now.current_track {
            let cb = cb.clone();
            let album_route = t
                .album
                .as_ref()
                .and_then(|a| a.id.as_ref())
                .map(|r| PageRoute::Album(r.id().to_string()))
                .or_else(|| {
                    t.uri.as_deref().and_then(|u| {
                        let clean = u.split('?').next()?.trim_end_matches('/');
                        let idx = clean.find("/album/")?;
                        let after = &clean[idx + "/album/".len()..];
                        let segs: Vec<&str> = after.split('/').filter(|s| !s.is_empty()).collect();
                        let id = segs.last()?;
                        if id.chars().all(|c| c.is_ascii_digit()) || id.starts_with("l.") {
                            Some(PageRoute::Album(id.to_string()))
                        } else {
                            None
                        }
                    })
                });
            let artist_route = t
                .artists
                .first()
                .and_then(|a| a.id.as_ref())
                .map(|r| PageRoute::Artist(r.id().to_string()));
            let popover = build_action_popover_full(
                &t.id,
                state.now.is_favorite,
                state.now.in_library,
                true,
                &[],
                None,
                album_route,
                artist_route,
                move |a| cb(a),
            );
            popover.set_position(gtk::PositionType::Top);
            button.set_popover(Some(&popover));
        }
    });
    button
}

pub fn popup_track_context_menu(
    parent: &impl IsA<gtk::Widget>,
    x: f64,
    y: f64,
    player: &SharedPlayer,
    on_menu: &MenuHandler,
) {
    let state = player.borrow();
    if let Some(t) = &state.now.current_track {
        let cb = on_menu.clone();
        let album_route = t
            .album
            .as_ref()
            .and_then(|a| a.id.as_ref())
            .map(|r| PageRoute::Album(r.id().to_string()))
            .or_else(|| {
                t.uri.as_deref().and_then(|u| {
                    let clean = u.split('?').next()?.trim_end_matches('/');
                    let idx = clean.find("/album/")?;
                    let after = &clean[idx + "/album/".len()..];
                    let segs: Vec<&str> = after.split('/').filter(|s| !s.is_empty()).collect();
                    let id = segs.last()?;
                    if id.chars().all(|c| c.is_ascii_digit()) || id.starts_with("l.") {
                        Some(PageRoute::Album(id.to_string()))
                    } else {
                        None
                    }
                })
            });
        let artist_route = t
            .artists
            .first()
            .and_then(|a| a.id.as_ref())
            .map(|r| PageRoute::Artist(r.id().to_string()));
        let popover = build_action_popover_full(
            &t.id,
            state.now.is_favorite,
            state.now.in_library,
            true,
            &[],
            None,
            album_route,
            artist_route,
            move |a| cb(a),
        );
        popover.set_parent(parent);
        popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        popover.connect_closed(|p| {
            p.unparent();
        });
        popover.popup();
    }
}
