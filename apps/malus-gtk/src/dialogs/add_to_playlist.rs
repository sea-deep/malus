//! Native Libadwaita dialog for adding a track to an existing playlist or creating a new one seeded with it.

use malus_client::MalusClient;
use malus_ipc::wire::LibraryKindWire;
use malus_model::MediaRef;
use relm4::adw::{self, prelude::*};
use relm4::gtk;

/// Shows a modal dialog allowing the user to pick a playlist to add a track to,
/// or create a new playlist seeded with that track.
pub fn show_add_to_playlist_dialog<F>(
    parent_window: &impl IsA<gtk::Window>,
    client: &MalusClient,
    track_ref: &MediaRef,
    on_toast: F,
) where
    F: Fn(String) + 'static + Clone,
{
    let window = adw::Window::builder()
        .transient_for(parent_window)
        .modal(true)
        .default_width(360)
        .default_height(440)
        .title("Add to Playlist")
        .build();

    let toolbar_view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();

    let btn_cancel = gtk::Button::builder().label("Cancel").build();
    let w_cancel = window.clone();
    btn_cancel.connect_clicked(move |_| {
        w_cancel.close();
    });
    header.pack_start(&btn_cancel);

    toolbar_view.add_top_bar(&header);

    let content_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_start(16)
        .margin_end(16)
        .margin_top(12)
        .margin_bottom(16)
        .build();

    // "+ New Playlist…" action button
    let btn_new_pl = gtk::Button::builder().css_classes(["flat"]).build();
    let btn_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let new_icon = gtk::Image::from_icon_name("list-add-symbolic");
    let new_lbl = gtk::Label::builder()
        .label("New Playlist…")
        .xalign(0.0)
        .hexpand(true)
        .build();
    btn_box.append(&new_icon);
    btn_box.append(&new_lbl);
    btn_new_pl.set_child(Some(&btn_box));

    {
        let p_win = parent_window.as_ref().clone();
        let c = client.clone();
        let t = track_ref.clone();
        let w = window.clone();
        let toast = on_toast.clone();
        btn_new_pl.connect_clicked(move |_| {
            w.close();
            let toast_clone = toast.clone();
            super::show_new_playlist_dialog(&p_win, &c, vec![t.clone()], move |pl| {
                toast_clone(format!("Created and added to “{}”", pl.title));
            });
        });
    }
    content_box.append(&btn_new_pl);

    let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
    content_box.append(&sep);

    // Scrolled window for existing playlists
    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .build();

    let list_box = gtk::ListBox::builder()
        .css_classes(["boxed-list"])
        .selection_mode(gtk::SelectionMode::None)
        .build();

    let skeleton_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .margin_top(8)
        .margin_bottom(8)
        .build();
    for _ in 0..5 {
        let row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .margin_start(12)
            .margin_end(12)
            .margin_top(6)
            .margin_bottom(6)
            .build();
        row.append(&crate::widgets::skeleton::skeleton_box(Some(40), 40, 6));
        let meta = gtk::Box::new(gtk::Orientation::Vertical, 4);
        meta.set_valign(gtk::Align::Center);
        meta.append(&crate::widgets::skeleton::skeleton_text(Some(140), 14));
        meta.append(&crate::widgets::skeleton::skeleton_text(Some(80), 11));
        row.append(&meta);
        skeleton_box.append(&row);
    }

    scrolled.set_child(Some(&skeleton_box));
    content_box.append(&scrolled);

    // Asynchronously fetch playlists
    {
        let c = client.clone();
        let t = track_ref.clone();
        let w = window.clone();
        let toast = on_toast;
        let sc = scrolled.clone();
        let lb = list_box.clone();

        relm4::gtk::glib::spawn_future_local(async move {
            match c
                .get_library(LibraryKindWire::Playlists, Some(100), None)
                .await
            {
                Ok(malus_ipc::wire::LibraryPageWire::Playlists(page)) => {
                    if page.items.is_empty() {
                        let empty_lbl = gtk::Label::builder()
                            .label("No playlists in your library yet")
                            .css_classes(["dim-label"])
                            .margin_top(24)
                            .margin_bottom(24)
                            .halign(gtk::Align::Center)
                            .valign(gtk::Align::Center)
                            .vexpand(true)
                            .build();
                        sc.set_child(Some(&empty_lbl));
                    } else {
                        for playlist in page.items {
                            let row = adw::ActionRow::builder()
                                .title(&playlist.title)
                                .activatable(true)
                                .build();
                            let icon = gtk::Image::from_icon_name("audio-x-generic-symbolic");
                            row.add_prefix(&icon);

                            let c_add = c.clone();
                            let pl_ref = playlist.id.clone();
                            let pl_name = playlist.title.clone();
                            let t_add = t.clone();
                            let toast_add = toast.clone();
                            let w_close = w.clone();

                            row.connect_activated(move |_| {
                                w_close.close();
                                let c3 = c_add.clone();
                                let pl3 = pl_ref.clone();
                                let pl_n = pl_name.clone();
                                let t3 = t_add.clone();
                                let toast3 = toast_add.clone();

                                relm4::gtk::glib::spawn_future_local(async move {
                                    match c3.add_tracks_to_playlist(&pl3, vec![t3]).await {
                                        Ok(()) => {
                                            toast3(format!("Added to “{pl_n}”"));
                                        }
                                        Err(e) => {
                                            toast3(format!("Failed to add to “{pl_n}”: {e}"));
                                        }
                                    }
                                });
                            });

                            lb.append(&row);
                        }
                        sc.set_child(Some(&lb));
                    }
                }
                _ => {
                    let err_lbl = gtk::Label::builder()
                        .label("Failed to load playlists")
                        .css_classes(["error", "dim-label"])
                        .margin_top(24)
                        .margin_bottom(24)
                        .halign(gtk::Align::Center)
                        .valign(gtk::Align::Center)
                        .vexpand(true)
                        .build();
                    sc.set_child(Some(&err_lbl));
                }
            }
        });
    }

    toolbar_view.set_content(Some(&content_box));
    window.set_content(Some(&toolbar_view));
    window.present();
}
