//! Modal dialog for creating a new playlist in Apple Music.

use malus_client::MalusClient;
use malus_model::{MediaRef, Playlist};
use relm4::adw::{self, prelude::*};
use relm4::gtk;
use std::rc::Rc;

/// Shows an AdwWindow modal dialog to create a new playlist.
pub fn show_new_playlist_dialog<F>(
    parent: &impl IsA<gtk::Window>,
    client: &MalusClient,
    initial_tracks: Vec<MediaRef>,
    on_created: F,
) where
    F: Fn(Playlist) + 'static + Clone,
{
    let window = adw::Window::builder()
        .transient_for(parent)
        .modal(true)
        .default_width(420)
        .default_height(280)
        .title("New Playlist")
        .build();

    let toolbar_view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();

    let btn_cancel = gtk::Button::builder()
        .label("Cancel")
        .focus_on_click(false)
        .build();
    let w_cancel = window.clone();
    btn_cancel.connect_clicked(move |_| {
        w_cancel.close();
    });
    header.pack_start(&btn_cancel);

    let btn_create = gtk::Button::builder()
        .label("Create")
        .css_classes(["suggested-action"])
        .focus_on_click(false)
        .sensitive(false)
        .build();
    header.pack_end(&btn_create);

    toolbar_view.add_top_bar(&header);

    let content_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(16)
        .margin_start(24)
        .margin_end(24)
        .margin_top(20)
        .margin_bottom(24)
        .build();

    let pref_group = adw::PreferencesGroup::new();

    let title_entry = adw::EntryRow::builder().title("Title").build();
    let desc_entry = adw::EntryRow::builder()
        .title("Description (optional)")
        .build();

    let btn_create_clone = btn_create.clone();
    title_entry.connect_changed(move |entry| {
        let text = entry.text();
        btn_create_clone.set_sensitive(!text.trim().is_empty());
    });

    pref_group.add(&title_entry);
    pref_group.add(&desc_entry);
    content_box.append(&pref_group);

    // Error label for feedback
    let error_label = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .visible(false)
        .css_classes(["error", "dim-label"])
        .build();
    content_box.append(&error_label);

    let client = client.clone();
    let window_close = window.clone();
    let title_row = title_entry.clone();
    let desc_row = desc_entry.clone();
    let err_lbl = error_label.clone();
    let btn_cr = btn_create.clone();

    let do_create = Rc::new(move || {
        let title = title_row.text().trim().to_string();
        if title.is_empty() {
            return;
        }
        let desc_text = desc_row.text().trim().to_string();
        let desc = if desc_text.is_empty() {
            None
        } else {
            Some(desc_text)
        };

        btn_cr.set_sensitive(false);
        title_row.set_sensitive(false);
        desc_row.set_sensitive(false);
        err_lbl.set_visible(false);

        let c = client.clone();
        let w = window_close.clone();
        let on_c = on_created.clone();
        let err_l = err_lbl.clone();
        let btn_ref = btn_cr.clone();
        let t_ref = title_row.clone();
        let d_ref = desc_row.clone();
        let init_tracks = initial_tracks.clone();

        relm4::gtk::glib::spawn_future_local(async move {
            match c.create_playlist(title, desc, init_tracks).await {
                Ok(playlist) => {
                    w.close();
                    on_c(playlist);
                }
                Err(e) => {
                    err_l.set_label(&format!("Failed to create playlist: {e}"));
                    err_l.set_visible(true);
                    btn_ref.set_sensitive(true);
                    t_ref.set_sensitive(true);
                    d_ref.set_sensitive(true);
                }
            }
        });
    });

    let do_create_click = do_create.clone();
    btn_create.connect_clicked(move |_| {
        do_create_click();
    });

    let do_create_enter = do_create.clone();
    title_entry.connect_entry_activated(move |_| {
        do_create_enter();
    });

    let do_create_desc = do_create.clone();
    desc_entry.connect_entry_activated(move |_| {
        do_create_desc();
    });

    toolbar_view.set_content(Some(&content_box));
    window.set_content(Some(&toolbar_view));
    window.present();
}
