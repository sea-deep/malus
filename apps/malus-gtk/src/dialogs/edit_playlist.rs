//! Modal dialogs for editing playlist metadata and confirming deletion.

use malus_client::MalusClient;
use malus_model::MediaRef;
use relm4::adw::{self, prelude::*};
use relm4::gtk;
use std::rc::Rc;

/// Shows an AdwWindow modal dialog to edit an existing playlist's title and description.
pub fn show_edit_playlist_dialog<F>(
    parent: &impl IsA<gtk::Window>,
    client: &MalusClient,
    playlist_ref: &MediaRef,
    current_name: &str,
    current_description: Option<&str>,
    on_updated: F,
) where
    F: Fn(String, Option<String>) + 'static + Clone,
{
    let window = adw::Window::builder()
        .transient_for(parent)
        .modal(true)
        .default_width(420)
        .default_height(280)
        .title("Edit Playlist")
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

    let btn_save = gtk::Button::builder()
        .label("Save")
        .css_classes(["suggested-action"])
        .focus_on_click(false)
        .build();
    header.pack_end(&btn_save);

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

    let title_entry = adw::EntryRow::builder()
        .title("Title")
        .text(current_name)
        .build();
    let desc_entry = adw::EntryRow::builder()
        .title("Description")
        .text(current_description.unwrap_or(""))
        .build();

    let btn_save_clone = btn_save.clone();
    title_entry.connect_changed(move |entry| {
        let text = entry.text();
        btn_save_clone.set_sensitive(!text.trim().is_empty());
    });

    pref_group.add(&title_entry);
    pref_group.add(&desc_entry);
    content_box.append(&pref_group);

    let error_label = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .visible(false)
        .css_classes(["error", "dim-label"])
        .build();
    content_box.append(&error_label);

    let client = client.clone();
    let pl_ref = playlist_ref.clone();
    let window_close = window.clone();
    let title_row = title_entry.clone();
    let desc_row = desc_entry.clone();
    let err_lbl = error_label.clone();
    let btn_sv = btn_save.clone();

    let do_save = Rc::new(move || {
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

        btn_sv.set_sensitive(false);
        title_row.set_sensitive(false);
        desc_row.set_sensitive(false);
        err_lbl.set_visible(false);

        let c = client.clone();
        let p_id = pl_ref.clone();
        let w = window_close.clone();
        let on_u = on_updated.clone();
        let err_l = err_lbl.clone();
        let btn_ref = btn_sv.clone();
        let t_ref = title_row.clone();
        let d_ref = desc_row.clone();
        let title_clone = title.clone();
        let desc_clone = desc.clone();

        relm4::gtk::glib::spawn_future_local(async move {
            match c
                .update_playlist(&p_id, title_clone.clone(), desc_clone.clone())
                .await
            {
                Ok(()) => {
                    w.close();
                    on_u(title_clone, desc_clone);
                }
                Err(e) => {
                    err_l.set_label(&format!("Failed to update playlist: {e}"));
                    err_l.set_visible(true);
                    btn_ref.set_sensitive(true);
                    t_ref.set_sensitive(true);
                    d_ref.set_sensitive(true);
                }
            }
        });
    });

    let do_save_click = do_save.clone();
    btn_save.connect_clicked(move |_| {
        do_save_click();
    });

    let do_save_enter = do_save.clone();
    title_entry.connect_entry_activated(move |_| {
        do_save_enter();
    });

    let do_save_desc = do_save.clone();
    desc_entry.connect_entry_activated(move |_| {
        do_save_desc();
    });

    toolbar_view.set_content(Some(&content_box));
    window.set_content(Some(&toolbar_view));
    window.present();
}

/// Shows an AdwAlertDialog confirming deletion of an editable playlist.
pub fn show_delete_playlist_dialog<F>(
    parent: &impl IsA<gtk::Widget>,
    client: &MalusClient,
    playlist_ref: &MediaRef,
    playlist_title: &str,
    on_deleted: F,
) where
    F: Fn() + 'static + Clone,
{
    let dialog = adw::AlertDialog::builder()
        .heading("Delete Playlist?")
        .body(format!(
            "Are you sure you want to delete “{playlist_title}”? This cannot be undone."
        ))
        .build();

    dialog.add_response("cancel", "Cancel");
    dialog.add_response("delete", "Delete");
    dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");

    let c = client.clone();
    let pl_ref = playlist_ref.clone();

    dialog.connect_response(None, move |_dlg, resp| {
        if resp == "delete" {
            let c2 = c.clone();
            let p2 = pl_ref.clone();
            let on_del = on_deleted.clone();
            relm4::gtk::glib::spawn_future_local(async move {
                if let Ok(()) = c2.delete_playlist(&p2).await {
                    on_del();
                }
            });
        }
    });

    dialog.present(Some(parent));
}
