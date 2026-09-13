//! Malus Native GTK4 / Libadwaita GUI powered by Relm4.

pub mod app;
pub mod artwork;
pub mod components;
pub mod factories;
pub mod model;

use malus_client::{MalusClient, default_socket_path};
use relm4::RelmApp;

pub const APP_ID: &str = "io.github.sea_deep.Malus";
pub const CSS_STYLE: &str = include_str!("../resources/style.css");

pub fn run() {
    tracing_subscriber::fmt::init();

    let app = RelmApp::new(APP_ID);
    relm4::set_global_css(CSS_STYLE);

    let socket_path = default_socket_path();
    let client = MalusClient::new(socket_path);

    app.run::<app::MalusApp>(client);
}
