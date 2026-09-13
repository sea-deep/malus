//! Persistent native GTK player footer (88px).
//!
//! Two conceptual layers:
//! - Primary control row (56px)
//! - Progress / seek row (32px)
//!
//! Features:
//! - 48x48 artwork (10px radius)
//! - GTK semantic symbolic icons (Prev 40px, Play/Pause 48px, Next 40px)
//! - First-class native GtkScale seek bar with 4px trough and tabular numerals
//! - Local monotonic extrapolation with drag-to-seek suspension
//! - Humanized error handling and quiet inactive state

use malus_client::{ClientError, MalusClient, PlaybackStateWire, PlayerStatusWire};
use relm4::gtk::{self, prelude::*};
use relm4::prelude::*;
use std::time::Duration;

use crate::components::artwork_widget::{ArtworkInit, ArtworkInput, ArtworkWidget};
use crate::design::tokens::*;
use crate::model::{PlayerModel, format_remaining_time, format_time};
use crate::services::ArtworkService;

pub struct PlayerBar {
    client: MalusClient,
    player: PlayerModel,

    // Seek drag state
    is_dragging_seek: bool,
    drag_seek_ms: u64,

    // Subcomponents
    artwork_comp: Controller<ArtworkWidget>,

    // Extrapolation tick
    _tick_source: Option<gtk::glib::SourceId>,
}

#[derive(Debug)]
pub enum PlayerBarInput {
    StatusChanged(PlayerStatusWire),
    Tick,
    PlayPauseClicked,
    PreviousClicked,
    NextClicked,
    VolumeChanged(f64),
    SeekReleased(f64),
    SeekMoved(f64),
}

#[derive(Debug)]
pub enum PlayerCmd {
    CommandResult(Result<(), ClientError>),
}

#[relm4::component(pub)]
impl Component for PlayerBar {
    type Init = (MalusClient, ArtworkService);
    type Input = PlayerBarInput;
    type Output = ();
    type CommandOutput = PlayerCmd;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,
            add_css_class: "player",
            set_height_request: PLAYER_TOTAL_HEIGHT,

            // ========================================================
            // Layer 1: Primary Controls Row (56px)
            // ========================================================
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 16,
                set_hexpand: true,
                add_css_class: "player-control-row",

                // Left: Artwork & Metadata
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 12,
                    set_size_request: (240, -1),
                    set_valign: gtk::Align::Center,

                    #[local_ref]
                    art_widget -> gtk::Picture {},

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 2,
                        set_valign: gtk::Align::Center,

                        gtk::Label {
                            set_xalign: 0.0,
                            set_ellipsize: gtk::pango::EllipsizeMode::End,
                            add_css_class: "player-title",
                            #[watch]
                            set_text: model.track_title(),
                        },

                        gtk::Label {
                            set_xalign: 0.0,
                            set_ellipsize: gtk::pango::EllipsizeMode::End,
                            add_css_class: "player-artist",
                            #[watch]
                            set_text: &model.track_artist(),
                        },
                    },
                },

                // Center: Transport Controls (Prev, Play/Pause, Next)
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 12,
                    set_hexpand: true,
                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::Center,

                    // Previous (40x40)
                    gtk::Button {
                        add_css_class: "player-btn-normal",
                        set_icon_name: ICON_PREVIOUS,
                        set_tooltip_text: Some("Previous"),
                        #[watch]
                        set_sensitive: !model.player.is_idle(),
                        connect_clicked[sender] => move |_| {
                            sender.input(PlayerBarInput::PreviousClicked);
                        },
                    },

                    // Play/Pause (48x48 prominent)
                    gtk::Button {
                        add_css_class: "player-btn-prominent",
                        set_tooltip_text: Some(if model.player.state == PlaybackStateWire::Playing { "Pause" } else { "Play" }),
                        #[watch]
                        set_icon_name: model.play_pause_icon(),
                        #[watch]
                        set_sensitive: !model.player.is_idle(),
                        connect_clicked[sender] => move |_| {
                            sender.input(PlayerBarInput::PlayPauseClicked);
                        },
                    },

                    // Next (40x40)
                    gtk::Button {
                        add_css_class: "player-btn-normal",
                        set_icon_name: ICON_NEXT,
                        set_tooltip_text: Some("Next"),
                        #[watch]
                        set_sensitive: !model.player.is_idle(),
                        connect_clicked[sender] => move |_| {
                            sender.input(PlayerBarInput::NextClicked);
                        },
                    },
                },

                // Right: Secondary Controls (Queue & Volume)
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 12,
                    set_size_request: (240, -1),
                    set_halign: gtk::Align::End,
                    set_valign: gtk::Align::Center,

                    // Queue Button (40x40)
                    gtk::Button {
                        add_css_class: "player-btn-normal",
                        set_icon_name: ICON_QUEUE,
                        set_tooltip_text: Some("Queue"),
                    },

                    // Volume Icon & Slider
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 8,
                        set_valign: gtk::Align::Center,

                        gtk::Image {
                            #[watch]
                            set_icon_name: Some(model.volume_icon()),
                            set_pixel_size: ICON_GLYPH_MD,
                        },

                        #[name(vol_slider)]
                        gtk::Scale {
                            add_css_class: "player-volume-scale",
                            set_orientation: gtk::Orientation::Horizontal,
                            set_draw_value: false,
                            set_range: (0.0, 100.0),
                            #[watch]
                            set_value: model.player.volume as f64,
                            connect_value_changed[sender] => move |scale| {
                                sender.input(PlayerBarInput::VolumeChanged(scale.value()));
                            },
                        },
                    },
                },
            },

            // ========================================================
            // Layer 2: Progress / Seek Row (32px)
            // ========================================================
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                set_hexpand: true,
                set_valign: gtk::Align::Center,
                add_css_class: "player-progress-row",

                // Elapsed Time
                #[name(elapsed_lbl)]
                gtk::Label {
                    add_css_class: "player-time",
                    set_xalign: 1.0,
                    #[watch]
                    set_text: &format_time(model.display_position_ms()),
                },

                // Seek Slider
                #[name(seek_slider)]
                gtk::Scale {
                    add_css_class: "player-seek-scale",
                    set_orientation: gtk::Orientation::Horizontal,
                    set_hexpand: true,
                    set_draw_value: false,
                    #[watch]
                    set_sensitive: !model.player.is_idle() && model.duration_secs() > 0.0,
                    #[watch]
                    set_range: (0.0, model.duration_secs()),
                    #[watch]
                    set_value: model.slider_value_secs(),
                },

                // Remaining Time (-M:SS)
                #[name(remaining_lbl)]
                gtk::Label {
                    add_css_class: "player-time",
                    set_xalign: 0.0,
                    #[watch]
                    set_text: &format_remaining_time(
                        model.display_position_ms(),
                        model.player.duration_ms.unwrap_or(0),
                    ),
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let (client, artwork_service) = init;

        let artwork_comp = ArtworkWidget::builder()
            .launch(ArtworkInit {
                url: None,
                size: (PLAYER_ARTWORK_SIZE, PLAYER_ARTWORK_SIZE),
                css_class: "player-artwork".into(),
                service: artwork_service,
            })
            .detach();

        // Local monotonic progress extrapolation (~250ms tick, zero IPC)
        let s_tick = sender.clone();
        let tick_source = gtk::glib::timeout_add_local(Duration::from_millis(250), move || {
            s_tick.input(PlayerBarInput::Tick);
            gtk::glib::ControlFlow::Continue
        });

        let model = Self {
            client,
            player: PlayerModel::default(),
            is_dragging_seek: false,
            drag_seek_ms: 0,
            artwork_comp,
            _tick_source: Some(tick_source),
        };

        let art_widget = model.artwork_comp.widget();
        let widgets = view_output!();

        // Handle seek slider release to fire single RPC
        let s_change = sender.clone();
        widgets
            .seek_slider
            .connect_change_value(move |_scale, _scroll, value| {
                s_change.input(PlayerBarInput::SeekReleased(value));
                gtk::glib::Propagation::Proceed
            });

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            PlayerBarInput::StatusChanged(status) => {
                let prev_track_id = self.player.current_track.as_ref().map(|t| t.id.clone());
                self.player.update_from_wire(
                    status.state,
                    status.current_track,
                    status.position_ms,
                    Some(status.duration_ms),
                    Some(status.volume),
                );

                eprintln!(
                    "[DEBUG player] StatusChanged: state={:?}, track={:?}, pos={}ms, dur={:?}ms, vol={}",
                    self.player.state,
                    self.player.current_track.as_ref().map(|t| &t.title),
                    self.player.position_ms,
                    self.player.duration_ms,
                    self.player.volume
                );

                let cur_track_id = self.player.current_track.as_ref().map(|t| t.id.clone());
                if prev_track_id != cur_track_id {
                    let art_url = self
                        .player
                        .current_track
                        .as_ref()
                        .and_then(|t| t.artwork.as_ref())
                        .map(|a| a.url.clone());
                    self.artwork_comp.emit(ArtworkInput::SetUrl(art_url));
                    self.is_dragging_seek = false;
                }
            }
            PlayerBarInput::Tick => {
                // Monotonic local tick causes view! to re-render extrapolated position
            }
            PlayerBarInput::PlayPauseClicked => {
                let client = self.client.clone();
                let is_playing = self.player.state == PlaybackStateWire::Playing;
                eprintln!(
                    "[DEBUG player] PlayPauseClicked: currently is_playing={}",
                    is_playing
                );
                sender.oneshot_command(async move {
                    let res = if is_playing {
                        client.pause().await
                    } else {
                        client.play().await
                    };
                    PlayerCmd::CommandResult(res)
                });
            }
            PlayerBarInput::PreviousClicked => {
                eprintln!("[DEBUG player] PreviousClicked");
                let client = self.client.clone();
                sender.oneshot_command(async move {
                    let res = client.previous().await;
                    PlayerCmd::CommandResult(res)
                });
            }
            PlayerBarInput::NextClicked => {
                eprintln!("[DEBUG player] NextClicked");
                let client = self.client.clone();
                sender.oneshot_command(async move {
                    let res = client.next().await;
                    PlayerCmd::CommandResult(res)
                });
            }
            PlayerBarInput::VolumeChanged(val) => {
                let vol = val.clamp(0.0, 100.0) as u8;
                if vol != self.player.volume {
                    eprintln!("[DEBUG player] VolumeChanged -> {}", vol);
                    self.player.volume = vol;
                    let client = self.client.clone();
                    sender.oneshot_command(async move {
                        let res = client.set_volume(vol).await;
                        PlayerCmd::CommandResult(res)
                    });
                }
            }
            PlayerBarInput::SeekMoved(val_secs) => {
                self.is_dragging_seek = true;
                self.drag_seek_ms = (val_secs * 1000.0) as u64;
            }
            PlayerBarInput::SeekReleased(val_secs) => {
                self.is_dragging_seek = false;
                let target_ms = (val_secs * 1000.0) as u64;
                eprintln!(
                    "[DEBUG player] SeekReleased -> {}ms ({}s)",
                    target_ms, val_secs
                );
                let client = self.client.clone();
                sender.oneshot_command(async move {
                    let res = client.seek(target_ms).await;
                    PlayerCmd::CommandResult(res)
                });
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
            PlayerCmd::CommandResult(Err(e)) => {
                tracing::warn!("Playback action failed: {e}");
            }
            PlayerCmd::CommandResult(Ok(())) => {}
        }
    }
}

impl PlayerBar {
    fn track_title(&self) -> &str {
        self.player
            .current_track
            .as_ref()
            .map(|t| t.title.as_str())
            .unwrap_or("")
    }

    fn track_artist(&self) -> String {
        self.player
            .current_track
            .as_ref()
            .map(|t| {
                t.artists
                    .iter()
                    .map(|a| a.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default()
    }

    fn play_pause_icon(&self) -> &'static str {
        if self.player.state == PlaybackStateWire::Playing {
            ICON_PAUSE
        } else {
            ICON_PLAY
        }
    }

    fn volume_icon(&self) -> &'static str {
        let vol = self.player.volume;
        if vol == 0 {
            ICON_VOLUME_MUTED
        } else if vol < 33 {
            ICON_VOLUME_LOW
        } else if vol < 66 {
            ICON_VOLUME_MED
        } else {
            ICON_VOLUME_HIGH
        }
    }

    fn display_position_ms(&self) -> u64 {
        if self.is_dragging_seek {
            self.drag_seek_ms
        } else {
            self.player.extrapolated_position_ms()
        }
    }

    fn slider_value_secs(&self) -> f64 {
        self.display_position_ms() as f64 / 1000.0
    }

    fn duration_secs(&self) -> f64 {
        (self.player.duration_ms.unwrap_or(0) as f64 / 1000.0).max(1.0)
    }
}

impl Drop for PlayerBar {
    fn drop(&mut self) {
        if let Some(source) = self._tick_source.take() {
            source.remove();
        }
    }
}
