//! Queue (Queue) utility pane.

use malus_client::MalusClient;
use malus_model::{MediaRef, PlaybackState, Queue, Track};
use relm4::gtk::{self, glib, prelude::*};
use relm4::prelude::*;

use crate::design::tokens::*;
use crate::model::format_time;
use crate::services::{ArtworkService, bind_artwork};
use crate::widgets::square_artwork::SquareArtwork;

type SectionHeaderEntry = (&'static str, Option<gtk::Box>);
type SectionHeadersRef = std::rc::Rc<std::cell::RefCell<Vec<SectionHeaderEntry>>>;

pub struct QueuePane {
    client: MalusClient,
    artwork_service: ArtworkService,
    queue: Queue,
    playback_state: PlaybackState,
    autoplay: bool,
    close: std::rc::Rc<dyn Fn()>,
    immersive: bool,
    current_row: Option<gtk::Box>,
    anchor_target: std::rc::Rc<std::cell::RefCell<Option<gtk::Widget>>>,
    section_headers: SectionHeadersRef,
    needs_rerender: bool,
    needs_scroll_to_now_playing: bool,
}

#[derive(Debug)]
pub enum QueueInput {
    SetQueue(Queue),
    SetPlaybackState(PlaybackState),
    SetAutoplay(bool),
    ToggleAutoplay,
    Reload,
    ScrollToNowPlaying,
    Jump(usize),
    Remove(usize),
    PlayNext(MediaRef),
    PlayLater(MediaRef),
    ClearUpcoming,
    Close,
}

#[derive(Debug)]
pub enum QueueCmd {
    InitLoaded {
        queue: Queue,
        playback_state: PlaybackState,
        autoplay: bool,
    },
    QueueLoaded(Queue),
}

fn update_sticky_header_position(
    vadj: &gtk::Adjustment,
    headers: &[(&'static str, Option<gtk::Box>)],
    content_box: &gtk::Box,
    sticky_box: &gtk::Box,
    sticky_label: &gtk::Label,
) {
    if headers.is_empty() {
        sticky_box.set_visible(false);
        return;
    }

    let scroll_y = vadj.value();
    let mut active_title = headers[0].0;

    for (title, widget_opt) in headers.iter().skip(1) {
        if let Some(widget) = widget_opt
            && let Some(bounds) = widget.compute_bounds(content_box)
        {
            let header_bottom = bounds.y() as f64 + bounds.height() as f64;
            if scroll_y >= header_bottom - 2.0 {
                active_title = *title;
            }
        }
    }

    sticky_label.set_text(active_title);
    sticky_box.set_visible(true);
}

fn perform_scroll_to_target(
    target: &gtk::Widget,
    content_box: &gtk::Box,
    scrolled_window: &gtk::ScrolledWindow,
) {
    let t = target.clone();
    let cb = content_box.clone();
    let sw = scrolled_window.clone();

    glib::idle_add_local_once(move || {
        let vadj = sw.vadjustment();
        let page_size = vadj.page_size();
        if let Some(bounds) = t.compute_bounds(&cb) {
            let target_y = bounds.y() as f64;
            let total_h = cb.height() as f64;

            if page_size > 0.0 && total_h > target_y {
                let after_target = total_h - target_y;
                if page_size > after_target {
                    let pad = (page_size - after_target) as i32 + 32;
                    cb.set_margin_bottom(pad);
                } else {
                    cb.set_margin_bottom(24);
                }
            }

            vadj.set_value(target_y);
            let vadj_tick = vadj.clone();
            glib::idle_add_local_once(move || {
                vadj_tick.set_value(target_y);
            });
        }
    });
}

#[relm4::component(pub)]
impl Component for QueuePane {
    type Init = (MalusClient, ArtworkService, std::rc::Rc<dyn Fn()>, bool);
    type Input = QueueInput;
    type Output = ();
    type CommandOutput = QueueCmd;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,
            add_css_class: "utility-pane",
            set_overflow: gtk::Overflow::Hidden,

            // Pane Header
            #[name(header_box)]
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                add_css_class: "utility-header",
                set_margin_start: 16,
                set_margin_end: 16,
                set_margin_top: 14,
                set_margin_bottom: 14,

                #[name(title_label)]
                gtk::Label {
                    set_xalign: 0.0,
                    set_hexpand: true,
                    add_css_class: "utility-title",
                    set_text: "Queue",
                },

                gtk::Button {
                    add_css_class: "flat",
                    add_css_class: "utility-action-btn",
                    set_focus_on_click: false,
                    set_label: "Clear",
                    #[watch]
                    set_sensitive: model.has_upcoming(),
                    connect_clicked => QueueInput::ClearUpcoming,
                },

                gtk::Button {
                    add_css_class: "flat",
                    add_css_class: "utility-autoplay-btn",
                    set_focus_on_click: false,
                    set_label: "∞",
                    #[watch]
                    set_tooltip_text: Some(if model.autoplay {
                        "Autoplay is on"
                    } else {
                        "Autoplay is off"
                    }),
                    #[watch]
                    set_css_classes: if model.autoplay {
                        &["flat", "utility-autoplay-btn", "control-active"]
                    } else {
                        &["flat", "utility-autoplay-btn", "control-inactive"]
                    },
                    connect_clicked => QueueInput::ToggleAutoplay,
                },

                #[name(close_btn)]
                gtk::Button {
                    add_css_class: "flat",
                    add_css_class: "player-icon-btn",
                    set_focus_on_click: false,
                    set_icon_name: ICON_CLOSE,
                    set_tooltip_text: Some("Close (Esc)"),
                    connect_clicked => QueueInput::Close,
                },
            },

            #[name(header_separator)]
            gtk::Separator {
                set_orientation: gtk::Orientation::Horizontal,
            },

            #[name(sticky_header_box)]
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                add_css_class: "queue-sticky-header",
                set_visible: false,

                #[name(sticky_header_label)]
                gtk::Label {
                    set_xalign: 0.0,
                    set_hexpand: true,
                    add_css_class: "queue-section-label",
                    set_text: "",
                },
            },

            #[name(scrolled_window)]
            gtk::ScrolledWindow {
                set_hscrollbar_policy: gtk::PolicyType::Never,
                set_vscrollbar_policy: gtk::PolicyType::Automatic,
                set_vexpand: true,

                #[name(content_box)]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 0,
                    set_margin_bottom: 24,
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let (client, artwork_service, close, immersive) = init;
        let section_headers: SectionHeadersRef =
            std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let anchor_target: std::rc::Rc<std::cell::RefCell<Option<gtk::Widget>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));

        let model = Self {
            client: client.clone(),
            artwork_service,
            queue: Queue::default(),
            playback_state: PlaybackState::Stopped,
            autoplay: malus_ipc::PlayerPreferences::load().autoplay,
            close,
            immersive,
            current_row: None,
            anchor_target: anchor_target.clone(),
            section_headers: section_headers.clone(),
            needs_rerender: false,
            needs_scroll_to_now_playing: true,
        };

        let widgets = view_output!();

        if immersive {
            root.remove_css_class("utility-pane");
            root.add_css_class("immersive-queue");
            widgets.title_label.set_text("Queue");
            widgets.close_btn.set_visible(false);
            widgets.header_separator.set_visible(false);
            widgets.header_box.set_margin_start(16);
            widgets.header_box.set_margin_end(16);
            widgets.header_box.set_margin_top(8);
            widgets.header_box.set_margin_bottom(8);
        } else {
            widgets.title_label.set_text("Queue");
            widgets.close_btn.set_visible(true);
            widgets.header_separator.set_visible(true);
            widgets.header_box.set_margin_start(16);
            widgets.header_box.set_margin_end(16);
            widgets.header_box.set_margin_top(14);
            widgets.header_box.set_margin_bottom(14);
        }

        // Connect vertical scroll watcher for sticky headers
        let s_headers = section_headers;
        let cb = widgets.content_box.clone();
        let sbox = widgets.sticky_header_box.clone();
        let slabel = widgets.sticky_header_label.clone();
        widgets
            .scrolled_window
            .vadjustment()
            .connect_value_changed(move |adj| {
                update_sticky_header_position(adj, &s_headers.borrow(), &cb, &sbox, &slabel);
            });

        // Whenever the scrolled window is mapped (sidebar opened), re-scroll to anchor
        let anchor = anchor_target;
        let cb_map = widgets.content_box.clone();
        let sw_map = widgets.scrolled_window.clone();
        widgets.scrolled_window.connect_map(move |_| {
            let anchor_ref = anchor.borrow();
            if let Some(ref target) = *anchor_ref {
                perform_scroll_to_target(target, &cb_map, &sw_map);
            }
        });

        // Initial fetch of queue, playback state, and autoplay status
        let c = client.clone();
        sender.oneshot_command(async move {
            let q = c.get_queue().await.unwrap_or_default();
            let status = c.get_status().await.ok();
            QueueCmd::InitLoaded {
                queue: q,
                playback_state: status
                    .as_ref()
                    .map(|s| s.state)
                    .unwrap_or(PlaybackState::Stopped),
                autoplay: status.map(|s| s.autoplay).unwrap_or(false),
            }
        });

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            QueueInput::SetQueue(q) => {
                let track_changed =
                    self.queue.current_track().map(|t| &t.id) != q.current_track().map(|t| &t.id);
                if track_changed {
                    self.needs_scroll_to_now_playing = true;
                }
                if self.queue != q {
                    self.queue = q;
                    self.needs_rerender = true;
                }
            }
            QueueInput::SetPlaybackState(state) => {
                if self.playback_state != state {
                    let old_active = self.playback_state != PlaybackState::Stopped;
                    let new_active = state != PlaybackState::Stopped;
                    self.playback_state = state;
                    if old_active != new_active
                        && let Some(ref row) = self.current_row
                    {
                        if new_active {
                            row.add_css_class("queue-row-current");
                        } else {
                            row.remove_css_class("queue-row-current");
                        }
                    }
                }
            }
            QueueInput::SetAutoplay(autoplay) => {
                if self.autoplay != autoplay {
                    self.autoplay = autoplay;
                    self.needs_rerender = true;
                }
            }
            QueueInput::ToggleAutoplay => {
                let next = !self.autoplay;
                self.autoplay = next;
                self.needs_rerender = true;
                let _ = malus_ipc::PlayerPreferences { autoplay: next }.save();
                let c = self.client.clone();
                sender.oneshot_command(async move {
                    let _ = c.set_autoplay(next).await;
                    let q = c.get_queue().await.unwrap_or_default();
                    QueueCmd::QueueLoaded(q)
                });
            }
            QueueInput::Reload => {
                let c = self.client.clone();
                sender.oneshot_command(async move {
                    let q = c.get_queue().await.unwrap_or_default();
                    QueueCmd::QueueLoaded(q)
                });
            }
            QueueInput::ScrollToNowPlaying => {
                self.needs_scroll_to_now_playing = true;
            }
            QueueInput::Jump(idx) => {
                let expected_id = self.queue.items.get(idx).map(|t| t.id.id().to_string());
                if idx < self.queue.items.len() {
                    self.queue.current_index = Some(idx);
                    self.playback_state = PlaybackState::Playing;
                    self.needs_rerender = true;
                }
                let c = self.client.clone();
                sender.oneshot_command(async move {
                    let _ = c.queue_jump_checked(idx, expected_id).await;
                    let q = c.get_queue().await.unwrap_or_default();
                    QueueCmd::QueueLoaded(q)
                });
            }
            QueueInput::Remove(idx) => {
                let expected_id = self.queue.items.get(idx).map(|t| t.id.id().to_string());
                let c = self.client.clone();
                sender.oneshot_command(async move {
                    let _ = c.queue_remove_checked(idx, expected_id).await;
                    let q = c.get_queue().await.unwrap_or_default();
                    QueueCmd::QueueLoaded(q)
                });
            }
            QueueInput::PlayNext(media_ref) => {
                let c = self.client.clone();
                sender.oneshot_command(async move {
                    let _ = c.play_next(&media_ref).await;
                    let q = c.get_queue().await.unwrap_or_default();
                    QueueCmd::QueueLoaded(q)
                });
            }
            QueueInput::PlayLater(media_ref) => {
                let c = self.client.clone();
                sender.oneshot_command(async move {
                    let _ = c.play_later(&media_ref).await;
                    let q = c.get_queue().await.unwrap_or_default();
                    QueueCmd::QueueLoaded(q)
                });
            }
            QueueInput::ClearUpcoming => {
                let c = self.client.clone();
                sender.oneshot_command(async move {
                    let _ = c.queue_clear_upcoming().await;
                    let q = c.get_queue().await.unwrap_or_default();
                    QueueCmd::QueueLoaded(q)
                });
            }
            QueueInput::Close => {
                (self.close)();
            }
        }
    }

    fn update_cmd(
        &mut self,
        message: Self::CommandOutput,
        _sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match message {
            QueueCmd::InitLoaded {
                queue,
                playback_state,
                autoplay,
            } => {
                self.queue = queue;
                self.playback_state = playback_state;
                self.autoplay = autoplay;
                self.needs_rerender = true;
                self.needs_scroll_to_now_playing = true;
            }
            QueueCmd::QueueLoaded(q) => {
                if self.queue != q {
                    let track_changed = self.queue.current_track().map(|t| &t.id)
                        != q.current_track().map(|t| &t.id);
                    self.queue = q;
                    self.needs_rerender = true;
                    if track_changed {
                        self.needs_scroll_to_now_playing = true;
                    }
                }
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
        self.update(message, sender.clone(), root);
        self.update_view(widgets, sender.clone());
        if std::mem::take(&mut self.needs_rerender) {
            let prev_val = widgets.scrolled_window.vadjustment().value();
            self.render_content(widgets, sender);
            if !self.needs_scroll_to_now_playing && prev_val > 0.0 {
                let vadj = widgets.scrolled_window.vadjustment();
                glib::idle_add_local_once(move || {
                    vadj.set_value(prev_val);
                });
            }
        }
        if std::mem::take(&mut self.needs_scroll_to_now_playing) {
            self.scroll_to_now_playing(widgets);
        }
    }

    fn update_cmd_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        message: Self::CommandOutput,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        self.update_cmd(message, sender.clone(), root);
        self.update_view(widgets, sender.clone());
        if std::mem::take(&mut self.needs_rerender) {
            let prev_val = widgets.scrolled_window.vadjustment().value();
            self.render_content(widgets, sender);
            if !self.needs_scroll_to_now_playing && prev_val > 0.0 {
                let vadj = widgets.scrolled_window.vadjustment();
                glib::idle_add_local_once(move || {
                    vadj.set_value(prev_val);
                });
            }
        }
        if std::mem::take(&mut self.needs_scroll_to_now_playing) {
            self.scroll_to_now_playing(widgets);
        }
    }
}

impl QueuePane {
    fn build_section_header(title: &'static str) -> gtk::Box {
        let header_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .css_classes(vec!["queue-section-header".to_string()])
            .build();

        let title_label = gtk::Label::builder()
            .label(title)
            .xalign(0.0)
            .hexpand(true)
            .css_classes(vec!["queue-section-label".to_string()])
            .build();
        header_box.append(&title_label);
        header_box
    }

    fn scroll_to_now_playing(&self, widgets: &QueuePaneWidgets) {
        if let Some(ref target) = *self.anchor_target.borrow() {
            perform_scroll_to_target(target, &widgets.content_box, &widgets.scrolled_window);
        } else {
            widgets.content_box.set_margin_bottom(24);
            widgets.scrolled_window.vadjustment().set_value(0.0);
        }
    }

    fn render_content(&mut self, widgets: &mut QueuePaneWidgets, sender: ComponentSender<Self>) {
        // Rebuild queue item widgets inside content_box
        while let Some(child) = widgets.content_box.first_child() {
            widgets.content_box.remove(&child);
        }
        self.current_row = None;
        *self.anchor_target.borrow_mut() = None;
        let mut sections_list = Vec::new();

        let items = &self.queue.items;

        if items.is_empty() {
            *self.section_headers.borrow_mut() = Vec::new();
            widgets.sticky_header_box.set_visible(false);

            let empty_label = gtk::Label::builder()
                .label("Queue is empty")
                .css_classes(vec!["utility-empty-label".to_string()])
                .margin_top(32)
                .build();
            widgets.content_box.append(&empty_label);
            return;
        }

        // Determine effective index safely: if current_index is valid, use it;
        // otherwise default to 0 so we always know where playback is.
        let effective_idx = match self.queue.current_index {
            Some(idx) if idx < items.len() => Some(idx),
            _ => Some(0),
        };
        let current_idx_val = effective_idx.unwrap_or(0);

        // 1. History Section (previously played items, in chronological order up to current)
        if current_idx_val > 0 {
            let history_title = "HISTORY";
            let header_widget = if sections_list.is_empty() {
                None
            } else {
                let header = Self::build_section_header(history_title);
                widgets.content_box.append(&header);
                Some(header)
            };
            sections_list.push((history_title, header_widget));

            let history_box = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(2)
                .margin_start(8)
                .margin_end(8)
                .margin_top(4)
                .margin_bottom(12)
                .build();

            for i in 0..current_idx_val {
                if let Some(track) = items.get(i) {
                    let row = self.build_history_row(i, track, sender.clone());
                    history_box.append(&row);
                }
            }

            widgets.content_box.append(&history_box);
        }

        // 2. Now Playing Section (only when NOT in immersive Now Playing mode!)
        if !self.immersive
            && let Some(track) = items.get(current_idx_val)
        {
            let np_title = "NOW PLAYING";
            let header_widget = if sections_list.is_empty() {
                None
            } else {
                let header = Self::build_section_header(np_title);
                widgets.content_box.append(&header);
                Some(header)
            };
            sections_list.push((np_title, header_widget));

            let now_playing_box = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(2)
                .margin_start(8)
                .margin_end(8)
                .margin_top(4)
                .margin_bottom(12)
                .build();

            let row = self.build_current_row(track, sender.clone());
            now_playing_box.append(&row);
            self.current_row = Some(row);

            *self.anchor_target.borrow_mut() = Some(now_playing_box.clone().upcast());
            widgets.content_box.append(&now_playing_box);
        }

        // 3. Upcoming Sections: UP NEXT and/or AUTOPLAY
        let start_idx = current_idx_val + 1;
        let items_len = items.len();
        let is_live = self.queue.current_track().is_some_and(|t| t.is_live());

        if start_idx < items_len {
            // Use same label style as sidebar in all modes
            let up_title = "UP NEXT";
            let auto_title = "AUTOPLAY";

            match self.queue.autoplay_start_index {
                Some(auto_idx) if auto_idx <= start_idx => {
                    // All upcoming items belong to the Autoplay station (no manually-added items)
                    let header_widget = if sections_list.is_empty() {
                        None
                    } else {
                        let header = Self::build_section_header(auto_title);
                        widgets.content_box.append(&header);
                        Some(header)
                    };
                    sections_list.push((auto_title, header_widget));

                    let autoplay_box = gtk::Box::builder()
                        .orientation(gtk::Orientation::Vertical)
                        .spacing(2)
                        .margin_start(8)
                        .margin_end(8)
                        .margin_top(4)
                        .margin_bottom(12)
                        .build();

                    if self.immersive && current_idx_val > 0 {
                        *self.anchor_target.borrow_mut() = Some(autoplay_box.clone().upcast());
                    }

                    for (idx, track) in items.iter().enumerate().skip(start_idx) {
                        let row = self.build_upcoming_row(idx, track, sender.clone());
                        autoplay_box.append(&row);
                    }

                    widgets.content_box.append(&autoplay_box);
                }
                Some(auto_idx) if auto_idx < items_len => {
                    // Manually-added Up Next items followed by Autoplay items
                    let up_header_widget = if sections_list.is_empty() {
                        None
                    } else {
                        let header = Self::build_section_header(up_title);
                        widgets.content_box.append(&header);
                        Some(header)
                    };
                    sections_list.push((up_title, up_header_widget));

                    let up_next_box = gtk::Box::builder()
                        .orientation(gtk::Orientation::Vertical)
                        .spacing(2)
                        .margin_start(8)
                        .margin_end(8)
                        .margin_top(4)
                        .margin_bottom(12)
                        .build();

                    if self.immersive && current_idx_val > 0 {
                        *self.anchor_target.borrow_mut() = Some(up_next_box.clone().upcast());
                    }

                    for (idx, track) in items.iter().enumerate().take(auto_idx).skip(start_idx) {
                        let row = self.build_upcoming_row(idx, track, sender.clone());
                        up_next_box.append(&row);
                    }

                    widgets.content_box.append(&up_next_box);

                    let auto_header = Self::build_section_header(auto_title);
                    widgets.content_box.append(&auto_header);
                    sections_list.push((auto_title, Some(auto_header)));

                    let autoplay_box = gtk::Box::builder()
                        .orientation(gtk::Orientation::Vertical)
                        .spacing(2)
                        .margin_start(8)
                        .margin_end(8)
                        .margin_top(4)
                        .margin_bottom(12)
                        .build();

                    for (idx, track) in items.iter().enumerate().skip(auto_idx) {
                        let row = self.build_upcoming_row(idx, track, sender.clone());
                        autoplay_box.append(&row);
                    }

                    widgets.content_box.append(&autoplay_box);
                }
                _ => {
                    // Standard Up Next (no autoplay items)
                    let header_widget = if sections_list.is_empty() {
                        None
                    } else {
                        let header = Self::build_section_header(up_title);
                        widgets.content_box.append(&header);
                        Some(header)
                    };
                    sections_list.push((up_title, header_widget));

                    let up_next_box = gtk::Box::builder()
                        .orientation(gtk::Orientation::Vertical)
                        .spacing(2)
                        .margin_start(8)
                        .margin_end(8)
                        .margin_top(4)
                        .margin_bottom(12)
                        .build();

                    if self.immersive && current_idx_val > 0 {
                        *self.anchor_target.borrow_mut() = Some(up_next_box.clone().upcast());
                    }

                    for (idx, track) in items.iter().enumerate().skip(start_idx) {
                        let row = self.build_upcoming_row(idx, track, sender.clone());
                        up_next_box.append(&row);
                    }

                    widgets.content_box.append(&up_next_box);
                }
            }
        } else if self.autoplay && !is_live && self.queue.current_track().is_some() {
            let auto_title = "AUTOPLAY";
            let header_widget = if sections_list.is_empty() {
                None
            } else {
                let header = Self::build_section_header(auto_title);
                widgets.content_box.append(&header);
                Some(header)
            };
            sections_list.push((auto_title, header_widget));

            let loading_box = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(SPACING_SM)
                .margin_start(SPACING_MD)
                .margin_end(SPACING_MD)
                .margin_top(SPACING_MD)
                .margin_bottom(SPACING_MD)
                .valign(gtk::Align::Center)
                .build();

            let spinner = gtk::Spinner::builder().spinning(true).build();
            loading_box.append(&spinner);

            let label = gtk::Label::builder()
                .label("Finding similar songs…")
                .css_classes(vec!["queue-empty-subtext".to_string()])
                .build();
            loading_box.append(&label);

            if self.immersive && current_idx_val > 0 {
                *self.anchor_target.borrow_mut() = Some(loading_box.clone().upcast());
            }
            widgets.content_box.append(&loading_box);
        } else {
            let empty_text = if is_live {
                "Live broadcast"
            } else {
                "No upcoming songs"
            };
            let empty_label = gtk::Label::builder()
                .label(empty_text)
                .css_classes(vec!["queue-empty-subtext".to_string()])
                .margin_top(SPACING_MD)
                .margin_bottom(SPACING_MD)
                .build();
            if self.immersive && current_idx_val > 0 {
                *self.anchor_target.borrow_mut() = Some(empty_label.clone().upcast());
            }
            widgets.content_box.append(&empty_label);
        }

        if current_idx_val > 0 {
            let initial_pad = (widgets.scrolled_window.vadjustment().page_size() as i32).max(600);
            widgets.content_box.set_margin_bottom(initial_pad);
        } else {
            widgets.content_box.set_margin_bottom(24);
        }

        *self.section_headers.borrow_mut() = sections_list;

        // Immediately update sticky header position and visibility
        let cb = widgets.content_box.clone();
        let sbox = widgets.sticky_header_box.clone();
        let slabel = widgets.sticky_header_label.clone();
        let s_headers = self.section_headers.clone();
        let vadj = widgets.scrolled_window.vadjustment();

        update_sticky_header_position(&vadj, &s_headers.borrow(), &cb, &sbox, &slabel);

        let cb_idle = cb.clone();
        let sbox_idle = sbox.clone();
        let slabel_idle = slabel.clone();
        let s_headers_idle = s_headers.clone();
        let vadj_idle = vadj.clone();
        glib::idle_add_local_once(move || {
            update_sticky_header_position(
                &vadj_idle,
                &s_headers_idle.borrow(),
                &cb_idle,
                &sbox_idle,
                &slabel_idle,
            );
        });
    }

    fn has_upcoming(&self) -> bool {
        let items_len = self.queue.items.len();
        if items_len == 0 {
            return false;
        }
        let effective_idx = match self.queue.current_index {
            Some(idx) if idx < items_len => idx,
            _ => 0,
        };
        effective_idx + 1 < items_len
    }

    fn build_queue_item_popover(
        &self,
        idx: Option<usize>,
        track: &Track,
        sender: ComponentSender<Self>,
        include_remove: bool,
    ) -> gtk::Popover {
        Self::build_queue_item_popover_helper(idx, track, sender, include_remove)
    }

    fn build_queue_item_popover_helper(
        idx: Option<usize>,
        track: &Track,
        sender: ComponentSender<Self>,
        include_remove: bool,
    ) -> gtk::Popover {
        let popover = gtk::Popover::new();
        let box_container = gtk::Box::new(gtk::Orientation::Vertical, 2);
        box_container.set_margin_top(4);
        box_container.set_margin_bottom(4);
        box_container.set_margin_start(4);
        box_container.set_margin_end(4);

        fn create_popover_btn(icon_name: &'static str, label_text: &str) -> gtk::Button {
            let btn = gtk::Button::new();
            btn.add_css_class("flat");
            btn.add_css_class("queue-popover-btn");
            btn.set_focus_on_click(false);
            let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            content.set_margin_start(4);
            content.set_margin_end(8);
            content.set_margin_top(2);
            content.set_margin_bottom(2);
            let icon = gtk::Image::from_icon_name(icon_name);
            let label = gtk::Label::builder()
                .label(label_text)
                .xalign(0.0)
                .hexpand(true)
                .build();
            content.append(&icon);
            content.append(&label);
            btn.set_child(Some(&content));
            btn
        }

        // 1. Play Now (if index is available)
        if let Some(i) = idx {
            let btn_now = create_popover_btn(ICON_PLAY, "Play Now");
            let s = sender.clone();
            let p = popover.clone();
            btn_now.connect_clicked(move |_| {
                s.input(QueueInput::Jump(i));
                p.popdown();
            });
            box_container.append(&btn_now);
        }

        // 2. Play Next
        let btn_next = create_popover_btn(ICON_NEXT, "Play Next");
        let s = sender.clone();
        let p = popover.clone();
        let track_ref = track.id.clone();
        btn_next.connect_clicked(move |_| {
            s.input(QueueInput::PlayNext(track_ref.clone()));
            p.popdown();
        });
        box_container.append(&btn_next);

        // 3. Play Later
        let btn_later = create_popover_btn(ICON_ADD, "Play Later");
        let s = sender.clone();
        let p = popover.clone();
        let track_ref = track.id.clone();
        btn_later.connect_clicked(move |_| {
            s.input(QueueInput::PlayLater(track_ref.clone()));
            p.popdown();
        });
        box_container.append(&btn_later);

        // 4. Copy Link
        if let Some(url) = track.id.web_url() {
            let btn_copy = create_popover_btn("edit-copy-symbolic", "Copy Link");
            let p = popover.clone();
            btn_copy.connect_clicked(move |b| {
                b.clipboard().set_text(&url);
                p.popdown();
            });
            box_container.append(&btn_copy);
        }

        // 5. Remove from Queue (for upcoming tracks)
        if include_remove && let Some(i) = idx {
            let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
            sep.set_margin_top(2);
            sep.set_margin_bottom(2);
            box_container.append(&sep);

            let btn_remove = create_popover_btn(ICON_REMOVE, "Remove from Queue");
            let s = sender.clone();
            let p = popover.clone();
            btn_remove.connect_clicked(move |_| {
                s.input(QueueInput::Remove(i));
                p.popdown();
            });
            box_container.append(&btn_remove);
        }

        popover.set_child(Some(&box_container));
        popover
    }

    fn build_current_row(&self, track: &Track, sender: ComponentSender<Self>) -> gtk::Box {
        let is_active = self.playback_state != PlaybackState::Stopped;
        let mut classes = vec!["queue-row".to_string()];
        if is_active {
            classes.push("queue-row-current".to_string());
        }
        let row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(10)
            .css_classes(classes)
            .height_request(44)
            .build();

        let art = SquareArtwork::new(40, "queue-art");
        let url = track.artwork.as_ref().map(|a| a.url.clone());
        bind_artwork(art.picture(), &self.artwork_service, url, 96);
        row.append(&art);

        let info_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .hexpand(true)
            .valign(gtk::Align::Center)
            .build();

        let title_label = gtk::Label::builder()
            .label(&track.title)
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .css_classes(vec!["queue-title".to_string()])
            .build();
        info_box.append(&title_label);

        let artist_label = gtk::Label::builder()
            .label(track.artist_display())
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .css_classes(vec!["queue-artist".to_string()])
            .build();
        info_box.append(&artist_label);

        row.append(&info_box);

        let dur_label = gtk::Label::builder()
            .label(format_time(track.duration_ms.unwrap_or(0)))
            .css_classes(vec!["queue-duration".to_string()])
            .valign(gtk::Align::Center)
            .build();
        row.append(&dur_label);

        // More options menu button (Play Next, Play Later)
        let menu_btn = gtk::MenuButton::builder()
            .icon_name(ICON_MORE)
            .css_classes(vec!["flat".to_string(), "queue-action-btn".to_string()])
            .valign(gtk::Align::Center)
            .tooltip_text("More actions")
            .focus_on_click(false)
            .build();
        let popover = self.build_queue_item_popover(None, track, sender.clone(), false);
        menu_btn.set_popover(Some(&popover));
        row.append(&menu_btn);

        let row_weak = row.downgrade();
        let s_right = sender;
        let t_right = track.clone();
        let right_click = gtk::GestureClick::new();
        right_click.set_button(gtk::gdk::BUTTON_SECONDARY);
        right_click.connect_pressed(move |g, _, x, y| {
            if let Some(r) = row_weak.upgrade() {
                g.set_state(gtk::EventSequenceState::Claimed);
                let pop =
                    Self::build_queue_item_popover_helper(None, &t_right, s_right.clone(), false);
                pop.set_parent(&r);
                pop.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
                pop.popup();
            }
        });
        row.add_controller(right_click);

        row
    }

    fn build_upcoming_row(
        &self,
        idx: usize,
        track: &Track,
        sender: ComponentSender<Self>,
    ) -> gtk::Box {
        let row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(10)
            .css_classes(vec!["queue-row".to_string()])
            .height_request(44)
            .build();

        let art = SquareArtwork::new(36, "queue-art");
        art.add_css_class("upcoming");
        let url = track.artwork.as_ref().map(|a| a.url.clone());
        bind_artwork(art.picture(), &self.artwork_service, url, 96);
        row.append(&art);

        let info_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .hexpand(true)
            .valign(gtk::Align::Center)
            .build();

        let title_label = gtk::Label::builder()
            .label(&track.title)
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .css_classes(vec!["queue-title".to_string()])
            .build();
        info_box.append(&title_label);

        let artist_label = gtk::Label::builder()
            .label(track.artist_display())
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .css_classes(vec!["queue-artist".to_string()])
            .build();
        info_box.append(&artist_label);

        row.append(&info_box);

        let dur_label = gtk::Label::builder()
            .label(format_time(track.duration_ms.unwrap_or(0)))
            .css_classes(vec!["queue-duration".to_string()])
            .valign(gtk::Align::Center)
            .build();
        row.append(&dur_label);

        // Play Now button
        let btn_jump = gtk::Button::builder()
            .icon_name(ICON_PLAY)
            .css_classes(vec!["flat".to_string(), "queue-action-btn".to_string()])
            .valign(gtk::Align::Center)
            .tooltip_text("Play now")
            .focus_on_click(false)
            .build();
        let s_jump = sender.clone();
        btn_jump.connect_clicked(move |_| {
            s_jump.input(QueueInput::Jump(idx));
        });
        row.append(&btn_jump);

        // More options menu button (Play Now, Play Next, Play Later, Remove)
        let menu_btn = gtk::MenuButton::builder()
            .icon_name(ICON_MORE)
            .css_classes(vec!["flat".to_string(), "queue-action-btn".to_string()])
            .valign(gtk::Align::Center)
            .tooltip_text("More actions")
            .focus_on_click(false)
            .build();
        let popover = self.build_queue_item_popover(Some(idx), track, sender.clone(), true);
        menu_btn.set_popover(Some(&popover));
        row.append(&menu_btn);

        // Remove button
        let btn_remove = gtk::Button::builder()
            .icon_name(ICON_REMOVE)
            .css_classes(vec!["flat".to_string(), "queue-action-btn".to_string()])
            .valign(gtk::Align::Center)
            .tooltip_text("Remove from queue")
            .focus_on_click(false)
            .build();
        let s_remove = sender.clone();
        btn_remove.connect_clicked(move |_| {
            s_remove.input(QueueInput::Remove(idx));
        });
        row.append(&btn_remove);

        let row_weak = row.downgrade();
        let s_right = sender.clone();
        let t_right = track.clone();
        let right_click = gtk::GestureClick::new();
        right_click.set_button(gtk::gdk::BUTTON_SECONDARY);
        right_click.connect_pressed(move |g, _, x, y| {
            if let Some(r) = row_weak.upgrade() {
                g.set_state(gtk::EventSequenceState::Claimed);
                let pop = Self::build_queue_item_popover_helper(
                    Some(idx),
                    &t_right,
                    s_right.clone(),
                    true,
                );
                pop.set_parent(&r);
                pop.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
                pop.popup();
            }
        });
        row.add_controller(right_click);

        let click_gesture = gtk::GestureClick::new();
        click_gesture.set_button(gtk::gdk::BUTTON_PRIMARY);
        let s_click = sender;
        click_gesture.connect_released(move |g, n, _, _| {
            if n == 1 {
                g.set_state(gtk::EventSequenceState::Claimed);
                s_click.input(QueueInput::Jump(idx));
            }
        });
        row.add_controller(click_gesture);

        row
    }

    fn build_history_row(
        &self,
        idx: usize,
        track: &Track,
        sender: ComponentSender<Self>,
    ) -> gtk::Box {
        let row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(10)
            .css_classes(vec![
                "queue-row".to_string(),
                "queue-row-history".to_string(),
            ])
            .height_request(44)
            .build();

        let art = SquareArtwork::new(36, "queue-art");
        art.add_css_class("upcoming");
        let url = track.artwork.as_ref().map(|a| a.url.clone());
        bind_artwork(art.picture(), &self.artwork_service, url, 96);
        row.append(&art);

        let info_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .hexpand(true)
            .valign(gtk::Align::Center)
            .build();

        let title_label = gtk::Label::builder()
            .label(&track.title)
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .css_classes(vec!["queue-title".to_string()])
            .build();
        info_box.append(&title_label);

        let artist_label = gtk::Label::builder()
            .label(track.artist_display())
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .css_classes(vec!["queue-artist".to_string()])
            .build();
        info_box.append(&artist_label);

        row.append(&info_box);

        let dur_label = gtk::Label::builder()
            .label(format_time(track.duration_ms.unwrap_or(0)))
            .css_classes(vec!["queue-duration".to_string()])
            .valign(gtk::Align::Center)
            .build();
        row.append(&dur_label);

        // Play Now button
        let btn_jump = gtk::Button::builder()
            .icon_name(ICON_PLAY)
            .css_classes(vec!["flat".to_string(), "queue-action-btn".to_string()])
            .valign(gtk::Align::Center)
            .tooltip_text("Play now")
            .focus_on_click(false)
            .build();
        let s_jump = sender.clone();
        btn_jump.connect_clicked(move |_| {
            s_jump.input(QueueInput::Jump(idx));
        });
        row.append(&btn_jump);

        // More options menu button (Play Now, Play Next, Play Later)
        let menu_btn = gtk::MenuButton::builder()
            .icon_name(ICON_MORE)
            .css_classes(vec!["flat".to_string(), "queue-action-btn".to_string()])
            .valign(gtk::Align::Center)
            .tooltip_text("More actions")
            .focus_on_click(false)
            .build();
        let popover = self.build_queue_item_popover(Some(idx), track, sender.clone(), false);
        menu_btn.set_popover(Some(&popover));
        row.append(&menu_btn);

        let row_weak = row.downgrade();
        let s_right = sender.clone();
        let t_right = track.clone();
        let right_click = gtk::GestureClick::new();
        right_click.set_button(gtk::gdk::BUTTON_SECONDARY);
        right_click.connect_pressed(move |g, _, x, y| {
            if let Some(r) = row_weak.upgrade() {
                g.set_state(gtk::EventSequenceState::Claimed);
                let pop = Self::build_queue_item_popover_helper(
                    Some(idx),
                    &t_right,
                    s_right.clone(),
                    false,
                );
                pop.set_parent(&r);
                pop.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
                pop.popup();
            }
        });
        row.add_controller(right_click);

        let click_gesture = gtk::GestureClick::new();
        click_gesture.set_button(gtk::gdk::BUTTON_PRIMARY);
        let s_click = sender;
        click_gesture.connect_released(move |g, n, _, _| {
            if n == 1 {
                g.set_state(gtk::EventSequenceState::Claimed);
                s_click.input(QueueInput::Jump(idx));
            }
        });
        row.add_controller(click_gesture);

        row
    }
}
