//! Subtle loading spinner indicator widget.

use relm4::adw;
use relm4::gtk::{self, prelude::*};

pub fn create_loading_spinner(message: Option<&str>) -> gtk::Box {
    let container = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .hexpand(true)
        .vexpand(true)
        .margin_top(48)
        .margin_bottom(48)
        .build();

    let spinner = adw::Spinner::new();
    spinner.set_size_request(32, 32);
    spinner.set_halign(gtk::Align::Center);

    container.append(&spinner);

    if let Some(msg) = message {
        let label = gtk::Label::builder()
            .label(msg)
            .css_classes(vec!["loading-message".to_string()])
            .halign(gtk::Align::Center)
            .build();
        container.append(&label);
    }

    container
}
