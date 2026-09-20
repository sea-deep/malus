//! Responsive skeleton loading widgets and page layouts.
//!
//! Provides shimmer/pulsing placeholders that match the exact shape of real content
//! to prevent jarring blank states and spinners during navigation.

use malus_model::PageRoute;
use relm4::gtk::{self, prelude::*};

/// A basic pulsing rectangular placeholder.
pub fn skeleton_box(width: Option<i32>, height: i32, _radius: i32) -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    b.add_css_class("skeleton-block");
    if let Some(w) = width {
        b.set_size_request(w, height);
    } else {
        b.set_height_request(height);
        b.set_hexpand(true);
    }
    b
}

/// A circular placeholder (e.g. artist avatar or icon).
pub fn skeleton_circle(diameter: i32) -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    b.add_css_class("skeleton-block");
    b.add_css_class("skeleton-circle");
    b.set_size_request(diameter, diameter);
    b
}

/// A single text line placeholder.
pub fn skeleton_text(width: Option<i32>, height: i32) -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    b.add_css_class("skeleton-block");
    b.add_css_class("skeleton-text");
    if let Some(w) = width {
        b.set_size_request(w, height);
    } else {
        b.set_height_request(height);
        b.set_hexpand(true);
    }
    b
}

/// A card placeholder matching album/playlist cards:
/// Square artwork + title line + subtitle line.
pub fn skeleton_card(card_width: i32) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    card.set_width_request(card_width);

    // Artwork square
    let art = skeleton_box(Some(card_width), card_width, 8);
    art.add_css_class("skeleton-artwork");
    card.append(&art);

    // Title line (approx 75% width)
    let title = skeleton_text(Some((card_width as f64 * 0.75) as i32), 14);
    card.append(&title);

    // Subtitle line (approx 50% width)
    let sub = skeleton_text(Some((card_width as f64 * 0.50) as i32), 12);
    card.append(&sub);

    card
}

/// A track row placeholder matching track lists:
/// Track number + title/artist lines + duration.
pub fn skeleton_track_row() -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("skeleton-track-row");
    row.set_height_request(44);
    row.set_hexpand(true);
    row.set_valign(gtk::Align::Center);

    // Track index / small icon
    let num = skeleton_box(Some(24), 16, 4);
    num.set_valign(gtk::Align::Center);
    row.append(&num);

    // Optional tiny artwork
    let art = skeleton_box(Some(36), 36, 6);
    art.set_valign(gtk::Align::Center);
    row.append(&art);

    // Title & artist stack
    let meta = gtk::Box::new(gtk::Orientation::Vertical, 4);
    meta.set_hexpand(true);
    meta.set_valign(gtk::Align::Center);
    let title = skeleton_text(Some(180), 13);
    let artist = skeleton_text(Some(110), 11);
    meta.append(&title);
    meta.append(&artist);
    row.append(&meta);

    // Duration
    let dur = skeleton_box(Some(36), 14, 4);
    dur.set_halign(gtk::Align::End);
    dur.set_valign(gtk::Align::Center);
    row.append(&dur);

    row
}

/// Shelves skeleton for Home, New, Radio:
/// Hero banner placeholder + horizontal shelves with card placeholders.
pub fn build_shelves_skeleton() -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 28);
    container.set_hexpand(true);

    // Hero banner
    let hero = skeleton_box(None, 200, 12);
    hero.set_margin_bottom(8);
    container.append(&hero);

    // Shelf 1
    let shelf1 = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let title1 = skeleton_text(Some(160), 20);
    shelf1.append(&title1);
    let row1 = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    for _ in 0..6 {
        row1.append(&skeleton_card(160));
    }
    shelf1.append(&row1);
    container.append(&shelf1);

    // Shelf 2
    let shelf2 = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let title2 = skeleton_text(Some(140), 20);
    shelf2.append(&title2);
    let row2 = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    for _ in 0..6 {
        row2.append(&skeleton_card(160));
    }
    shelf2.append(&row2);
    container.append(&shelf2);

    container
}

/// Grid skeleton for LibraryAlbums, LibraryRecentlyAdded, LibraryPlaylists, LibraryMadeForYou.
pub fn build_grid_skeleton(title_width: i32) -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 20);
    container.set_hexpand(true);

    // Title header
    let header = gtk::Box::new(gtk::Orientation::Vertical, 6);
    header.append(&skeleton_text(Some(title_width), 28));
    header.append(&skeleton_text(Some(80), 14));
    header.set_margin_bottom(12);
    container.append(&header);

    // FlowBox with card placeholders
    let flow = gtk::FlowBox::new();
    flow.set_selection_mode(gtk::SelectionMode::None);
    flow.set_homogeneous(true);
    flow.set_column_spacing(16);
    flow.set_row_spacing(20);
    flow.set_valign(gtk::Align::Start);
    flow.set_hexpand(true);

    for _ in 0..12 {
        let child = skeleton_card(160);
        flow.append(&child);
    }
    container.append(&flow);

    container
}

/// Track list skeleton for LibrarySongs.
pub fn build_tracks_skeleton() -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 16);
    container.set_hexpand(true);

    // Title
    let header = gtk::Box::new(gtk::Orientation::Vertical, 6);
    header.append(&skeleton_text(Some(180), 28));
    header.append(&skeleton_text(Some(90), 14));
    header.set_margin_bottom(8);
    container.append(&header);

    // Track rows
    let list = gtk::Box::new(gtk::Orientation::Vertical, 4);
    for _ in 0..14 {
        list.append(&skeleton_track_row());
    }
    container.append(&list);

    container
}

/// Split view skeleton for LibraryArtists and LibraryGenres.
pub fn build_split_skeleton() -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    container.set_hexpand(true);
    container.set_vexpand(true);

    // Left master list placeholder
    let left = gtk::Box::new(gtk::Orientation::Vertical, 12);
    left.set_width_request(240);
    left.set_margin_end(8);

    // Search bar placeholder
    left.append(&skeleton_box(None, 34, 8));

    // List rows
    for i in 0..12 {
        let w = if i % 3 == 0 {
            140
        } else if i % 2 == 0 {
            170
        } else {
            120
        };
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.set_height_request(32);
        row.set_valign(gtk::Align::Center);
        row.append(&skeleton_text(Some(w), 15));
        left.append(&row);
    }
    container.append(&left);

    // Right detail placeholder
    let right = gtk::Box::new(gtk::Orientation::Vertical, 20);
    right.set_hexpand(true);

    let detail_header = gtk::Box::new(gtk::Orientation::Vertical, 6);
    detail_header.append(&skeleton_text(Some(200), 28));
    detail_header.append(&skeleton_text(Some(100), 14));
    right.append(&detail_header);

    let flow = gtk::FlowBox::new();
    flow.set_selection_mode(gtk::SelectionMode::None);
    flow.set_homogeneous(true);
    flow.set_column_spacing(16);
    flow.set_row_spacing(20);
    flow.set_valign(gtk::Align::Start);
    flow.set_hexpand(true);

    for _ in 0..8 {
        flow.append(&skeleton_card(160));
    }
    right.append(&flow);

    container.append(&right);
    container
}

/// Detail skeleton for Album, Playlist, Replay.
pub fn build_detail_skeleton() -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 24);
    container.set_hexpand(true);

    // Header with artwork and metadata
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 24);
    header.set_valign(gtk::Align::Center);

    // Large artwork
    let art = skeleton_box(Some(190), 190, 10);
    header.append(&art);

    // Metadata
    let meta = gtk::Box::new(gtk::Orientation::Vertical, 10);
    meta.set_valign(gtk::Align::Center);
    meta.set_hexpand(true);

    meta.append(&skeleton_text(Some(60), 12));
    meta.append(&skeleton_text(Some(260), 32));
    meta.append(&skeleton_text(Some(180), 16));
    meta.append(&skeleton_text(Some(120), 13));

    // Action buttons row
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    actions.set_margin_top(8);
    actions.append(&skeleton_box(Some(90), 34, 999));
    actions.append(&skeleton_box(Some(90), 34, 999));
    actions.append(&skeleton_circle(34));
    meta.append(&actions);

    header.append(&meta);
    container.append(&header);

    // Track rows
    let tracks = gtk::Box::new(gtk::Orientation::Vertical, 4);
    tracks.set_margin_top(12);
    for _ in 0..8 {
        tracks.append(&skeleton_track_row());
    }
    container.append(&tracks);

    container
}

/// Artist detail skeleton: Hero header + discography shelf + songs.
pub fn build_artist_skeleton() -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 28);
    container.set_hexpand(true);

    // Hero banner
    let hero = skeleton_box(None, 220, 12);
    container.append(&hero);

    // Discography shelf
    let shelf = gtk::Box::new(gtk::Orientation::Vertical, 12);
    shelf.append(&skeleton_text(Some(140), 20));
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    for _ in 0..5 {
        row.append(&skeleton_card(160));
    }
    shelf.append(&row);
    container.append(&shelf);

    // Top songs
    let songs = gtk::Box::new(gtk::Orientation::Vertical, 8);
    songs.append(&skeleton_text(Some(120), 20));
    for _ in 0..5 {
        songs.append(&skeleton_track_row());
    }
    container.append(&songs);

    container
}

/// Dispatches to the appropriate skeleton layout based on `PageRoute`.
pub fn build_route_skeleton(route: &PageRoute) -> gtk::Widget {
    let b: gtk::Box = match route {
        PageRoute::Home | PageRoute::New | PageRoute::Radio => build_shelves_skeleton(),
        PageRoute::LibraryAlbums
        | PageRoute::LibraryRecentlyAdded
        | PageRoute::LibraryPlaylists
        | PageRoute::LibraryMadeForYou => build_grid_skeleton(180),
        PageRoute::LibrarySongs => build_tracks_skeleton(),
        PageRoute::LibraryArtists | PageRoute::LibraryGenres => build_split_skeleton(),
        PageRoute::Album(_)
        | PageRoute::Playlist(_)
        | PageRoute::Replay(_)
        | PageRoute::Curator(_) => build_detail_skeleton(),
        PageRoute::Artist(_) => build_artist_skeleton(),
        PageRoute::Search => build_grid_skeleton(140),
    };
    b.upcast()
}
