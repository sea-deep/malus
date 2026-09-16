//! Song credits presentation modal dialog.

use malus_client::MalusClient;
use malus_model::{Credits, MediaRef};
use relm4::adw::{self, prelude::*};
use relm4::gtk;

/// Shows an AdwWindow modal dialog displaying credits for a track.
pub fn show_credits_dialog(
    parent: &impl IsA<gtk::Window>,
    client: &MalusClient,
    track_ref: &MediaRef,
    track_title: &str,
) {
    let window = adw::Window::builder()
        .transient_for(parent)
        .modal(true)
        .default_width(460)
        .default_height(540)
        .title("Song Credits")
        .build();

    let toolbar_view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    toolbar_view.add_top_bar(&header);

    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .build();

    let content_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(20)
        .margin_start(24)
        .margin_end(24)
        .margin_top(20)
        .margin_bottom(32)
        .build();

    // Track Title header
    let track_label = gtk::Label::builder()
        .label(track_title)
        .wrap(true)
        .xalign(0.0)
        .css_classes(["credits-track-title"])
        .build();
    content_box.append(&track_label);

    let spinner = gtk::Spinner::new();
    spinner.set_spinning(true);
    spinner.set_size_request(24, 24);
    spinner.set_halign(gtk::Align::Center);
    spinner.set_margin_top(32);
    content_box.append(&spinner);

    scrolled.set_child(Some(&content_box));
    toolbar_view.set_content(Some(&scrolled));
    window.set_content(Some(&toolbar_view));
    window.present();

    let client_clone = client.clone();
    let track_ref_clone = track_ref.clone();
    let content_box_weak = content_box.downgrade();
    let spinner_weak = spinner.downgrade();
    let track_label = track_label.downgrade();

    gtk::glib::MainContext::default().spawn_local(async move {
        let (res, metadata) = tokio::join!(
            client_clone.get_credits(&track_ref_clone),
            client_clone.get_catalog_item(&track_ref_clone)
        );
        if let (Some(label), Ok(malus_ipc::wire::CatalogItemWire::Track(track))) =
            (track_label.upgrade(), metadata)
        {
            label.set_text(&track.title);
        }

        if let (Some(cb), Some(sp)) = (content_box_weak.upgrade(), spinner_weak.upgrade()) {
            cb.remove(&sp);

            match res {
                Ok(credits) if !credits.is_empty() => {
                    populate_credits(&cb, &credits);
                }
                result => {
                    let message = match result {
                        Err(error) => format!("Credits couldn’t be loaded: {error}"),
                        _ => "No credits available for this song".into(),
                    };
                    let empty_label = gtk::Label::builder()
                        .label(&message)
                        .wrap(true)
                        .css_classes(["utility-empty-label"])
                        .margin_top(32)
                        .build();
                    cb.append(&empty_label);
                }
            }
        }
    });
}

fn populate_credits(container: &gtk::Box, credits: &Credits) {
    for category in &credits.categories {
        let cat_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(10)
            .margin_top(8)
            .build();

        let cat_title = gtk::Label::builder()
            .label(&category.title)
            .xalign(0.0)
            .css_classes(["credits-category-title"])
            .build();
        cat_box.append(&cat_title);

        let items_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .build();

        for item in &category.items {
            let item_box = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(2)
                .build();

            let name_label = gtk::Label::builder()
                .label(&item.name)
                .xalign(0.0)
                .css_classes(["credits-item-name"])
                .build();
            item_box.append(&name_label);

            if !item.roles.is_empty() {
                let roles_str = item.roles.join(", ");
                let roles_label = gtk::Label::builder()
                    .label(&roles_str)
                    .xalign(0.0)
                    .css_classes(["credits-item-roles"])
                    .build();
                item_box.append(&roles_label);
            }

            items_box.append(&item_box);
        }

        cat_box.append(&items_box);
        container.append(&cat_box);
    }
}
