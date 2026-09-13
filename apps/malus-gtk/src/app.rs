//! Root application window shell for malus-gui-next.
//!
//! Enforces:
//! - Music first, controls second, app chrome third, engineering never
//! - Clean ApplicationWindow with minimal headerbar (back/forward history only)
//! - Structurally attached 224px sidebar
//! - Responsive content surface (Home as default destination)
//! - Structurally attached 88px persistent GTK footer player
//! - Authoritative daemon event subscription & automatic reconnection

use malus_client::{ClientError, ClientEvent, ConnectionStatus, MalusClient, PlayerStatusWire};
use relm4::adw::{self, prelude::*};
use relm4::gtk;
use relm4::prelude::*;

use crate::components::player_bar::{PlayerBar, PlayerBarInput};
use crate::components::sidebar::{Sidebar, SidebarInput, SidebarOutput};
use crate::design::tokens::*;
use crate::model::{NavigationHistory, Route};
use crate::pages::home::{HomeInput, HomeOutput, HomePage};
use crate::services::ArtworkService;

pub struct MalusApp {
    client: MalusClient,
    _artwork_service: ArtworkService,
    history: NavigationHistory,
    connection_status: ConnectionStatus,

    // Child Components
    sidebar: Controller<Sidebar>,
    home_page: Controller<HomePage>,
    player_bar: Controller<PlayerBar>,
}

#[derive(Debug)]
pub enum AppInput {
    Navigate(Route),
    GoBack,
    GoForward,
    PlayMedia(String),
    TogglePlayback,
}

#[derive(Debug)]
pub enum AppCmd {
    ConnectionStatusChanged(ConnectionStatus),
    DaemonEvent(ClientEvent),
    InitialStatus(Option<PlayerStatusWire>),
    PlayResult(Result<(), ClientError>),
}

#[relm4::component(pub)]
impl Component for MalusApp {
    type Init = MalusClient;
    type Input = AppInput;
    type Output = ();
    type CommandOutput = AppCmd;

    view! {
        #[root]
        adw::ApplicationWindow {
            set_default_size: (1120, 760),
            set_title: Some("Malus"),
            add_css_class: "malus-window",

            adw::ToolbarView {
                // Top HeaderBar (Minimal: Navigation buttons + Window Title)
                add_top_bar = &adw::HeaderBar {
                    add_css_class: "malus-header",

                    pack_start = &gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 4,

                        gtk::Button {
                            add_css_class: "header-nav-btn",
                            set_icon_name: ICON_BACK,
                            set_tooltip_text: Some("Back"),
                            #[watch]
                            set_sensitive: model.history.can_go_back(),
                            connect_clicked => AppInput::GoBack,
                        },

                        gtk::Button {
                            add_css_class: "header-nav-btn",
                            set_icon_name: ICON_FORWARD,
                            set_tooltip_text: Some("Forward"),
                            #[watch]
                            set_sensitive: model.history.can_go_forward(),
                            connect_clicked => AppInput::GoForward,
                        },
                    },
                },

                // Main Layout: OverlaySplitView (Sidebar + Content Stack)
                #[wrap(Some)]
                set_content = &adw::OverlaySplitView {
                    set_min_sidebar_width: SIDEBAR_WIDTH_NORMAL,
                    set_max_sidebar_width: SIDEBAR_WIDTH_NORMAL,
                    set_sidebar_width_fraction: 0.20,

                    #[wrap(Some)]
                    #[local_ref]
                    set_sidebar = sidebar_widget -> gtk::Box {},

                    #[name(content_stack)]
                    #[wrap(Some)]
                    set_content = &gtk::Stack {
                        set_transition_type: gtk::StackTransitionType::Crossfade,
                        #[watch]
                        set_visible_child_name: model.active_route_name(),
                    },
                },

                // Structurally Attached Bottom Player
                #[local_ref]
                add_bottom_bar = player_widget -> gtk::Box {},
            },
        }
    }

    fn init(
        client: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        if let Ok(theme) = std::env::var("MALUS_THEME") {
            let manager = adw::StyleManager::default();
            if theme.eq_ignore_ascii_case("light") {
                manager.set_color_scheme(adw::ColorScheme::ForceLight);
            } else if theme.eq_ignore_ascii_case("dark") {
                manager.set_color_scheme(adw::ColorScheme::ForceDark);
            }
        }

        let artwork_service = ArtworkService::new();

        let sidebar = Sidebar::builder().launch(Route::Home).forward(
            sender.input_sender(),
            |out| match out {
                SidebarOutput::Navigate(route) => AppInput::Navigate(route),
            },
        );

        let home_page = HomePage::builder()
            .launch((client.clone(), artwork_service.clone()))
            .forward(sender.input_sender(), |out| match out {
                HomeOutput::PlayItem(id) => AppInput::PlayMedia(id),
            });

        let player_bar = PlayerBar::builder()
            .launch((client.clone(), artwork_service.clone()))
            .detach();

        let model = Self {
            client: client.clone(),
            _artwork_service: artwork_service,
            history: NavigationHistory::default(),
            connection_status: ConnectionStatus::Connecting,
            sidebar,
            home_page,
            player_bar,
        };

        // Authoritative background event subscription
        let client_events = client.clone();
        sender.command(move |out, shutdown| {
            shutdown
                .register(async move {
                    let (mut event_rx, mut status_rx) = client_events.subscribe_events();
                    loop {
                        tokio::select! {
                            res = status_rx.changed() => {
                                if res.is_ok() {
                                    let status = *status_rx.borrow();
                                    let _ = out.send(AppCmd::ConnectionStatusChanged(status));
                                } else {
                                    break;
                                }
                            }
                            event = event_rx.recv() => {
                                match event {
                                    Ok(ev) => {
                                        let _ = out.send(AppCmd::DaemonEvent(ev));
                                    }
                                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                        tracing::warn!("Lagged by {n} daemon events");
                                    }
                                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                })
                .drop_on_shutdown()
        });

        // Initial fetch of player status
        let client_init = client.clone();
        sender.oneshot_command(async move {
            let status = client_init.get_status().await.ok();
            AppCmd::InitialStatus(status)
        });

        let sidebar_widget = model.sidebar.widget();
        let player_widget = model.player_bar.widget();
        let widgets = view_output!();

        if let Ok(w_str) = std::env::var("MALUS_WINDOW_WIDTH") {
            if let Ok(w) = w_str.parse::<i32>() {
                if let Ok(h_str) = std::env::var("MALUS_WINDOW_HEIGHT") {
                    if let Ok(h) = h_str.parse::<i32>() {
                        root.set_default_size(w, h);
                        root.set_size_request(w, h);
                    }
                }
            }
        }

        widgets
            .content_stack
            .add_named(model.home_page.widget(), Some("home"));
        widgets.content_stack.set_visible_child_name("home");

        // Global keyboard shortcuts
        let event_ctrl = gtk::EventControllerKey::new();
        let s_key = sender.clone();
        event_ctrl.connect_key_pressed(move |_ctrl, keyval, _code, modifier| {
            if modifier.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
                if keyval == gtk::gdk::Key::k
                    || keyval == gtk::gdk::Key::l
                    || keyval == gtk::gdk::Key::f
                {
                    s_key.input(AppInput::Navigate(Route::Search));
                    return gtk::glib::Propagation::Stop;
                }
            } else if modifier.contains(gtk::gdk::ModifierType::ALT_MASK) {
                if keyval == gtk::gdk::Key::Left {
                    s_key.input(AppInput::GoBack);
                    return gtk::glib::Propagation::Stop;
                } else if keyval == gtk::gdk::Key::Right {
                    s_key.input(AppInput::GoForward);
                    return gtk::glib::Propagation::Stop;
                }
            } else if keyval == gtk::gdk::Key::space {
                s_key.input(AppInput::TogglePlayback);
                return gtk::glib::Propagation::Stop;
            }
            gtk::glib::Propagation::Proceed
        });
        root.add_controller(event_ctrl);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            AppInput::Navigate(route) => {
                eprintln!("[DEBUG app] Navigate -> {:?}", route);
                self.history.navigate_to(route.clone());
                self.sidebar
                    .emit(SidebarInput::SetActiveRoute(route.clone()));
                if route == Route::Home {
                    self.home_page.emit(HomeInput::Reload);
                }
            }
            AppInput::GoBack => {
                if let Some(route) = self.history.go_back().cloned() {
                    eprintln!("[DEBUG app] GoBack -> {:?}", route);
                    self.sidebar
                        .emit(SidebarInput::SetActiveRoute(route.clone()));
                    if route == Route::Home {
                        self.home_page.emit(HomeInput::Reload);
                    }
                }
            }
            AppInput::GoForward => {
                if let Some(route) = self.history.go_forward().cloned() {
                    eprintln!("[DEBUG app] GoForward -> {:?}", route);
                    self.sidebar
                        .emit(SidebarInput::SetActiveRoute(route.clone()));
                    if route == Route::Home {
                        self.home_page.emit(HomeInput::Reload);
                    }
                }
            }
            AppInput::PlayMedia(id) => {
                eprintln!("[DEBUG app] PlayMedia -> id={}", id);
                let client = self.client.clone();
                sender.oneshot_command(async move {
                    let res = client.play_track(&id).await;
                    AppCmd::PlayResult(res)
                });
            }
            AppInput::TogglePlayback => {
                eprintln!("[DEBUG app] TogglePlayback");
                let client = self.client.clone();
                sender.oneshot_command(async move {
                    let res = client.toggle_play().await;
                    AppCmd::PlayResult(res)
                });
            }
        }
    }

    fn update_cmd(
        &mut self,
        message: Self::CommandOutput,
        sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match message {
            AppCmd::ConnectionStatusChanged(status) => {
                eprintln!("[DEBUG app] ConnectionStatusChanged: {:?}", status);
                self.connection_status = status;
                if status == ConnectionStatus::Connected {
                    let client = self.client.clone();
                    sender.oneshot_command(async move {
                        let status = client.get_status().await.ok();
                        AppCmd::InitialStatus(status)
                    });
                    self.home_page.emit(HomeInput::Reload);
                }
            }
            AppCmd::DaemonEvent(ev) => {
                eprintln!("[DEBUG app] DaemonEvent: {:?}", ev);
                if let ClientEvent::StatusChanged(status) = ev {
                    self.player_bar.emit(PlayerBarInput::StatusChanged(status));
                }
            }
            AppCmd::InitialStatus(status) => {
                eprintln!(
                    "[DEBUG app] InitialStatus: {:?}",
                    status
                        .as_ref()
                        .map(|s| (&s.state, s.current_track.as_ref().map(|t| t.title.clone())))
                );
                if let Some(s) = status {
                    self.player_bar.emit(PlayerBarInput::StatusChanged(s));
                }
            }
            AppCmd::PlayResult(Err(e)) => {
                eprintln!("[DEBUG app] PlayResult Error: {e}");
                tracing::warn!("Play request returned error: {e}");
            }
            AppCmd::PlayResult(Ok(())) => {
                eprintln!("[DEBUG app] PlayResult Success");
            }
        }
    }
}

impl MalusApp {
    fn active_route_name(&self) -> &'static str {
        match self.history.current() {
            Route::Home => "home",
            Route::Search => "home",
            Route::Albums => "home",
            Route::Songs => "home",
            Route::Playlists => "home",
        }
    }
}
