//! Root application window shell for Malus GTK.
//!
//! Enforces:
//! - Music first, controls second, app chrome third, engineering never
//! - Top HeaderBar with window controls, navigation, and sidebar toggle
//! - Persistent Bottom Player / Transport Bar
//! - Nested AdwOverlaySplitView architecture:
//!   - Outer: Left navigation sidebar (start position, toggleable, auto-collapsible on narrow windows)
//!   - Inner: Right utility pane (end position, mutually exclusive: Queue, Lyrics)
//! - Center: Dynamic page stack (Feed, Search, Now Playing, Settings)
//! - Native AdwToastOverlay for user feedback and error propagation
//! - Authoritative daemon event subscription & automatic reconnection

use malus_client::{ClientEvent, ConnectionStatus, MalusClient};
use malus_ipc::wire::PageActionWire;
use malus_model::{AccountMediaState, Lyrics, MediaRef, PageRoute, PlayerStatus, Queue};
use relm4::adw::{self, prelude::*};
use relm4::gtk;
use relm4::prelude::*;
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};

use crate::design::tokens::*;
use crate::dialogs::{
    show_add_to_playlist_dialog, show_delete_playlist_dialog, show_edit_playlist_dialog,
    show_new_playlist_dialog,
};
use crate::navigation::{AppDestination, NavigationHistory};
use crate::pages::feed::{FeedInput, FeedOutput, FeedPage};
use crate::pages::now_playing::NowPlayingPage;
use crate::pages::search::{SearchInput, SearchOutput, SearchPage};
use crate::pages::settings::{SettingsInput, SettingsPage};
use crate::panes::credits::show_credits_dialog;
use crate::services::{ArtworkService, DecodedImage};
use crate::shell::player_bar::PlayerBar;
use crate::shell::sidebar::{Sidebar, SidebarInput, SidebarOutput};
use crate::shell::utility_pane::UtilityPane;
use crate::state::{NowPlayingMode, PlayerCommand, PlayerPresentation, SharedPlayer, UtilityMode};
use crate::widgets::{
    ActionMenuCommand,
    player_controls::{CommandHandler, MenuHandler},
};

pub struct MalusApp {
    client: MalusClient,
    artwork_service: ArtworkService,
    history: NavigationHistory,
    utility_mode: UtilityMode,
    connection_status: ConnectionStatus,

    player: SharedPlayer,
    now_playing_mode: Option<NowPlayingMode>,
    now_playing_closing: bool,
    artwork_generation: u64,
    artwork_url: Option<String>,
    current_artwork: Option<std::sync::Arc<crate::services::DecodedImage>>,
    trim_counter: u32,
    volume_tx: tokio::sync::watch::Sender<Option<u8>>,
    tick: Option<gtk::glib::SourceId>,

    // Child Components
    player_bar: PlayerBar,
    sidebar: Controller<Sidebar>,
    utility_pane: UtilityPane,
    feed_page: Controller<FeedPage>,
    search_page: Controller<SearchPage>,
    now_playing_page: NowPlayingPage,
    settings_page: Controller<SettingsPage>,
}

#[derive(Debug)]
pub enum AppInput {
    Navigate(AppDestination),
    GoBack,
    GoForward,
    ToggleSidebar,
    ToggleQueue,
    ToggleLyrics,
    PlayMedia(MediaRef),
    PlayTrack {
        track: MediaRef,
        collection: Option<MediaRef>,
        index: Option<usize>,
    },
    PlayCollection {
        reference: MediaRef,
        shuffle: bool,
    },
    InvokeAction(PageActionWire),
    ViewCredits(MediaRef),
    OpenNowPlaying(NowPlayingMode),
    CloseNowPlaying,
    BrowseTransitionFinished,
    ToggleNowPlayingLyrics,
    ToggleNowPlayingQueue,
    Player(PlayerCommand),
    Tick,
    TogglePlayback,
    CloseUtility,
    Escape,
    FocusSearch,
    ShowToast(String),
    ShowAddToPlaylist(MediaRef),
    ShowNewPlaylistDialog {
        initial_track: Option<MediaRef>,
    },
    ShowEditPlaylistDialog {
        playlist_ref: MediaRef,
        current_name: String,
        current_desc: Option<String>,
    },
    ShowDeletePlaylistDialog {
        playlist_ref: MediaRef,
        playlist_title: String,
    },
    RemoveTrackFromPlaylist {
        playlist: MediaRef,
        track_index: usize,
        expected_track: MediaRef,
    },
    ReloadFeed,
    ReloadPlaylists,
    CopyLink(String),
}

#[derive(Debug)]
pub enum AppCmd {
    ConnectionStatusChanged(ConnectionStatus),
    DaemonEvent(ClientEvent),
    InitialStatus(Option<PlayerStatus>),
    InitialQueue(Option<Queue>),
    ShowToast(String),
    LyricsLoaded {
        generation: u64,
        result: Result<Lyrics, String>,
    },
    ArtworkLoaded {
        generation: u64,
        image: Option<Arc<DecodedImage>>,
    },
    MediaStateLoaded {
        generation: u64,
        state: Option<AccountMediaState>,
    },
    PlayerFinished(Result<(), String>),
    TrackEnriched {
        generation: u64,
        track: malus_model::Track,
    },
    TrackEnrichedFailed,
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
            set_default_size: (1180, 780),
            set_size_request: (560, 320),
            set_title: Some("Malus"),
            add_css_class: "malus-window",

            #[name(shell_stack)]
            gtk::Stack {
                set_transition_type: gtk::StackTransitionType::Crossfade,
                set_transition_duration: 220,
                set_hhomogeneous: false,
                set_vhomogeneous: false,
                add_named[Some("browse")] = &adw::ToolbarView {
                    // Top HeaderBar: Window controls & navigation
                    #[name(top_header_bar)]
                    add_top_bar = &adw::HeaderBar {
                        set_show_title: true,
                        #[wrap(Some)]
                        set_title_widget = &gtk::Label {
                            set_text: "Malus",
                            add_css_class: "navigation-title",
                        },

                        pack_start = &gtk::Button {
                            add_css_class: "flat",
                            set_icon_name: ICON_SIDEBAR_TOGGLE,
                            set_tooltip_text: Some("Toggle Sidebar (Ctrl+B)"),
                            set_focus_on_click: false,
                            connect_clicked => AppInput::ToggleSidebar,
                        },

                        pack_start = &gtk::Button {
                            add_css_class: "flat",
                            set_icon_name: ICON_BACK,
                            set_tooltip_text: Some("Back (Alt+Left)"),
                            set_focus_on_click: false,
                            #[watch]
                            set_sensitive: model.history.can_go_back(),
                            connect_clicked => AppInput::GoBack,
                        },

                        pack_start = &gtk::Button {
                            add_css_class: "flat",
                            set_icon_name: ICON_FORWARD,
                            set_tooltip_text: Some("Forward (Alt+Right)"),
                            set_focus_on_click: false,
                            #[watch]
                            set_sensitive: model.history.can_go_forward(),
                            connect_clicked => AppInput::GoForward,
                        },
                    },

                    add_top_bar = &adw::Banner {
                        add_css_class: "connection-banner",
                        #[watch]
                        set_revealed: model.connection_status != ConnectionStatus::Connected,
                        #[watch]
                        set_title: if model.connection_status == ConnectionStatus::Disconnected {
                            "Connection lost. Reconnecting to your music…"
                        } else { "Connecting to your music…" },
                    },

                    #[name(toast_overlay)]
                    #[wrap(Some)]
                    set_content = &adw::ToastOverlay {
                        // Outer Layout: Sidebar Split View
                        #[name(outer_split_view)]
                        #[wrap(Some)]
                        set_child = &adw::OverlaySplitView {
                            set_sidebar_position: gtk::PackType::Start,
                            set_min_sidebar_width: SIDEBAR_WIDTH_NORMAL,
                            set_max_sidebar_width: SIDEBAR_WIDTH_NORMAL,
                            set_sidebar_width_fraction: 0.20,
                            set_collapsed: false,
                            set_show_sidebar: true,

                            #[wrap(Some)]
                            #[local_ref]
                            set_sidebar = sidebar_widget -> gtk::Box {},

                            // Inner Layout: Utility Pane Split View
                            #[name(inner_split_view)]
                            #[wrap(Some)]
                            set_content = &adw::OverlaySplitView {
                                set_sidebar_position: gtk::PackType::End,
                                set_min_sidebar_width: UTILITY_PANE_MIN_WIDTH,
                                set_max_sidebar_width: UTILITY_PANE_WIDTH,
                                set_collapsed: false,
                                set_show_sidebar: false,

                                #[wrap(Some)]
                                #[local_ref]
                                set_sidebar = utility_widget -> gtk::Box {},

                                // Main Content: Page Stack
                                #[name(content_stack)]
                                #[wrap(Some)]
                                set_content = &gtk::Stack {
                                    set_transition_type: gtk::StackTransitionType::Crossfade,
                                    set_hhomogeneous: false,
                                    set_vhomogeneous: false,
                                    set_vexpand: true,
                                    set_hexpand: true,
                                },
                            },
                        },
                    },

                    // Bottom Bar: Persistent Player Transport Bar
                    #[local_ref]
                    add_bottom_bar = player_widget -> gtk::Box {},
                },
                #[local_ref]
                add_named[Some("now_playing")] = now_playing_widget -> gtk::Overlay {},
            },
        }
    }

    fn init(
        client: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        if let Ok(w_str) = std::env::var("MALUS_WINDOW_WIDTH")
            && let Ok(w) = w_str.parse::<i32>()
        {
            let h = std::env::var("MALUS_WINDOW_HEIGHT")
                .ok()
                .and_then(|v| v.parse::<i32>().ok())
                .unwrap_or(780);
            root.set_default_size(w, h);
            root.set_size_request(w.min(560), h.min(320));
        }

        let artwork_service = ArtworkService::new();
        if let Some(settings) = gtk::Settings::default() {
            settings.set_gtk_primary_button_warps_slider(true);
        }

        let player = Rc::new(RefCell::new(PlayerPresentation::default()));
        let s = sender.clone();
        let command: CommandHandler = Rc::new(move |command| s.input(AppInput::Player(command)));
        let s = sender.clone();
        let menu: MenuHandler = Rc::new(move |action| {
            s.input(match action {
                ActionMenuCommand::Action(a) => AppInput::InvokeAction(a),
                ActionMenuCommand::ViewCredits(r) => AppInput::ViewCredits(r),
                ActionMenuCommand::AddToPlaylist(r) => AppInput::ShowAddToPlaylist(r),
                ActionMenuCommand::RemoveFromPlaylist {
                    playlist,
                    track_index,
                    expected_track,
                } => AppInput::RemoveTrackFromPlaylist {
                    playlist,
                    track_index,
                    expected_track,
                },
                ActionMenuCommand::Navigate(r) => AppInput::Navigate(AppDestination::Page(r)),
                ActionMenuCommand::CopyLink(u) => AppInput::CopyLink(u),
            })
        });
        let s_open = sender.clone();
        let s_lyrics = sender.clone();
        let s_queue = sender.clone();
        let player_bar = PlayerBar::new(
            &player,
            &command,
            &menu,
            move || s_open.input(AppInput::OpenNowPlaying(NowPlayingMode::Player)),
            move || s_lyrics.input(AppInput::ToggleLyrics),
            move || s_queue.input(AppInput::ToggleQueue),
        );

        // 2. Navigation Sidebar (Left)
        let sidebar = Sidebar::builder()
            .launch((AppDestination::Page(PageRoute::Home), client.clone()))
            .forward(sender.input_sender(), |out| match out {
                SidebarOutput::Navigate(dest) => AppInput::Navigate(dest),
            });

        let s = sender.clone();
        let s_close = sender.clone();
        let utility_pane = UtilityPane::new(
            client.clone(),
            artwork_service.clone(),
            &player,
            &command,
            move || s.input(AppInput::OpenNowPlaying(NowPlayingMode::Lyrics)),
            move || s_close.input(AppInput::CloseUtility),
        );

        // 4. Center Pages
        let feed_page = FeedPage::builder()
            .launch((client.clone(), artwork_service.clone(), PageRoute::Home))
            .forward(sender.input_sender(), |out| match out {
                FeedOutput::Navigate(r) => AppInput::Navigate(AppDestination::Page(r)),
                FeedOutput::Play(r) => AppInput::PlayMedia(r),
                FeedOutput::PlayTrack {
                    track,
                    collection,
                    index,
                } => AppInput::PlayTrack {
                    track,
                    collection,
                    index,
                },
                FeedOutput::PlayCollection { reference, shuffle } => {
                    AppInput::PlayCollection { reference, shuffle }
                }
                FeedOutput::Action(a) => AppInput::InvokeAction(a),
                FeedOutput::ViewCredits(r) => AppInput::ViewCredits(r),
                FeedOutput::ShowAddToPlaylist(r) => AppInput::ShowAddToPlaylist(r),
                FeedOutput::ShowNewPlaylistDialog { initial_track } => {
                    AppInput::ShowNewPlaylistDialog { initial_track }
                }
                FeedOutput::ShowEditPlaylistDialog {
                    playlist_ref,
                    current_name,
                    current_desc,
                } => AppInput::ShowEditPlaylistDialog {
                    playlist_ref,
                    current_name,
                    current_desc,
                },
                FeedOutput::ShowDeletePlaylistDialog {
                    playlist_ref,
                    playlist_title,
                } => AppInput::ShowDeletePlaylistDialog {
                    playlist_ref,
                    playlist_title,
                },
                FeedOutput::RemoveTrackFromPlaylist {
                    playlist,
                    track_index,
                    expected_track,
                } => AppInput::RemoveTrackFromPlaylist {
                    playlist,
                    track_index,
                    expected_track,
                },
                FeedOutput::CopyLink(url) => AppInput::CopyLink(url),
            });

        let search_page = SearchPage::builder()
            .launch((client.clone(), artwork_service.clone()))
            .forward(sender.input_sender(), |out| match out {
                SearchOutput::Navigate(r) => AppInput::Navigate(AppDestination::Page(r)),
                SearchOutput::Play(r) => AppInput::PlayMedia(r),
                SearchOutput::Action(a) => AppInput::InvokeAction(a),
                SearchOutput::ViewCredits(r) => AppInput::ViewCredits(r),
                SearchOutput::ShowAddToPlaylist(r) => AppInput::ShowAddToPlaylist(r),
                SearchOutput::CopyLink(url) => AppInput::CopyLink(url),
            });

        let s_close = sender.clone();
        let s_lyrics = sender.clone();
        let s_queue = sender.clone();
        let s_player = sender.clone();
        let s_nav = sender.clone();
        let now_playing_page = NowPlayingPage::new(
            client.clone(),
            artwork_service.clone(),
            &player,
            &command,
            &menu,
            move || s_close.input(AppInput::CloseNowPlaying),
            move || s_lyrics.input(AppInput::ToggleNowPlayingLyrics),
            move || s_queue.input(AppInput::ToggleNowPlayingQueue),
            move || s_player.input(AppInput::OpenNowPlaying(NowPlayingMode::Player)),
            move |dest| s_nav.input(AppInput::Navigate(dest)),
        );
        let (volume_tx, mut volume_rx) = tokio::sync::watch::channel(None::<u8>);
        let c = client.clone();
        sender.command(move |out, shutdown| {
            shutdown
                .register(async move {
                    // A single in-flight volume command; latest drag value replaces any
                    // queued one while a slow IPC/MusicKit acknowledgement is pending.
                    while volume_rx.changed().await.is_ok() {
                        let volume = *volume_rx.borrow_and_update();
                        if let Some(volume) = volume {
                            let result = c.set_volume(volume).await.map_err(|e| e.to_string());
                            let _ = out.send(AppCmd::PlayerFinished(result));
                        }
                    }
                })
                .drop_on_shutdown()
        });

        let settings_page = SettingsPage::builder().launch(client.clone()).detach();

        let mut model = Self {
            client: client.clone(),
            artwork_service,
            player,
            now_playing_mode: None,
            now_playing_closing: false,
            artwork_generation: 0,
            artwork_url: None,
            current_artwork: None,
            trim_counter: 0,
            volume_tx,
            tick: None,
            history: NavigationHistory::default(),
            utility_mode: UtilityMode::Closed,
            connection_status: ConnectionStatus::Connecting,
            player_bar,
            sidebar,
            utility_pane,
            feed_page,
            search_page,
            now_playing_page,
            settings_page,
        };

        model.refresh_player();

        // Subscribe to authoritative background daemon events
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

        // Initial fetch of player status & queue
        let c_status = client.clone();
        sender.oneshot_command(async move {
            let status = c_status.get_status().await.ok();
            AppCmd::InitialStatus(status)
        });

        let c_queue = client;
        sender.oneshot_command(async move {
            let queue = c_queue.get_queue().await.ok();
            AppCmd::InitialQueue(queue)
        });

        let s = sender.clone();
        model.tick = Some(gtk::glib::timeout_add_local(
            Duration::from_millis(100),
            move || {
                s.input(AppInput::Tick);
                gtk::glib::ControlFlow::Continue
            },
        ));
        let player_widget = &model.player_bar.root;
        let now_playing_widget = &model.now_playing_page.root;
        let sidebar_widget = model.sidebar.widget();
        let utility_widget = &model.utility_pane.root;

        let widgets = view_output!();

        widgets
            .content_stack
            .add_named(model.feed_page.widget(), Some("feed"));
        widgets
            .content_stack
            .add_named(model.search_page.widget(), Some("search"));
        widgets
            .content_stack
            .add_named(model.settings_page.widget(), Some("settings"));
        widgets.content_stack.set_visible_child_name("feed");
        widgets.shell_stack.set_visible_child_name("browse");
        let completed = sender.clone();
        widgets
            .shell_stack
            .connect_transition_running_notify(move |stack| {
                if !stack.is_transition_running()
                    && stack.visible_child_name().as_deref() == Some("browse")
                {
                    completed.input(AppInput::BrowseTransitionFinished);
                }
            });

        // Responsive breakpoint to collapse utility pane (lyrics/queue) to an overlay when window width <= 1040px
        let utility_breakpoint = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            1040.0,
            adw::LengthUnit::Px,
        ));
        utility_breakpoint.add_setter(
            &widgets.inner_split_view,
            "collapsed",
            Some(&true.to_value()),
        );
        root.add_breakpoint(utility_breakpoint);

        // Responsive breakpoint to collapse navigation sidebar when the window becomes narrow.
        let nav_breakpoint = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            BREAKPOINT_COLLAPSE_NAVIGATION,
            adw::LengthUnit::Px,
        ));
        nav_breakpoint.add_setter(
            &widgets.outer_split_view,
            "collapsed",
            Some(&true.to_value()),
        );
        nav_breakpoint.add_setter(
            &widgets.outer_split_view,
            "show-sidebar",
            Some(&false.to_value()),
        );
        nav_breakpoint.add_setter(
            &widgets.inner_split_view,
            "collapsed",
            Some(&true.to_value()),
        );
        root.add_breakpoint(nav_breakpoint);

        let s_utility = sender.clone();
        widgets
            .inner_split_view
            .connect_show_sidebar_notify(move |view| {
                if !view.shows_sidebar() {
                    s_utility.input(AppInput::CloseUtility);
                }
            });

        if let Ok(w_str) = std::env::var("MALUS_WINDOW_WIDTH")
            && let Ok(w) = w_str.parse::<i32>()
            && let Ok(h_str) = std::env::var("MALUS_WINDOW_HEIGHT")
            && let Ok(h) = h_str.parse::<i32>()
        {
            root.set_default_size(w, h);
        }

        if let Ok(route_str) = std::env::var("MALUS_INITIAL_ROUTE") {
            if let Some(route) = PageRoute::parse(&route_str) {
                sender.input(AppInput::Navigate(AppDestination::Page(route)));
            } else if route_str == "settings" {
                sender.input(AppInput::Navigate(AppDestination::Settings));
            } else if route_str == "nowplaying" {
                sender.input(AppInput::OpenNowPlaying(NowPlayingMode::Player));
            } else if route_str == "nowplaying:lyrics" {
                sender.input(AppInput::OpenNowPlaying(NowPlayingMode::Lyrics));
            } else if route_str == "nowplaying:queue" {
                sender.input(AppInput::OpenNowPlaying(NowPlayingMode::Queue));
            } else if let Some(q) = route_str.strip_prefix("search:") {
                sender.input(AppInput::Navigate(AppDestination::Search(q.to_string())));
            }
        }

        if let Ok(pane_str) = std::env::var("MALUS_INITIAL_PANE") {
            let s_pane = sender.clone();
            root.connect_map(move |_| {
                let s = s_pane.clone();
                let p = pane_str.clone();
                relm4::gtk::glib::idle_add_local_once(move || {
                    if p == "queue" {
                        s.input(AppInput::ToggleQueue);
                    } else if p == "lyrics" {
                        s.input(AppInput::ToggleLyrics);
                    }
                });
            });
        }

        if let Ok(sb_str) = std::env::var("MALUS_INITIAL_SIDEBAR")
            && (sb_str == "1" || sb_str == "true")
        {
            widgets.outer_split_view.set_show_sidebar(true);
        }

        // Global keyboard shortcuts with input-focus safety
        let event_ctrl = gtk::EventControllerKey::new();
        event_ctrl.set_propagation_phase(gtk::PropagationPhase::Capture);
        let s_key = sender.clone();
        let r_key = root.clone();
        event_ctrl.connect_key_pressed(move |_ctrl, keyval, _code, modifier| {
            let focus = gtk::prelude::GtkWindowExt::focus(&r_key);
            let is_editing = focus
                .as_ref()
                .map(|w| w.is::<gtk::Editable>() || w.is::<gtk::TextView>() || w.is::<gtk::Text>())
                .unwrap_or(false);

            let k = keyval.to_lower();
            if modifier.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
                if k == gtk::gdk::Key::b {
                    s_key.input(AppInput::ToggleSidebar);
                    return gtk::glib::Propagation::Stop;
                } else if k == gtk::gdk::Key::q || k == gtk::gdk::Key::u {
                    s_key.input(AppInput::ToggleQueue);
                    return gtk::glib::Propagation::Stop;
                } else if k == gtk::gdk::Key::l {
                    s_key.input(AppInput::ToggleLyrics);
                    return gtk::glib::Propagation::Stop;
                } else if k == gtk::gdk::Key::f || k == gtk::gdk::Key::k {
                    s_key.input(AppInput::FocusSearch);
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
            } else if keyval == gtk::gdk::Key::AudioPlay || keyval == gtk::gdk::Key::AudioPause {
                s_key.input(AppInput::TogglePlayback);
                return gtk::glib::Propagation::Stop;
            } else if keyval == gtk::gdk::Key::AudioNext {
                s_key.input(AppInput::Player(PlayerCommand::Next));
                return gtk::glib::Propagation::Stop;
            } else if keyval == gtk::gdk::Key::AudioPrev {
                s_key.input(AppInput::Player(PlayerCommand::Previous));
                return gtk::glib::Propagation::Stop;
            } else if keyval == gtk::gdk::Key::AudioStop {
                s_key.input(AppInput::Player(PlayerCommand::Pause));
                return gtk::glib::Propagation::Stop;
            } else if !is_editing && keyval == gtk::gdk::Key::space {
                s_key.input(AppInput::TogglePlayback);
                return gtk::glib::Propagation::Stop;
            } else if keyval == gtk::gdk::Key::Escape {
                s_key.input(AppInput::Escape);
                return gtk::glib::Propagation::Stop;
            }
            gtk::glib::Propagation::Proceed
        });
        root.add_controller(event_ctrl);

        // GTK4 native ShortcutController for accelerator robustness across XKB maps / CapsLock / NumLock
        let shortcut_ctrl = gtk::ShortcutController::new();
        shortcut_ctrl.set_scope(gtk::ShortcutScope::Local);

        let add_shortcut = |accel: &str, create_input: Box<dyn Fn() -> AppInput>| {
            if let Some(trigger) = gtk::ShortcutTrigger::parse_string(accel) {
                let s = sender.clone();
                let action = gtk::CallbackAction::new(move |_, _| {
                    s.input(create_input());
                    gtk::glib::Propagation::Stop
                });
                shortcut_ctrl.add_shortcut(gtk::Shortcut::new(Some(trigger), Some(action)));
            }
        };

        add_shortcut("<Control>b", Box::new(|| AppInput::ToggleSidebar));
        add_shortcut("<Control>q", Box::new(|| AppInput::ToggleQueue));
        add_shortcut("<Control>u", Box::new(|| AppInput::ToggleQueue));
        add_shortcut("<Control>l", Box::new(|| AppInput::ToggleLyrics));
        add_shortcut("<Control>f", Box::new(|| AppInput::FocusSearch));
        add_shortcut("<Control>k", Box::new(|| AppInput::FocusSearch));
        root.add_controller(shortcut_ctrl);

        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        message: Self::Input,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        match message {
            AppInput::Navigate(dest) => {
                if self.now_playing_mode.take().is_some() {
                    self.now_playing_closing = true;
                    self.player_bar.refresh(&self.player, self.utility_mode);
                }
                widgets.shell_stack.set_visible_child_name("browse");
                let searching = dest.is_search();
                self.history.navigate_to(dest.clone());
                self.show_destination(dest, widgets);
                if widgets.outer_split_view.is_collapsed() && !searching {
                    widgets.outer_split_view.set_show_sidebar(false);
                }
            }
            AppInput::GoBack => {
                if self.now_playing_mode.is_some() {
                    sender.input(AppInput::CloseNowPlaying);
                } else if let Some(dest) = self.history.go_back().cloned() {
                    self.show_destination(dest, widgets);
                } else {
                    sender.input(AppInput::CloseUtility);
                }
            }
            AppInput::GoForward => {
                if self.now_playing_mode.is_none()
                    && let Some(dest) = self.history.go_forward().cloned()
                {
                    self.show_destination(dest, widgets);
                }
            }
            AppInput::ToggleSidebar => {
                let show = !widgets.outer_split_view.shows_sidebar();
                widgets.outer_split_view.set_show_sidebar(show);
            }
            AppInput::ToggleQueue => {
                if self.now_playing_mode.is_some() {
                    sender.input(AppInput::ToggleNowPlayingQueue);
                } else {
                    self.utility_mode.toggle_queue();
                    self.sync_utility(widgets);
                }
            }
            AppInput::ToggleLyrics => {
                if self.now_playing_mode.is_some() {
                    sender.input(AppInput::ToggleNowPlayingLyrics);
                } else {
                    self.utility_mode.toggle_lyrics();
                    self.sync_utility(widgets);
                }
            }
            AppInput::Escape => {
                sender.input(if self.now_playing_mode.is_some() {
                    AppInput::CloseNowPlaying
                } else {
                    AppInput::CloseUtility
                });
            }
            AppInput::CloseUtility => {
                self.utility_mode = UtilityMode::Closed;
                self.sync_utility(widgets);
            }
            AppInput::FocusSearch => {
                if self.now_playing_mode.take().is_some() {
                    self.now_playing_closing = true;
                    self.player_bar.refresh(&self.player, self.utility_mode);
                }
                widgets.shell_stack.set_visible_child_name("browse");
                widgets.outer_split_view.set_show_sidebar(true);
                if self.history.current() == &AppDestination::Page(PageRoute::Search) {
                    self.search_page.emit(SearchInput::FocusSearch);
                } else {
                    sender.input(AppInput::Navigate(AppDestination::Page(PageRoute::Search)));
                }
            }
            AppInput::ShowToast(message) => {
                widgets.toast_overlay.add_toast(adw::Toast::new(&message));
            }
            AppInput::PlayCollection { reference, shuffle } => {
                let client = self.client.clone();
                sender.oneshot_command(async move {
                    AppCmd::PlayerFinished(
                        client
                            .play_collection(&reference, shuffle)
                            .await
                            .map_err(|e| e.to_string()),
                    )
                });
            }
            AppInput::PlayMedia(media_ref) => {
                let c = self.client.clone();
                let s = sender.clone();
                relm4::spawn(async move {
                    if let Err(e) = c.play_media(&media_ref).await {
                        s.input(AppInput::ShowToast(format!("Playback failed: {e}")));
                    }
                });
                self.now_playing_page.reload_queue();
                self.utility_pane.reload_queue();
            }
            AppInput::PlayTrack {
                track,
                collection,
                index,
            } => {
                let c = self.client.clone();
                let s = sender.clone();
                relm4::spawn(async move {
                    if let Err(e) = c
                        .play_media_with_context(&track, collection.as_ref(), index)
                        .await
                    {
                        s.input(AppInput::ShowToast(format!("Playback failed: {e}")));
                    }
                });
                self.now_playing_page.reload_queue();
                self.utility_pane.reload_queue();
            }
            AppInput::ReloadPlaylists => {
                self.sidebar.emit(SidebarInput::ReloadPlaylists);
            }
            AppInput::InvokeAction(action) => {
                let is_playlist_mutation = matches!(
                    &action,
                    PageActionWire::AddToLibrary(MediaRef::Playlist(_))
                        | PageActionWire::RemoveFromLibrary(MediaRef::Playlist(_))
                );
                let c = self.client.clone();
                let s = sender.clone();
                relm4::spawn(async move {
                    match c.invoke_action(action).await {
                        Ok(res) => {
                            if let Some(msg) = res.message {
                                s.input(AppInput::ShowToast(msg));
                            }
                            if is_playlist_mutation {
                                s.input(AppInput::ReloadPlaylists);
                                s.input(AppInput::ReloadFeed);
                            }
                        }
                        Err(e) => {
                            s.input(AppInput::ShowToast(format!("Action failed: {e}")));
                        }
                    }
                });
            }
            AppInput::ViewCredits(track_ref) => {
                let title = self
                    .player
                    .borrow()
                    .now
                    .current_track
                    .as_ref()
                    .filter(|track| track.id == track_ref)
                    .map(|track| track.title.clone());
                show_credits_dialog(
                    root,
                    &self.client,
                    &track_ref,
                    title.as_deref().unwrap_or("Song Credits"),
                );
            }
            AppInput::ShowAddToPlaylist(track_ref) => {
                let s = sender.clone();
                show_add_to_playlist_dialog(root, &self.client, &track_ref, move |msg| {
                    s.input(AppInput::ShowToast(msg));
                });
            }
            AppInput::ShowNewPlaylistDialog { initial_track } => {
                let s = sender.clone();
                show_new_playlist_dialog(
                    root,
                    &self.client,
                    initial_track.into_iter().collect(),
                    move |pl| {
                        s.input(AppInput::ShowToast(format!(
                            "Created playlist “{}”",
                            pl.title
                        )));
                        s.input(AppInput::ReloadPlaylists);
                        s.input(AppInput::Navigate(AppDestination::Page(
                            PageRoute::Playlist(pl.id.id().to_string()),
                        )));
                    },
                );
            }
            AppInput::ShowEditPlaylistDialog {
                playlist_ref,
                current_name,
                current_desc,
            } => {
                let s = sender.clone();
                show_edit_playlist_dialog(
                    root,
                    &self.client,
                    &playlist_ref,
                    &current_name,
                    current_desc.as_deref(),
                    move |name, _desc| {
                        s.input(AppInput::ShowToast(format!("Updated “{name}”")));
                        s.input(AppInput::ReloadPlaylists);
                        s.input(AppInput::ReloadFeed);
                    },
                );
            }
            AppInput::ShowDeletePlaylistDialog {
                playlist_ref,
                playlist_title,
            } => {
                let s = sender.clone();
                show_delete_playlist_dialog(
                    root,
                    &self.client,
                    &playlist_ref,
                    &playlist_title,
                    move || {
                        s.input(AppInput::ShowToast("Deleted playlist".to_string()));
                        s.input(AppInput::ReloadPlaylists);
                        s.input(AppInput::Navigate(AppDestination::Page(
                            PageRoute::LibraryPlaylists,
                        )));
                    },
                );
            }
            AppInput::RemoveTrackFromPlaylist {
                playlist,
                track_index,
                expected_track,
            } => {
                let c = self.client.clone();
                let s = sender.clone();
                relm4::gtk::glib::spawn_future_local(async move {
                    match c
                        .remove_track_from_playlist(&playlist, track_index, &expected_track)
                        .await
                    {
                        Ok(()) => {
                            s.input(AppInput::ShowToast("Removed from playlist".to_string()));
                            s.input(AppInput::ReloadFeed);
                        }
                        Err(e) => {
                            s.input(AppInput::ShowToast(format!("Failed to remove: {e}")));
                        }
                    }
                });
            }
            AppInput::ReloadFeed => {
                self.feed_page.emit(FeedInput::Reload);
            }
            AppInput::OpenNowPlaying(mode) => {
                self.now_playing_closing = false;
                self.now_playing_mode = Some(mode);
                self.now_playing_page.set_mode(mode);
                self.now_playing_page.refresh(&self.player);
                self.now_playing_page
                    .set_artwork(self.current_artwork.as_deref());
                widgets.shell_stack.set_visible_child_name("now_playing");
                if mode == NowPlayingMode::Queue {
                    self.now_playing_page.reload_queue();
                    self.now_playing_page.scroll_to_now_playing();
                }
            }
            AppInput::CloseNowPlaying => {
                self.now_playing_mode = None;
                self.now_playing_closing = true;
                self.player_bar.refresh(&self.player, self.utility_mode);
                widgets.shell_stack.set_visible_child_name("browse");
            }
            AppInput::BrowseTransitionFinished => {
                if self.now_playing_closing {
                    self.now_playing_closing = false;
                    self.now_playing_page.clear_backdrops();
                }
            }
            AppInput::ToggleNowPlayingLyrics => {
                let mode = if self.now_playing_mode == Some(NowPlayingMode::Lyrics) {
                    NowPlayingMode::Player
                } else {
                    NowPlayingMode::Lyrics
                };
                sender.input(AppInput::OpenNowPlaying(mode));
            }
            AppInput::ToggleNowPlayingQueue => {
                let mode = if self.now_playing_mode == Some(NowPlayingMode::Queue) {
                    NowPlayingMode::Player
                } else {
                    NowPlayingMode::Queue
                };
                sender.input(AppInput::OpenNowPlaying(mode));
            }
            AppInput::Tick => {
                let mut p = self.player.borrow_mut();
                let position = p.now.extrapolated_position_ms();
                p.lyrics.update_position(position);
                drop(p);
                if self.now_playing_mode.is_some() {
                    self.now_playing_page.tick(&self.player);
                } else {
                    self.player_bar.tick(&self.player);
                    if self.utility_mode.is_lyrics() {
                        self.utility_pane.lyrics.view.refresh();
                    }
                }

                // Trim freed allocator memory at 5s, 10s (post-settling), and periodically during idle/playback
                self.trim_counter = self.trim_counter.wrapping_add(1);
                if self.trim_counter == 50
                    || self.trim_counter == 100
                    || (self.trim_counter > 100 && self.trim_counter.is_multiple_of(300))
                {
                    crate::trim_memory();
                }
                return;
            }
            AppInput::Player(command) => self.dispatch_player(command, &sender),
            AppInput::TogglePlayback => {
                let c = self.client.clone();
                let s = sender.clone();
                relm4::spawn(async move {
                    if let Err(e) = c.toggle_play().await {
                        s.input(AppInput::ShowToast(format!("Playback error: {e}")));
                    }
                });
            }
            AppInput::CopyLink(url) => {
                widgets.top_header_bar.clipboard().set_text(&url);
                sender.input(AppInput::ShowToast("Link copied to clipboard".to_string()));
            }
        }
        self.update_view(widgets, sender);
    }

    fn update_cmd(
        &mut self,
        message: Self::CommandOutput,
        sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match message {
            AppCmd::ConnectionStatusChanged(status) => {
                let was_disconnected = self.connection_status == ConnectionStatus::Disconnected;
                let was_not_connected = self.connection_status != ConnectionStatus::Connected;
                self.connection_status = status;
                if status == ConnectionStatus::Connected {
                    if was_disconnected {
                        sender.input(AppInput::ShowToast("Connected to Malus daemon".to_string()));
                    }
                    if was_not_connected {
                        self.feed_page.emit(FeedInput::Reload);
                        self.sidebar.emit(SidebarInput::ReloadPlaylists);
                        if let AppDestination::Search(query) = self.history.current() {
                            self.search_page
                                .emit(SearchInput::ExecuteSearch(query.clone()));
                        }
                    }
                    let c = self.client.clone();
                    sender.oneshot_command(async move {
                        let st = c.get_status().await.ok();
                        AppCmd::InitialStatus(st)
                    });
                    let c2 = self.client.clone();
                    sender.oneshot_command(async move {
                        let q = c2.get_queue().await.ok();
                        AppCmd::InitialQueue(q)
                    });
                } else if status == ConnectionStatus::Disconnected {
                    sender.input(AppInput::ShowToast(
                        "Disconnected from Malus daemon. Retrying...".to_string(),
                    ));
                }
            }
            AppCmd::DaemonEvent(ev) => match ev {
                ClientEvent::StatusChanged(status) => self.receive_status(status, &sender),
                ClientEvent::QueueChanged(queue) => {
                    self.now_playing_page.set_queue(queue.clone());
                    self.utility_pane.set_queue(queue);
                }
                ClientEvent::MediaStateChanged(media_state) => {
                    self.feed_page
                        .emit(FeedInput::MediaState(media_state.clone()));
                    self.search_page
                        .emit(SearchInput::MediaState(media_state.clone()));
                    self.player
                        .borrow_mut()
                        .now
                        .update_media_state(&media_state);
                    self.refresh_player();
                    if matches!(media_state.reference, MediaRef::Playlist(_)) {
                        self.sidebar.emit(SidebarInput::ReloadPlaylists);
                    }
                }
                ClientEvent::AuthChanged(_) => {
                    self.settings_page.emit(SettingsInput::Reload);
                    self.sidebar.emit(SidebarInput::ReloadPlaylists);
                    self.feed_page.emit(FeedInput::Reload);
                }
                ClientEvent::PlaybackError { source, message } => {
                    sender.input(AppInput::ShowToast(format!("{source}: {message}")));
                }
                ClientEvent::TrackChanged(_) => {}
            },
            AppCmd::InitialStatus(status) => {
                if let Some(status) = status {
                    self.receive_status(status, &sender);
                }
            }
            AppCmd::InitialQueue(queue) => {
                if let Some(queue) = queue {
                    self.now_playing_page.set_queue(queue.clone());
                    self.utility_pane.set_queue(queue);
                }
            }
            AppCmd::ArtworkLoaded { generation, image } => {
                if generation == self.artwork_generation {
                    self.current_artwork = image.clone();
                    self.player_bar.set_artwork(image.as_deref());
                    if self.now_playing_mode.is_some() {
                        self.now_playing_page.set_artwork(image.as_deref());
                    }
                }
            }
            AppCmd::LyricsLoaded { generation, result } => {
                let mut p = self.player.borrow_mut();
                if generation == p.lyrics.generation {
                    p.lyrics.loading = false;
                    match result {
                        Ok(lyrics) => p.lyrics.content = Some(lyrics),
                        Err(e) => p.lyrics.error = Some(e),
                    }
                    let position = p.now.extrapolated_position_ms();
                    p.lyrics.update_position(position);
                }
                drop(p);
                self.refresh_player();
                self.utility_pane.lyrics.view.refresh();
            }
            AppCmd::MediaStateLoaded { generation, state } => {
                if generation == self.player.borrow().lyrics.generation
                    && let Some(state) = state
                {
                    self.feed_page.emit(FeedInput::MediaState(state.clone()));
                    self.search_page
                        .emit(SearchInput::MediaState(state.clone()));
                    self.player.borrow_mut().now.update_media_state(&state);
                    self.refresh_player();
                }
            }
            AppCmd::PlayerFinished(result) => {
                if let Err(error) = result {
                    self.player_bar.cancel_interactions();
                    self.now_playing_page.cancel_interactions();
                    sender.input(AppInput::ShowToast(format!("Playback failed: {error}")));
                }
            }
            AppCmd::TrackEnriched { generation, track } => {
                let mut p = self.player.borrow_mut();
                if generation == p.lyrics.generation
                    && let Some(cur) = p.now.current_track.as_mut()
                    && cur.id == track.id
                {
                    if cur.album.as_ref().and_then(|a| a.id.as_ref()).is_none()
                        && let Some(alb) = track.album
                    {
                        cur.album = Some(alb);
                    }
                    if !track.artists.is_empty() && cur.artists.iter().any(|a| a.id.is_none()) {
                        cur.artists = track.artists;
                    }
                    drop(p);
                    self.refresh_player();
                }
            }
            AppCmd::TrackEnrichedFailed => {}
            AppCmd::ShowToast(msg) => {
                sender.input(AppInput::ShowToast(msg));
            }
        }
    }
}

impl Drop for MalusApp {
    fn drop(&mut self) {
        if let Some(source) = self.tick.take() {
            source.remove();
        }
    }
}

impl MalusApp {
    fn show_destination(&self, destination: AppDestination, widgets: &MalusAppWidgets) {
        self.sidebar
            .emit(SidebarInput::SetActive(destination.clone()));
        match destination {
            AppDestination::Page(route) => {
                if route == PageRoute::Search {
                    self.search_page.emit(SearchInput::ShowLanding);
                    self.search_page.emit(SearchInput::FocusSearch);
                    widgets.content_stack.set_visible_child_name("search");
                } else {
                    self.feed_page.emit(FeedInput::LoadRoute(route));
                    widgets.content_stack.set_visible_child_name("feed");
                }
            }
            AppDestination::Search(query) => {
                self.search_page.emit(SearchInput::ExecuteSearch(query));
                self.search_page.emit(SearchInput::FocusSearch);
                widgets.content_stack.set_visible_child_name("search");
            }
            AppDestination::Settings => {
                self.settings_page.emit(SettingsInput::Reload);
                widgets.content_stack.set_visible_child_name("settings");
            }
        }
    }
    fn sync_utility(&self, widgets: &MalusAppWidgets) {
        widgets
            .inner_split_view
            .set_show_sidebar(self.utility_mode.is_open());
        self.utility_pane.set_mode(self.utility_mode);
        if self.utility_mode == UtilityMode::Queue {
            self.utility_pane.reload_queue();
            self.utility_pane.scroll_to_now_playing();
        } else if self.utility_mode == UtilityMode::Lyrics {
            self.utility_pane.lyrics.view.refresh();
        }
        self.player_bar.refresh(&self.player, self.utility_mode);
    }
    fn refresh_player(&self) {
        self.player_bar.refresh(&self.player, self.utility_mode);
        self.now_playing_page.refresh(&self.player);
    }
    fn receive_status(&mut self, status: PlayerStatus, sender: &ComponentSender<Self>) {
        let mut p = self.player.borrow_mut();
        let changed = p.now.current_track.as_ref().map(|t| &t.id)
            != status.current_track.as_ref().map(|t| &t.id);
        p.now.update_from_status(&status);
        let url = p
            .now
            .current_track
            .as_ref()
            .and_then(|t| t.artwork.as_ref())
            .map(|a| a.url.clone());
        if changed {
            self.utility_pane.reload_queue();
            self.utility_pane.scroll_to_now_playing();
            p.lyrics.generation = p.lyrics.generation.wrapping_add(1);
            p.lyrics.content = None;
            p.lyrics.active = None;
            p.lyrics.error = None;
            p.lyrics.loading = p.now.current_track.is_some();
            let generation = p.lyrics.generation;
            if let Some(track) = &p.now.current_track {
                let id = track.id.clone();
                let c = self.client.clone();
                sender.oneshot_command(async move {
                    AppCmd::LyricsLoaded {
                        generation,
                        result: c.get_lyrics(&id).await.map_err(|e| e.to_string()),
                    }
                });
                let id = track.id.clone();
                let c = self.client.clone();
                sender.oneshot_command(async move {
                    AppCmd::MediaStateLoaded {
                        generation,
                        state: c.get_media_state(&id).await.ok(),
                    }
                });
                let needs_enrichment = track.album.as_ref().and_then(|a| a.id.as_ref()).is_none()
                    || track.artists.iter().any(|a| a.id.is_none());
                if needs_enrichment {
                    let id = track.id.clone();
                    let c = self.client.clone();
                    sender.oneshot_command(async move {
                        match c.get_catalog_item(&id).await {
                            Ok(malus_ipc::wire::CatalogItemWire::Track(enriched)) => {
                                AppCmd::TrackEnriched {
                                    generation,
                                    track: enriched,
                                }
                            }
                            _ => AppCmd::TrackEnrichedFailed,
                        }
                    });
                }
            }
        }
        let position = p.now.extrapolated_position_ms();
        p.lyrics.update_position(position);
        drop(p);
        if changed || self.artwork_url != url {
            self.artwork_url = url.clone();
            self.artwork_generation = self.artwork_generation.wrapping_add(1);
            let generation = self.artwork_generation;
            let service = self.artwork_service.clone();
            // Clear sharp art promptly; retain the old atmosphere until the next
            // decode finishes, then crossfade both updates without a blank flash.
            self.player_bar.set_artwork(None);
            sender.oneshot_command(async move {
                AppCmd::ArtworkLoaded {
                    generation,
                    image: match url {
                        Some(url) => service.load(&url).await,
                        None => None,
                    },
                }
            });
        }
        self.now_playing_page.set_playback_state(status.state);
        self.now_playing_page.set_autoplay(status.autoplay);
        self.utility_pane.set_playback_state(status.state);
        self.utility_pane.set_autoplay(status.autoplay);
        self.refresh_player();
        self.utility_pane.lyrics.view.refresh();
    }
    fn dispatch_player(&self, command: PlayerCommand, sender: &ComponentSender<Self>) {
        if let PlayerCommand::Volume(volume) = command {
            let _ = self.volume_tx.send(Some(volume));
            return;
        }
        if let PlayerCommand::Seek { ref track, .. } = command
            && self
                .player
                .borrow()
                .now
                .current_track
                .as_ref()
                .map(|t| &t.id)
                != Some(track)
        {
            return;
        }
        let c = self.client.clone();
        sender.oneshot_command(async move {
            let result = match command {
                PlayerCommand::TogglePlay => c.toggle_play().await,
                PlayerCommand::Pause => c.pause().await,
                PlayerCommand::Previous => c.previous().await,
                PlayerCommand::Next => c.next().await,
                PlayerCommand::Shuffle(value) => c.set_shuffle(value).await,
                PlayerCommand::Repeat(value) => c.set_repeat(value).await,
                PlayerCommand::Autoplay(value) => c.set_autoplay(value).await,
                PlayerCommand::Seek { position_ms, .. } => c.seek(position_ms).await,
                PlayerCommand::Volume(_) => unreachable!(),
            };
            AppCmd::PlayerFinished(result.map_err(|e| e.to_string()))
        });
    }
}
