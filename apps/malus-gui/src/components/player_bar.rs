//! PlayerBar component with GLib local extrapolation tick and capability gating.
//!
//! Controls are gated strictly by the PLAYBACK provider's capabilities.
//! Progress slider uses a local GLib timeout (zero IPC).

use malus_client::{ClientError, MalusClient, PlaybackStateWire, PlayerStatusWire};
use relm4::gtk::{self, prelude::*};
use relm4::prelude::*;
use std::time::Duration;

use crate::artwork::ArtworkService;
use crate::components::artwork_widget::{ArtworkInit, ArtworkInput, ArtworkWidget};
use crate::model::{PlayerModel, ProviderCache, format_time};

pub struct PlayerBar {
    client: MalusClient,
    player: PlayerModel,
    provider_cache: ProviderCache,

    // Local seek drag state
    is_dragging_seek: bool,
    drag_seek_ms: u64,

    // Artwork subcomponent
    artwork_comp: Controller<ArtworkWidget>,

    // GLib local timeout source tag for progress tick
    _tick_source: Option<gtk::glib::SourceId>,
}

#[derive(Debug)]
pub enum PlayerBarInput {
    StatusChanged(PlayerStatusWire),
    UpdateProviderCache(ProviderCache),
    Tick,
    PlayClicked,
    SeekStarted,
    SeekMoved(f64),
    SeekReleased(f64),
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
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 16,
            add_css_class: "player-bar",

            // 1. Idle State
            #[name(idle_box)]
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_hexpand: true,
                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::Center,
                #[watch]
                set_visible: model.is_idle(),

                gtk::Label {
                    set_text: "No track playing",
                    add_css_class: "player-idle",
                },
            },

            // 2. Active Player UI
            #[name(active_box)]
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 16,
                set_hexpand: true,
                #[watch]
                set_visible: !model.is_idle(),

                // Left: Artwork & Track Metadata
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 12,
                    set_size_request: (220, -1),
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

                // Center: Playback Controls & Progress Bar
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 4,
                    set_hexpand: true,
                    set_valign: gtk::Align::Center,

                    // Play/Pause button
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_halign: gtk::Align::Center,
                        set_spacing: 8,

                        gtk::Button {
                            add_css_class: "player-btn-primary",
                            #[watch]
                            set_visible: model.has_playback_cap("playback"),
                            #[watch]
                            set_icon_name: model.play_pause_icon(),
                            connect_clicked[sender] => move |_| {
                                sender.input(PlayerBarInput::PlayClicked);
                            },
                        },
                    },

                    // Progress slider + Time labels
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 8,
                        set_hexpand: true,
                        #[watch]
                        set_visible: model.has_playback_cap("playback.seek"),

                        #[name(elapsed_lbl)]
                        gtk::Label {
                            add_css_class: "player-time-label",
                            #[watch]
                            set_text: &format_time(model.display_position_ms()),
                        },

                        #[name(seek_slider)]
                        gtk::Scale {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_hexpand: true,
                            set_draw_value: false,
                            #[watch]
                            set_range: (0.0, model.duration_secs()),
                            #[watch]
                            set_value: model.slider_value_secs(),
                        },

                        #[name(duration_lbl)]
                        gtk::Label {
                            add_css_class: "player-time-label",
                            #[watch]
                            set_text: &format_time(model.player.duration_ms.unwrap_or(0)),
                        },
                    },
                },

                // Right spacer / balance
                gtk::Box {
                    set_size_request: (220, -1),
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
                size: (44, 44),
                css_class: "player-artwork".into(),
                service: artwork_service,
            })
            .detach();

        // Local GLib progress timer (~250ms, zero IPC)
        let s_tick = sender.clone();
        let tick_source = gtk::glib::timeout_add_local(Duration::from_millis(250), move || {
            s_tick.input(PlayerBarInput::Tick);
            gtk::glib::ControlFlow::Continue
        });

        let model = Self {
            client,
            player: PlayerModel::default(),
            provider_cache: ProviderCache::default(),
            is_dragging_seek: false,
            drag_seek_ms: 0,
            artwork_comp,
            _tick_source: Some(tick_source),
        };

        let art_widget = model.artwork_comp.widget();
        let widgets = view_output!();

        // Connect seek slider drag events
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
            PlayerBarInput::UpdateProviderCache(cache) => {
                self.provider_cache = cache;
            }
            PlayerBarInput::Tick => {
                // Monotonic local tick — view! updates extrapolated position
            }
            PlayerBarInput::PlayClicked => {
                let client = self.client.clone();
                let is_playing = self.player.state == PlaybackStateWire::Playing;

                sender.oneshot_command(async move {
                    let res = if is_playing {
                        client.pause().await
                    } else {
                        client.play().await
                    };
                    PlayerCmd::CommandResult(res)
                });
            }
            PlayerBarInput::SeekStarted => {
                self.is_dragging_seek = true;
            }
            PlayerBarInput::SeekMoved(val_secs) => {
                self.drag_seek_ms = (val_secs * 1000.0) as u64;
            }
            PlayerBarInput::SeekReleased(val_secs) => {
                self.is_dragging_seek = false;
                let target_ms = (val_secs * 1000.0) as u64;
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
        _message: Self::CommandOutput,
        _sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        // Authoritative state arrives via daemon event subscription
    }
}

impl PlayerBar {
    fn is_idle(&self) -> bool {
        self.player.current_track.is_none() && self.player.state == PlaybackStateWire::Stopped
    }

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
            "media-playback-pause-symbolic"
        } else {
            "media-playback-start-symbolic"
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

    fn has_playback_cap(&self, cap: &str) -> bool {
        if let Some(ref prov) = self.player.playback_provider {
            self.provider_cache.has_capability(prov, cap)
        } else {
            // Default to true for core playback if provider not yet derived
            cap == "playback" || cap == "playback.seek"
        }
    }
}

impl Drop for PlayerBar {
    fn drop(&mut self) {
        if let Some(source) = self._tick_source.take() {
            source.remove();
        }
    }
}
