//! Playing Next (Queue) utility pane.

use malus_client::MalusClient;
use malus_model::{Queue, Track};
use relm4::gtk::{self, prelude::*};
use relm4::prelude::*;

use crate::design::tokens::*;
use crate::model::format_time;
use crate::services::{ArtworkService, bind_artwork};
use crate::widgets::square_artwork::SquareArtwork;

pub struct QueuePane {
    client: MalusClient,
    artwork_service: ArtworkService,
    queue: Queue,
    close: std::rc::Rc<dyn Fn()>,
}

#[derive(Debug)]
pub enum QueueInput {
    SetQueue(Queue),
    Reload,
    Jump(usize),
    Remove(usize),
    ClearUpcoming,
    Close,
}

#[derive(Debug)]
pub enum QueueCmd {
    QueueLoaded(Queue),
}

#[relm4::component(pub)]
impl Component for QueuePane {
    type Init = (MalusClient, ArtworkService, std::rc::Rc<dyn Fn()>);
    type Input = QueueInput;
    type Output = ();
    type CommandOutput = QueueCmd;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,
            add_css_class: "utility-pane",

            // Pane Header
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                add_css_class: "utility-header",
                set_margin_start: 16,
                set_margin_end: 16,
                set_margin_top: 14,
                set_margin_bottom: 14,

                gtk::Label {
                    set_xalign: 0.0,
                    set_hexpand: true,
                    add_css_class: "utility-title",
                    set_text: "Playing Next",
                },

                gtk::Button {
                    add_css_class: "flat",
                    add_css_class: "utility-action-btn",
                    set_label: "Clear",
                    #[watch]
                    set_sensitive: model.has_upcoming(),
                    connect_clicked => QueueInput::ClearUpcoming,
                },

                gtk::Button {
                    add_css_class: "flat",
                    add_css_class: "player-icon-btn",
                    set_icon_name: ICON_CLOSE,
                    set_tooltip_text: Some("Close (Esc)"),
                    connect_clicked => QueueInput::Close,
                },
            },

            gtk::Separator {
                set_orientation: gtk::Orientation::Horizontal,
            },

            // Scrollable Content
            gtk::ScrolledWindow {
                set_hscrollbar_policy: gtk::PolicyType::Never,
                set_vscrollbar_policy: gtk::PolicyType::Automatic,
                set_vexpand: true,

                #[name(content_box)]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 16,
                    set_margin_start: 16,
                    set_margin_end: 16,
                    set_margin_top: 16,
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
        let (client, artwork_service, close) = init;
        let model = Self {
            client: client.clone(),
            artwork_service,
            queue: Queue::default(),
            close,
        };

        let widgets = view_output!();

        // Initial fetch of queue
        let c = client.clone();
        sender.oneshot_command(async move {
            let q = c.get_queue().await.unwrap_or_default();
            QueueCmd::QueueLoaded(q)
        });

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            QueueInput::SetQueue(q) => {
                self.queue = q;
            }
            QueueInput::Reload => {
                let c = self.client.clone();
                sender.oneshot_command(async move {
                    let q = c.get_queue().await.unwrap_or_default();
                    QueueCmd::QueueLoaded(q)
                });
            }
            QueueInput::Jump(idx) => {
                let c = self.client.clone();
                sender.oneshot_command(async move {
                    let _ = c.queue_jump(idx).await;
                    let q = c.get_queue().await.unwrap_or_default();
                    QueueCmd::QueueLoaded(q)
                });
            }
            QueueInput::Remove(idx) => {
                let c = self.client.clone();
                sender.oneshot_command(async move {
                    let _ = c.queue_remove(idx).await;
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
            QueueCmd::QueueLoaded(q) => {
                self.queue = q;
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
        self.render_content(widgets, sender);
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
        self.render_content(widgets, sender);
    }
}

impl QueuePane {
    fn render_content(&self, widgets: &mut QueuePaneWidgets, sender: ComponentSender<Self>) {
        // Rebuild queue item widgets inside content_box
        while let Some(child) = widgets.content_box.first_child() {
            widgets.content_box.remove(&child);
        }

        let items = &self.queue.items;

        if items.is_empty() {
            let empty_label = gtk::Label::builder()
                .label("Queue is empty")
                .css_classes(vec!["utility-empty-label".to_string()])
                .margin_top(32)
                .build();
            widgets.content_box.append(&empty_label);
            return;
        }

        // Determine effective index safely: if current_index is valid, use it;
        // otherwise default to 0 so we always show Now Playing when items exist.
        let effective_idx = match self.queue.current_index {
            Some(idx) if idx < items.len() => Some(idx),
            _ => Some(0),
        };

        // 1. Now Playing Section
        if let Some(idx) = effective_idx
            && let Some(track) = items.get(idx)
        {
            let sec_title = gtk::Label::builder()
                .label("NOW PLAYING")
                .xalign(0.0)
                .css_classes(vec!["queue-section-label".to_string()])
                .build();
            widgets.content_box.append(&sec_title);

            let row = self.build_current_row(track);
            widgets.content_box.append(&row);
        }

        // 2. Up Next Section
        let start_idx = effective_idx.map(|i| i + 1).unwrap_or(0);
        if start_idx < items.len() {
            let sec_title = gtk::Label::builder()
                .label("UP NEXT")
                .xalign(0.0)
                .css_classes(vec!["queue-section-label".to_string()])
                .margin_top(8)
                .build();
            widgets.content_box.append(&sec_title);

            let up_next_box = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(4)
                .build();

            for (idx, track) in items.iter().enumerate().skip(start_idx) {
                let row = self.build_upcoming_row(idx, track, sender.clone());
                up_next_box.append(&row);
            }

            widgets.content_box.append(&up_next_box);
        } else {
            let empty_label = gtk::Label::builder()
                .label("No upcoming songs")
                .css_classes(vec!["queue-empty-subtext".to_string()])
                .margin_top(16)
                .build();
            widgets.content_box.append(&empty_label);
        }
    }
}

impl QueuePane {
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

    fn build_current_row(&self, track: &Track) -> gtk::Box {
        let row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(10)
            .css_classes(vec![
                "queue-row".to_string(),
                "queue-row-current".to_string(),
            ])
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

        // Jump button
        let btn_jump = gtk::Button::builder()
            .icon_name(ICON_PLAY)
            .css_classes(vec!["flat".to_string(), "queue-action-btn".to_string()])
            .valign(gtk::Align::Center)
            .tooltip_text("Play this song next")
            .build();
        let s_jump = sender.clone();
        btn_jump.connect_clicked(move |_| {
            s_jump.input(QueueInput::Jump(idx));
        });
        row.append(&btn_jump);

        // Remove button
        let btn_remove = gtk::Button::builder()
            .icon_name(ICON_REMOVE)
            .css_classes(vec!["flat".to_string(), "queue-action-btn".to_string()])
            .valign(gtk::Align::Center)
            .tooltip_text("Remove from queue")
            .build();
        let s_remove = sender;
        btn_remove.connect_clicked(move |_| {
            s_remove.input(QueueInput::Remove(idx));
        });
        row.append(&btn_remove);

        row
    }
}
