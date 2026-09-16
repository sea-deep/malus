//! The pane and immersive page render the same application-owned LyricsState.
//! Rows are built once per lyrics response, and measured after GTK allocation.
use crate::{
    state::{PlayerCommand, SharedPlayer},
    widgets::player_controls::{CommandHandler, icon_button},
};
use relm4::{
    adw::{self, prelude::*},
    gtk::{self},
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};

pub struct LyricsView {
    pub root: gtk::ScrolledWindow,
    content: gtk::Box,
    rows: Rc<RefCell<Vec<gtk::Widget>>>,
    rendered: Cell<(u64, bool)>,
    active: Rc<Cell<Option<usize>>>,
    center_pending: Rc<Cell<bool>>,
    player: SharedPlayer,
    send: CommandHandler,
}
impl LyricsView {
    pub fn new(player: &SharedPlayer, send: &CommandHandler, immersive: bool) -> Self {
        let root = gtk::ScrolledWindow::new();
        root.set_hscrollbar_policy(gtk::PolicyType::Never);
        root.set_vscrollbar_policy(gtk::PolicyType::Automatic);
        root.set_hexpand(true);
        root.set_vexpand(true);
        root.add_css_class(if immersive {
            "immersive-lyrics"
        } else {
            "pane-lyrics"
        });
        let content = gtk::Box::new(gtk::Orientation::Vertical, if immersive { 32 } else { 16 });
        content.set_margin_start(if immersive { 12 } else { 24 });
        content.set_margin_end(24);
        root.set_child(Some(&content));
        let rows = Rc::new(RefCell::new(Vec::<gtk::Widget>::new()));
        let active = Rc::new(Cell::new(None::<usize>));
        let center_pending = Rc::new(Cell::new(true));
        let pause_until = Rc::new(Cell::new(Instant::now()));
        let animation: Rc<RefCell<Option<adw::TimedAnimation>>> = Rc::new(RefCell::new(None));
        let controller = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        let pause = pause_until.clone();
        let scroll_animation = animation.clone();
        controller.connect_scroll(move |_, _, _| {
            if let Some(animation) = scroll_animation.borrow_mut().take() {
                animation.pause();
            }
            pause.set(Instant::now() + Duration::from_secs(3));
            gtk::glib::Propagation::Proceed
        });
        root.add_controller(controller);
        // Capture clicks and touches during user interaction
        let pointer_down = Rc::new(Cell::new(false));
        let gesture = gtk::GestureClick::new();
        gesture.set_propagation_phase(gtk::PropagationPhase::Capture);
        let p1 = pointer_down.clone();
        let drag_animation = animation.clone();
        gesture.connect_pressed(move |_, _, _, _| {
            p1.set(true);
            if let Some(animation) = drag_animation.borrow_mut().take() {
                animation.pause();
            }
        });
        let p2 = pointer_down.clone();
        let pause1 = pause_until.clone();
        gesture.connect_released(move |_, _, _, _| {
            p2.set(false);
            pause1.set(Instant::now() + Duration::from_secs(3));
        });
        let p3 = pointer_down.clone();
        let pause2 = pause_until.clone();
        gesture.connect_cancel(move |_, _| {
            p3.set(false);
            pause2.set(Instant::now() + Duration::from_secs(3));
        });
        root.add_controller(gesture);
        let pointer = pointer_down.clone();
        root.connect_unmap(move |_| pointer.set(false));
        let key = gtk::EventControllerKey::new();
        let pause = pause_until.clone();
        let key_animation = animation.clone();
        key.connect_key_pressed(move |_, _, _, _| {
            pause.set(Instant::now() + Duration::from_secs(3));
            if let Some(animation) = key_animation.borrow_mut().take() {
                animation.pause();
            }
            gtk::glib::Propagation::Proceed
        });
        root.add_controller(key);
        let rs = rows.clone();
        let a = active.clone();
        let pending = center_pending.clone();
        let c = content.clone();
        let last_geometry = Cell::new((0, false));
        let state = player.clone();
        root.add_tick_callback(move |scrolled, _| {
            let height = scrolled.height();
            let synced = state
                .borrow()
                .lyrics
                .content
                .as_ref()
                .is_some_and(|lyrics| lyrics.synced);
            if last_geometry.replace((height, synced)) != (height, synced) {
                c.set_margin_top(if synced {
                    (height as f64 * 0.35) as i32
                } else {
                    24
                });
                c.set_margin_bottom(if synced {
                    (height as f64 * 0.45) as i32
                } else {
                    24
                });
                pending.set(true);
            }
            if pending.get() && !pointer_down.get() && Instant::now() >= pause_until.get() {
                if let Some(index) = a.get() {
                    if let Some(row) = rs.borrow().get(index)
                        && row.height() > 0
                        && let Some(bounds) = row.compute_bounds(&c)
                    {
                        let adjustment = scrolled.vadjustment();
                        // Bounds include real wrapped line heights. Arithmetic
                        // based on row index cannot center multiline lyrics.
                        let y = f64::from(bounds.y())
                            + f64::from(c.margin_top())
                            + f64::from(bounds.height()) * 0.5;
                        let target = (y - adjustment.page_size() * 0.40)
                            .clamp(0.0, (adjustment.upper() - adjustment.page_size()).max(0.0));
                        if let Some(old) = animation.borrow_mut().take() {
                            old.pause();
                        }
                        let target_object = adw::PropertyAnimationTarget::new(&adjustment, "value");
                        let next = adw::TimedAnimation::new(
                            scrolled,
                            adjustment.value(),
                            target,
                            280,
                            target_object,
                        );
                        next.set_easing(adw::Easing::EaseOutCubic);
                        next.play();
                        *animation.borrow_mut() = Some(next);
                        pending.set(false);
                    }
                } else {
                    pending.set(false);
                }
            }
            gtk::glib::ControlFlow::Continue
        });
        let pending = center_pending.clone();
        root.connect_map(move |_| pending.set(true));
        Self {
            root,
            content,
            rows,
            rendered: Cell::new((u64::MAX, false)),
            active,
            center_pending,
            player: player.clone(),
            send: send.clone(),
        }
    }
    pub fn refresh(&self) {
        let state = self.player.borrow();
        let lyrics = &state.lyrics;
        let key = (lyrics.generation, lyrics.loading);
        let rebuilt = self.rendered.replace(key) != key;
        if rebuilt {
            while let Some(child) = self.content.first_child() {
                self.content.remove(&child);
            }
            self.rows.borrow_mut().clear();
            match &lyrics.content {
                Some(text) if !text.lines.is_empty() => {
                    for line in &text.lines {
                        let label = gtk::Label::new(Some(&line.text));
                        label.set_xalign(0.0);
                        label.set_wrap(true);
                        label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
                        label.set_hexpand(true);
                        label.set_max_width_chars(28);
                        let row: gtk::Widget = if text.synced
                            && let Some(start) = line.start_ms
                            && let Some(track) = &state.now.current_track
                        {
                            let button = gtk::Button::new();
                            button.add_css_class("flat");
                            button.set_child(Some(&label));
                            let send = self.send.clone();
                            let id = track.id.clone();
                            button.connect_clicked(move |_| {
                                send(PlayerCommand::Seek {
                                    track: id.clone(),
                                    position_ms: start,
                                })
                            });
                            button.upcast()
                        } else {
                            label.upcast()
                        };
                        row.add_css_class("lyric-line");
                        if !text.synced {
                            row.add_css_class("lyric-unsynced");
                        }
                        self.content.append(&row);
                        self.rows.borrow_mut().push(row);
                    }
                }
                _ => {
                    let status = if lyrics.loading {
                        "Loading lyrics…"
                    } else if lyrics.error.is_some() {
                        "Lyrics couldn’t be loaded"
                    } else if state.now.current_track.is_none() {
                        "Play a song to see its lyrics"
                    } else {
                        "Lyrics aren’t available for this song"
                    };
                    let label = gtk::Label::new(Some(status));
                    label.set_wrap(true);
                    label.add_css_class("lyrics-empty");
                    self.content.append(&label);
                }
            }
        }
        let old = self.active.replace(lyrics.active);
        if rebuilt || old != lyrics.active {
            let rows = self.rows.borrow();
            for (index, row) in rows.iter().enumerate() {
                // Remove all distance classes first
                const DISTANCE_CLASSES: [&str; 4] = [
                    "lyric-dist-1",
                    "lyric-dist-2",
                    "lyric-dist-3",
                    "lyric-dist-4",
                ];
                for class in DISTANCE_CLASSES {
                    row.remove_css_class(class);
                }
                if Some(index) == lyrics.active {
                    row.add_css_class("lyric-active");
                    row.remove_css_class("lyric-past");
                } else {
                    row.remove_css_class("lyric-active");
                    if lyrics.active.is_some_and(|active| index < active) {
                        row.add_css_class("lyric-past");
                    } else {
                        row.remove_css_class("lyric-past");
                    }
                    // Progressive distance classes for fade effect
                    if let Some(active) = lyrics.active {
                        let dist = index.abs_diff(active);
                        let class = dist.min(4);
                        if class >= 1 {
                            row.add_css_class(DISTANCE_CLASSES[class - 1]);
                        }
                    }
                }
            }
            self.center_pending.set(true);
        }
    }
    pub fn reveal_current(&self) {
        self.refresh();
        self.center_pending.set(true);
    }
}

pub struct LyricsPane {
    pub root: gtk::Box,
    pub view: LyricsView,
}
impl LyricsPane {
    pub fn new(
        player: &SharedPlayer,
        send: &CommandHandler,
        expand: impl Fn() + 'static,
        close: impl Fn() + 'static,
    ) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("utility-pane");
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        header.set_margin_start(16);
        header.set_margin_end(16);
        header.set_margin_top(12);
        header.set_margin_bottom(12);
        let title = gtk::Label::new(Some("Lyrics"));
        title.set_xalign(0.0);
        title.set_hexpand(true);
        title.add_css_class("utility-title");
        let expand_btn = icon_button(
            "view-fullscreen-symbolic",
            "Expand Lyrics",
            "player-icon-btn",
        );
        expand_btn.connect_clicked(move |_| expand());
        let close_btn = icon_button("window-close-symbolic", "Close (Esc)", "player-icon-btn");
        close_btn.connect_clicked(move |_| close());
        header.append(&title);
        header.append(&expand_btn);
        header.append(&close_btn);
        root.append(&header);
        let view = LyricsView::new(player, send, false);
        root.append(&view.root);
        Self { root, view }
    }
}
