//! Responsive skeleton loading widgets and page layouts.
//!
//! Provides shimmer/pulsing placeholders that match the exact shape of real content
//! to prevent jarring blank states and spinners during navigation.

use crate::design::tokens::ARTWORK_SHELF_SIZE;
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

/// A station / artist circular card placeholder.
pub fn skeleton_station_card(card_width: i32) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    card.set_width_request(card_width);
    card.set_halign(gtk::Align::Center);

    let art = skeleton_circle(card_width);
    card.append(&art);

    let title = skeleton_text(Some((card_width as f64 * 0.70) as i32), 14);
    title.set_halign(gtk::Align::Center);
    card.append(&title);

    let sub = skeleton_text(Some((card_width as f64 * 0.45) as i32), 12);
    sub.set_halign(gtk::Align::Center);
    card.append(&sub);

    card
}

/// Home page skeleton matching official Apple Music Home / Listen Now layout:
/// Page title "Home" + "Top Picks for You" shelf (232px cards) + "Recently Played" shelf (176px cards) + recommendations shelf.
pub fn build_home_skeleton() -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 28);
    container.set_hexpand(true);

    // Page Title: "Home"
    let title = skeleton_text(Some(120), 32);
    title.set_margin_bottom(12);
    container.append(&title);

    // Shelf 1: "Top Picks for You" (standard shelf cards)
    let shelf1 = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let title1 = skeleton_text(Some(180), 22);
    shelf1.append(&title1);
    let row1 = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    for _ in 0..6 {
        row1.append(&skeleton_card(ARTWORK_SHELF_SIZE));
    }
    shelf1.append(&row1);
    container.append(&shelf1);

    // Shelf 2: "Recently Played" (176px cards)
    let shelf2 = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let title2 = skeleton_text(Some(160), 22);
    shelf2.append(&title2);
    let row2 = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    for _ in 0..6 {
        row2.append(&skeleton_card(ARTWORK_SHELF_SIZE));
    }
    shelf2.append(&row2);
    container.append(&shelf2);

    // Shelf 3: Recommendations shelf (176px cards)
    let shelf3 = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let title3 = skeleton_text(Some(150), 22);
    shelf3.append(&title3);
    let row3 = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    for _ in 0..6 {
        row3.append(&skeleton_card(ARTWORK_SHELF_SIZE));
    }
    shelf3.append(&row3);
    container.append(&shelf3);

    container
}

/// Shelves skeleton for New:
/// Hero banner placeholder + horizontal shelves with card placeholders.
pub fn build_shelves_skeleton() -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 28);
    container.set_hexpand(true);

    // Hero banner
    let hero = skeleton_box(None, 220, 12);
    hero.set_margin_bottom(8);
    container.append(&hero);

    // Shelf 1: New Releases
    let shelf1 = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let title1 = skeleton_text(Some(160), 22);
    shelf1.append(&title1);
    let row1 = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    for _ in 0..6 {
        row1.append(&skeleton_card(ARTWORK_SHELF_SIZE));
    }
    shelf1.append(&row1);
    container.append(&shelf1);

    // Shelf 2: Hot Tracks
    let shelf2 = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let title2 = skeleton_text(Some(140), 22);
    shelf2.append(&title2);
    let row2 = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    for _ in 0..6 {
        row2.append(&skeleton_card(ARTWORK_SHELF_SIZE));
    }
    shelf2.append(&row2);
    container.append(&shelf2);

    container
}

/// Radio page skeleton: Hero banner + circular station cards shelf + rectangular station cards shelf.
pub fn build_radio_skeleton() -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 28);
    container.set_hexpand(true);

    // Featured hero banner
    let hero = skeleton_box(None, 220, 12);
    hero.set_margin_bottom(8);
    container.append(&hero);

    // Live stations shelf (circular avatars)
    let shelf1 = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let title1 = skeleton_text(Some(170), 22);
    shelf1.append(&title1);
    let row1 = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    for _ in 0..6 {
        row1.append(&skeleton_station_card(160));
    }
    shelf1.append(&row1);
    container.append(&shelf1);

    // Broadcast stations shelf
    let shelf2 = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let title2 = skeleton_text(Some(190), 22);
    shelf2.append(&title2);
    let row2 = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    for _ in 0..6 {
        row2.append(&skeleton_card(ARTWORK_SHELF_SIZE));
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
        let child = skeleton_card(ARTWORK_SHELF_SIZE);
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

/// Right-hand detail skeleton for an artist in LibraryArtists (used during initial split load and in-place artist switching).
pub fn build_artist_detail_right_skeleton() -> gtk::Box {
    let right = gtk::Box::new(gtk::Orientation::Vertical, 20);
    right.set_hexpand(true);
    right.set_vexpand(true);
    right.set_margin_top(16);
    right.set_margin_start(24);
    right.set_margin_end(24);

    // Artist header: Avatar + Name + Count + Action pills
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    header.append(&skeleton_circle(80));

    let meta = gtk::Box::new(gtk::Orientation::Vertical, 8);
    meta.set_valign(gtk::Align::Center);
    meta.set_hexpand(true);
    meta.append(&skeleton_text(Some(220), 28));
    meta.append(&skeleton_text(Some(100), 14));

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    actions.set_margin_top(4);
    actions.append(&skeleton_box(Some(72), 30, 999));
    actions.append(&skeleton_box(Some(72), 30, 999));
    meta.append(&actions);

    header.append(&meta);
    right.append(&header);

    // Albums Grid
    let flow = gtk::FlowBox::new();
    flow.set_selection_mode(gtk::SelectionMode::None);
    flow.set_homogeneous(true);
    flow.set_column_spacing(16);
    flow.set_row_spacing(20);
    flow.set_valign(gtk::Align::Start);
    flow.set_hexpand(true);
    flow.set_margin_top(12);

    for _ in 0..8 {
        flow.append(&skeleton_card(ARTWORK_SHELF_SIZE));
    }
    right.append(&flow);

    right
}

/// Split view skeleton for LibraryArtists.
pub fn build_split_artists_skeleton() -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    container.set_hexpand(true);
    container.set_vexpand(true);

    // Left master list placeholder
    let left = gtk::Box::new(gtk::Orientation::Vertical, 12);
    left.set_width_request(240);
    left.set_margin_end(8);
    left.set_margin_start(12);
    left.set_margin_top(8);

    // Title: "Artists"
    left.append(&skeleton_text(Some(100), 28));

    // Search bar placeholder: "Find in Artists"
    left.append(&skeleton_box(None, 34, 8));

    // Artist list rows with circular avatars + names
    for i in 0..12 {
        let w = if i % 3 == 0 {
            140
        } else if i % 2 == 0 {
            170
        } else {
            120
        };
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row.set_height_request(38);
        row.set_valign(gtk::Align::Center);
        row.append(&skeleton_circle(28));
        row.append(&skeleton_text(Some(w), 14));
        left.append(&row);
    }
    container.append(&left);

    // Right detail placeholder
    let right = build_artist_detail_right_skeleton();
    container.append(&right);

    container
}

/// Split view skeleton for LibraryGenres.
pub fn build_split_genres_skeleton() -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    container.set_hexpand(true);
    container.set_vexpand(true);

    // Left master list placeholder
    let left = gtk::Box::new(gtk::Orientation::Vertical, 12);
    left.set_width_request(240);
    left.set_margin_end(8);
    left.set_margin_start(12);
    left.set_margin_top(8);

    // Title: "Genres"
    left.append(&skeleton_text(Some(100), 28));

    // Search bar placeholder: "Find in Genres"
    left.append(&skeleton_box(None, 34, 8));

    // Genre rows: Genre name + count badge
    for i in 0..12 {
        let w = if i % 3 == 0 {
            120
        } else if i % 2 == 0 {
            150
        } else {
            100
        };
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.set_height_request(34);
        row.set_valign(gtk::Align::Center);
        let name = skeleton_text(Some(w), 14);
        name.set_hexpand(true);
        row.append(&name);
        row.append(&skeleton_box(Some(28), 18, 999));
        left.append(&row);
    }
    container.append(&left);

    // Right detail placeholder
    let right = gtk::Box::new(gtk::Orientation::Vertical, 20);
    right.set_hexpand(true);
    right.set_margin_top(16);
    right.set_margin_start(24);
    right.set_margin_end(24);

    let detail_header = gtk::Box::new(gtk::Orientation::Vertical, 6);
    detail_header.append(&skeleton_text(Some(180), 28));
    detail_header.append(&skeleton_text(Some(80), 14));
    right.append(&detail_header);

    let flow = gtk::FlowBox::new();
    flow.set_selection_mode(gtk::SelectionMode::None);
    flow.set_homogeneous(true);
    flow.set_column_spacing(16);
    flow.set_row_spacing(20);
    flow.set_valign(gtk::Align::Start);
    flow.set_hexpand(true);

    for _ in 0..8 {
        flow.append(&skeleton_card(ARTWORK_SHELF_SIZE));
    }
    right.append(&flow);

    container.append(&right);
    container
}

/// Search results skeleton matching categorized search layout:
/// Category tabs + Top result card & Top songs + Albums shelf.
pub fn build_search_results_skeleton() -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 24);
    container.set_hexpand(true);

    // Tabs filter bar: 6 pill buttons
    let tabs = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    for w in [90, 60, 65, 70, 75, 70] {
        tabs.append(&skeleton_box(Some(w), 32, 999));
    }
    container.append(&tabs);

    // Top Result + Songs split
    let top_section = gtk::Box::new(gtk::Orientation::Horizontal, 24);
    top_section.set_hexpand(true);

    // Top Result Card placeholder
    let top_card = gtk::Box::new(gtk::Orientation::Vertical, 12);
    top_card.set_width_request(220);
    let card_title = skeleton_text(Some(100), 20);
    top_card.append(&card_title);
    let art = skeleton_box(Some(220), 220, 10);
    top_card.append(&art);
    top_card.append(&skeleton_text(Some(160), 16));
    top_card.append(&skeleton_text(Some(100), 13));
    top_section.append(&top_card);

    // Songs column placeholder (4 track rows)
    let songs_col = gtk::Box::new(gtk::Orientation::Vertical, 6);
    songs_col.set_hexpand(true);
    let songs_title = skeleton_text(Some(60), 20);
    songs_col.append(&songs_title);
    for _ in 0..4 {
        songs_col.append(&skeleton_track_row());
    }
    top_section.append(&songs_col);

    container.append(&top_section);

    // Albums Shelf placeholder
    let albums_shelf = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let shelf_title = skeleton_text(Some(80), 20);
    albums_shelf.append(&shelf_title);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    for _ in 0..5 {
        row.append(&skeleton_card(ARTWORK_SHELF_SIZE));
    }
    albums_shelf.append(&row);
    container.append(&albums_shelf);

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
        row.append(&skeleton_card(ARTWORK_SHELF_SIZE));
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
        PageRoute::Home => build_home_skeleton(),
        PageRoute::New => build_shelves_skeleton(),
        PageRoute::Radio => build_radio_skeleton(),
        PageRoute::LibraryAlbums
        | PageRoute::LibraryRecentlyAdded
        | PageRoute::LibraryPlaylists
        | PageRoute::LibraryMadeForYou => build_grid_skeleton(180),
        PageRoute::LibrarySongs => build_tracks_skeleton(),
        PageRoute::LibraryArtists => build_split_artists_skeleton(),
        PageRoute::LibraryGenres => build_split_genres_skeleton(),
        PageRoute::Album(_)
        | PageRoute::Playlist(_)
        | PageRoute::Replay(_)
        | PageRoute::Curator(_) => build_detail_skeleton(),
        PageRoute::Artist(_) => build_artist_skeleton(),
        PageRoute::Search => build_search_results_skeleton(),
    };
    b.upcast()
}
