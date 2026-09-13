//! Malus Greenfield Consumer Music Player GUI powered by Relm4 & GTK4.

pub mod app;
pub mod components;
pub mod design;
pub mod model;
pub mod pages;
pub mod services;

use malus_client::{MalusClient, default_socket_path};
use relm4::RelmApp;

pub const APP_ID: &str = "io.github.sea_deep.MalusNext";
pub const CSS_STYLE: &str = include_str!("../resources/style.css");

pub fn run() {
    let _ = tracing_subscriber::fmt::try_init();

    if let Ok(theme) = std::env::var("MALUS_THEME") {
        if theme.eq_ignore_ascii_case("light") {
            unsafe { std::env::set_var("ADW_DEBUG_COLOR_SCHEME", "prefer-light") };
        } else if theme.eq_ignore_ascii_case("dark") {
            unsafe { std::env::set_var("ADW_DEBUG_COLOR_SCHEME", "prefer-dark") };
        }
    }

    let app = RelmApp::new(APP_ID);
    relm4::set_global_css(CSS_STYLE);

    if let Ok(theme) = std::env::var("MALUS_THEME") {
        let manager = relm4::adw::StyleManager::default();
        if theme.eq_ignore_ascii_case("light") {
            manager.set_color_scheme(relm4::adw::ColorScheme::ForceLight);
        } else if theme.eq_ignore_ascii_case("dark") {
            manager.set_color_scheme(relm4::adw::ColorScheme::ForceDark);
        }
    }

    let socket_path = default_socket_path();
    let client = MalusClient::new(socket_path);

    app.run::<app::MalusApp>(client);
}
