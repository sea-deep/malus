//! Malus Greenfield Consumer Music Player GUI powered by Relm4 & GTK4.

pub mod app;
pub mod design;
pub mod dialogs;
pub mod model;
pub mod navigation;
pub mod pages;
pub mod panes;
pub mod services;
pub mod shell;
pub mod state;
pub mod widgets;

use malus_client::{MalusClient, default_socket_path};
use relm4::RelmApp;

pub const APP_ID: &str = "com.sea_deep.malus";
pub const CSS_STYLE: &str = include_str!("../resources/style.css");
pub const LYRICS_QUOTE_SVG: &str = include_str!("../resources/icons/lyrics-quote-symbolic.svg");
pub const REPLAY_SVG: &str = include_str!("../resources/icons/replay-symbolic.svg");
pub const STAR_OUTLINE_SVG: &str = include_str!("../resources/icons/star-outline-symbolic.svg");
pub const SIDEBAR_SHOW_SVG: &str = include_str!("../resources/icons/sidebar-show-symbolic.svg");
pub const MALUS_LOGO_SVG: &str = include_str!("../resources/icons/malus-logo.svg");
pub const MALUS_LOGO_MONO_SVG: &str = include_str!("../resources/icons/malus-logo-mono.svg");
pub const MALUS_LOGO_SYMBOLIC_SVG: &str =
    include_str!("../resources/icons/malus-logo-symbolic.svg");

fn init_custom_icons() {
    let icons_dir = std::env::temp_dir().join("malus-icons");
    let _ = std::fs::create_dir_all(&icons_dir);
    let quote_path = icons_dir.join("lyrics-quote-symbolic.svg");
    let _ = std::fs::write(&quote_path, LYRICS_QUOTE_SVG);
    let replay_path = icons_dir.join("replay-symbolic.svg");
    let _ = std::fs::write(&replay_path, REPLAY_SVG);
    let star_outline_path = icons_dir.join("star-outline-symbolic.svg");
    let _ = std::fs::write(&star_outline_path, STAR_OUTLINE_SVG);
    let sidebar_path = icons_dir.join("sidebar-show-symbolic.svg");
    let _ = std::fs::write(&sidebar_path, SIDEBAR_SHOW_SVG);
    let logo_path = icons_dir.join("malus-logo.svg");
    let _ = std::fs::write(&logo_path, MALUS_LOGO_SVG);
    let logo_mono_path = icons_dir.join("malus-logo-mono.svg");
    let _ = std::fs::write(&logo_mono_path, MALUS_LOGO_MONO_SVG);
    let logo_sym_path = icons_dir.join("malus-logo-symbolic.svg");
    let _ = std::fs::write(&logo_sym_path, MALUS_LOGO_SYMBOLIC_SVG);

    if let Some(display) = relm4::gtk::gdk::Display::default() {
        let theme = relm4::gtk::IconTheme::for_display(&display);
        theme.add_search_path(&icons_dir);
    }

    // Set the colour logo as the default window icon (taskbar, alt-tab, etc.)
    relm4::gtk::Window::set_default_icon_name("malus-logo");
}

/// Safely returns freed heap and arena memory back to the operating system.
#[inline]
pub fn trim_memory() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    unsafe {
        unsafe extern "C" {
            fn malloc_trim(pad: usize) -> i32;
        }
        malloc_trim(0);
    }
}

fn init_allocator() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    unsafe {
        unsafe extern "C" {
            fn mallopt(param: i32, value: i32) -> i32;
        }
        // M_ARENA_MAX = -8: Cap glibc thread arenas to 2 to prevent multi-arena sprawl.
        mallopt(-8, 2);
        // M_TRIM_THRESHOLD = -1: Keep trim threshold at 128KB, disabling dynamic runaway.
        mallopt(-1, 131072);
    }
}

pub fn run() {
    init_allocator();
    let _ = tracing_subscriber::fmt::try_init();

    // Bound Relm4 Tokio worker and blocking thread pool to avoid glibc thread arena sprawl.
    let _ = relm4::RELM_THREADS.set(2);
    let _ = relm4::RELM_BLOCKING_THREADS.set(4);

    relm4::gtk::init().expect("Malus needs a graphical display");

    // Reset legacy GTK3 dark theme preference on GtkSettings to prevent Libadwaita warning;
    // Malus manages theme and color scheme cleanly via AdwStyleManager.
    if let Some(settings) = relm4::gtk::Settings::default() {
        settings.set_gtk_application_prefer_dark_theme(false);
    }

    relm4::adw::init().expect("Could not initialize Libadwaita");

    let app_id = std::env::var("MALUS_APP_ID").unwrap_or_else(|_| APP_ID.to_string());
    let app = RelmApp::new(&app_id);
    relm4::set_global_css(CSS_STYLE);
    init_custom_icons();
    design::theme::Appearance::load().apply();

    let socket_path = default_socket_path();
    let client = MalusClient::new(socket_path);

    app.run::<app::MalusApp>(client);
}
