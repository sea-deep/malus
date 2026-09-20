//! Context action menu for tracks, albums, and playlists.

use malus_ipc::wire::PageActionWire;
use malus_model::{MediaRef, PageRoute};
use relm4::gtk::{self, prelude::*};

#[derive(Debug, Clone)]
pub enum ActionMenuCommand {
    Action(PageActionWire),
    ViewCredits(MediaRef),
    AddToPlaylist(MediaRef),
    RemoveFromPlaylist {
        playlist: MediaRef,
        track_index: usize,
        expected_track: MediaRef,
    },
    Navigate(PageRoute),
    CopyLink(String),
}

fn create_menu_btn(label_text: &str) -> gtk::Button {
    let btn = gtk::Button::new();
    btn.add_css_class("flat");
    btn.set_focus_on_click(false);
    let label = gtk::Label::builder()
        .label(label_text)
        .xalign(0.0)
        .hexpand(true)
        .build();
    btn.set_child(Some(&label));
    btn
}

/// Creates a GtkPopoverMenu with standard Apple Music contextual actions.
pub fn build_action_popover<F>(
    reference: &MediaRef,
    is_favorite: bool,
    in_library: bool,
    is_track: bool,
    on_action: F,
) -> gtk::Popover
where
    F: Fn(ActionMenuCommand) + 'static + Clone,
{
    build_action_popover_full(
        reference,
        is_favorite,
        in_library,
        is_track,
        &[],
        None,
        None,
        None,
        on_action,
    )
}

/// Creates a GtkPopoverMenu respecting dynamic actions provided by the backend.
pub fn build_action_popover_with_actions<F>(
    reference: &MediaRef,
    is_favorite: bool,
    in_library: bool,
    is_track: bool,
    actions: &[PageActionWire],
    on_action: F,
) -> gtk::Popover
where
    F: Fn(ActionMenuCommand) + 'static + Clone,
{
    build_action_popover_full(
        reference,
        is_favorite,
        in_library,
        is_track,
        actions,
        None,
        None,
        None,
        on_action,
    )
}

/// Creates a GtkPopoverMenu supporting explicit actions, playlist context, and album/artist navigation.
#[allow(clippy::too_many_arguments)]
pub fn build_action_popover_full<F>(
    reference: &MediaRef,
    is_favorite: bool,
    in_library: bool,
    is_track: bool,
    actions: &[PageActionWire],
    playlist_context: Option<(MediaRef, usize)>,
    album_route: Option<PageRoute>,
    artist_route: Option<PageRoute>,
    on_action: F,
) -> gtk::Popover
where
    F: Fn(ActionMenuCommand) + 'static + Clone,
{
    let popover = gtk::Popover::new();
    let box_container = gtk::Box::new(gtk::Orientation::Vertical, 4);
    box_container.set_margin_top(6);
    box_container.set_margin_bottom(6);
    box_container.set_margin_start(6);
    box_container.set_margin_end(6);

    let has_explicit_actions = !actions.is_empty();

    // Play Next
    let show_play_next = if has_explicit_actions {
        actions
            .iter()
            .any(|a| matches!(a, PageActionWire::PlayNext(_)))
    } else {
        true
    };
    if show_play_next {
        let btn_play_next = create_menu_btn("Play Next");
        let r = reference.clone();
        let cb = on_action.clone();
        let p = popover.clone();
        btn_play_next.connect_clicked(move |_| {
            cb(ActionMenuCommand::Action(PageActionWire::PlayNext(
                r.clone(),
            )));
            p.popdown();
        });
        box_container.append(&btn_play_next);
    }

    // Play Later
    let show_play_later = if has_explicit_actions {
        actions
            .iter()
            .any(|a| matches!(a, PageActionWire::PlayLater(_)))
    } else {
        true
    };
    if show_play_later {
        let btn_play_later = create_menu_btn("Play Later");
        let r = reference.clone();
        let cb = on_action.clone();
        let p = popover.clone();
        btn_play_later.connect_clicked(move |_| {
            cb(ActionMenuCommand::Action(PageActionWire::PlayLater(
                r.clone(),
            )));
            p.popdown();
        });
        box_container.append(&btn_play_later);
    }

    let separator1 = gtk::Separator::new(gtk::Orientation::Horizontal);
    separator1.set_margin_top(4);
    separator1.set_margin_bottom(4);
    box_container.append(&separator1);

    // Add to Playlist (songs only)
    if is_track {
        let btn_add_to_playlist = create_menu_btn("Add to Playlist…");
        let r = reference.clone();
        let cb = on_action.clone();
        let p = popover.clone();
        btn_add_to_playlist.connect_clicked(move |_| {
            cb(ActionMenuCommand::AddToPlaylist(r.clone()));
            p.popdown();
        });
        box_container.append(&btn_add_to_playlist);
    }

    // Remove from Playlist (if rendered within an editable playlist)
    if let Some((ref pl_ref, idx)) = playlist_context {
        let btn_remove_pl = create_menu_btn("Remove from Playlist");
        let r = reference.clone();
        let pl_r = pl_ref.clone();
        let cb = on_action.clone();
        let p = popover.clone();
        btn_remove_pl.connect_clicked(move |_| {
            cb(ActionMenuCommand::RemoveFromPlaylist {
                playlist: pl_r.clone(),
                track_index: idx,
                expected_track: r.clone(),
            });
            p.popdown();
        });
        box_container.append(&btn_remove_pl);
    }

    // Favorite / Unfavorite
    let effective_favorite = is_favorite
        || actions
            .iter()
            .any(|a| matches!(a, PageActionWire::Unfavorite(_)));

    let fav_label = if effective_favorite {
        "Unfavorite"
    } else {
        "Favorite"
    };
    let btn_favorite = create_menu_btn(fav_label);
    {
        let r = reference.clone();
        let cb = on_action.clone();
        let p = popover.clone();
        btn_favorite.connect_clicked(move |_| {
            let action = if effective_favorite {
                PageActionWire::Unfavorite(r.clone())
            } else {
                PageActionWire::Favorite(r.clone())
            };
            cb(ActionMenuCommand::Action(action));
            p.popdown();
        });
    }
    box_container.append(&btn_favorite);

    // Suggest Less
    let show_suggest_less = if has_explicit_actions {
        actions
            .iter()
            .any(|a| matches!(a, PageActionWire::SuggestLess(_)))
    } else {
        true
    };
    if show_suggest_less {
        let btn_suggest_less = create_menu_btn("Suggest Less");
        let r = reference.clone();
        let cb = on_action.clone();
        let p = popover.clone();
        btn_suggest_less.connect_clicked(move |_| {
            cb(ActionMenuCommand::Action(PageActionWire::SuggestLess(
                r.clone(),
            )));
            p.popdown();
        });
        box_container.append(&btn_suggest_less);
    }

    // Add to Library / Remove from Library
    let effective_in_library = if has_explicit_actions {
        !actions
            .iter()
            .any(|a| matches!(a, PageActionWire::AddToLibrary(_)))
            || in_library
    } else {
        in_library
    };

    if !effective_in_library {
        let btn_add_lib = create_menu_btn("Add to Library");
        let r = reference.clone();
        let cb = on_action.clone();
        let p = popover.clone();
        btn_add_lib.connect_clicked(move |_| {
            cb(ActionMenuCommand::Action(PageActionWire::AddToLibrary(
                r.clone(),
            )));
            p.popdown();
        });
        box_container.append(&btn_add_lib);
    } else {
        let btn_rem_lib = create_menu_btn("Remove from Library");
        let r = reference.clone();
        let cb = on_action.clone();
        let p = popover.clone();
        btn_rem_lib.connect_clicked(move |_| {
            cb(ActionMenuCommand::Action(
                PageActionWire::RemoveFromLibrary(r.clone()),
            ));
            p.popdown();
        });
        box_container.append(&btn_rem_lib);
    }

    // Credits (for tracks)
    if is_track {
        let separator2 = gtk::Separator::new(gtk::Orientation::Horizontal);
        separator2.set_margin_top(4);
        separator2.set_margin_bottom(4);
        box_container.append(&separator2);

        let btn_credits = create_menu_btn("View Credits");
        let r = reference.clone();
        let cb = on_action.clone();
        let p = popover.clone();
        btn_credits.connect_clicked(move |_| {
            cb(ActionMenuCommand::ViewCredits(r.clone()));
            p.popdown();
        });
        box_container.append(&btn_credits);
    }

    // Navigation (View Album / View Artist)
    if album_route.is_some() || artist_route.is_some() {
        let separator3 = gtk::Separator::new(gtk::Orientation::Horizontal);
        separator3.set_margin_top(4);
        separator3.set_margin_bottom(4);
        box_container.append(&separator3);

        if let Some(route) = album_route {
            let btn_album = create_menu_btn("Go to Album");
            let cb = on_action.clone();
            let p = popover.clone();
            btn_album.connect_clicked(move |_| {
                cb(ActionMenuCommand::Navigate(route.clone()));
                p.popdown();
            });
            box_container.append(&btn_album);
        }

        if let Some(route) = artist_route {
            let btn_artist = create_menu_btn("Go to Artist");
            let cb = on_action.clone();
            let p = popover.clone();
            btn_artist.connect_clicked(move |_| {
                cb(ActionMenuCommand::Navigate(route.clone()));
                p.popdown();
            });
            box_container.append(&btn_artist);
        }
    }

    // Copy Link
    if let Some(url) = reference.web_url() {
        let separator4 = gtk::Separator::new(gtk::Orientation::Horizontal);
        separator4.set_margin_top(4);
        separator4.set_margin_bottom(4);
        box_container.append(&separator4);

        let btn_copy = create_menu_btn("Copy Link");
        let cb = on_action.clone();
        let p = popover.clone();
        btn_copy.connect_clicked(move |_| {
            cb(ActionMenuCommand::CopyLink(url.clone()));
            p.popdown();
        });
        box_container.append(&btn_copy);
    }

    popover.set_child(Some(&box_container));
    popover
}
