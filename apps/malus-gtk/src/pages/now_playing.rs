//! One immersive page with Player / Lyrics presentation modes.
//!
//! Two deliberate responsive compositions sharing the SAME state/components:
//!
//! WIDE (≥880px): artwork/player left, synced lyrics right.
//! X close top-left, lyrics button top-right.
//!
//! COMPACT (<880px): compact track header (small 64px artwork, title, artist,
//! fav/more), lyrics scrollable, transport anchored near bottom,
//! down-chevron dismiss centered at top, no giant artwork.
use crate::{
    design::tokens::*,
    navigation::AppDestination,
    panes::{
        lyrics::LyricsView,
        queue::{QueueInput, QueuePane},
    },
    services::{ArtworkService, DecodedImage, artwork_texture, generate_backdrop_texture},
    state::{NowPlayingMode, SharedPlayer},
    widgets::{player_controls::*, square_artwork::SquareArtwork},
};
use malus_client::MalusClient;
use malus_model::{PageRoute, PlaybackState, Queue, Track};
use relm4::{
    Controller,
    gtk::{self, prelude::*},
    prelude::*,
};
use std::{cell::Cell, rc::Rc};

pub struct NowPlayingPage {
    pub root: gtk::Overlay,
    mode: Rc<Cell<NowPlayingMode>>,
    wide_scroll: gtk::ScrolledWindow,
    last_layout: Rc<Cell<(i32, i32, NowPlayingMode, bool)>>,
    // Wide layout widgets
    artwork: SquareArtwork,
    title: gtk::Label,
    subtitle_box: gtk::Box,
    favorite: gtk::Button,
    more: gtk::MenuButton,
    transport: Transport,
    seek: SeekControl,
    volume: VolumeControl,
    pub lyrics: LyricsView,
    queue: Controller<QueuePane>,
    // Compact layout widgets (separate instances)
    compact_artwork: SquareArtwork,
    compact_player_artwork: SquareArtwork,
    compact_lyrics_button: gtk::Button,
    compact_queue_button: gtk::Button,
    compact_title: gtk::Label,
    compact_subtitle_box: gtk::Box,
    compact_favorite: gtk::Button,
    compact_more: gtk::MenuButton,
    compact_mini_title: gtk::Label,
    compact_mini_subtitle_box: gtk::Box,
    compact_mini_favorite: gtk::Button,
    compact_mini_more: gtk::MenuButton,
    compact_transport: Transport,
    compact_seek: SeekControl,
    compact_volume: VolumeControl,
    compact_lyrics: LyricsView,
    compact_queue: Controller<QueuePane>,
    // Shared
    backdrops: [gtk::Picture; 2],
    backdrop_stack: gtk::Stack,
    backdrop_index: Cell<usize>,
    lyrics_button: gtk::Button,
    queue_button: gtk::Button,
    navigate: Rc<dyn Fn(AppDestination) + 'static>,
    last_track_fingerprint: std::cell::RefCell<Option<String>>,
}

impl NowPlayingPage {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        client: MalusClient,
        artwork_service: ArtworkService,
        player: &SharedPlayer,
        send: &CommandHandler,
        menu: &MenuHandler,
        close: impl Fn() + 'static,
        toggle_lyrics: impl Fn() + 'static,
        toggle_queue: impl Fn() + 'static,
        expand_player: impl Fn() + 'static,
        navigate: impl Fn(AppDestination) + 'static,
    ) -> Self {
        let toggle_lyrics = Rc::new(toggle_lyrics);
        let toggle_queue = Rc::new(toggle_queue);
        let expand_player = Rc::new(expand_player);
        let root = gtk::Overlay::new();
        root.add_css_class("now-playing");
        root.set_hexpand(true);
        root.set_vexpand(true);
        root.set_overflow(gtk::Overflow::Hidden);

        // --- Backdrop ---
        let backdrop_stack = gtk::Stack::new();
        backdrop_stack.set_transition_type(gtk::StackTransitionType::Crossfade);
        backdrop_stack.set_transition_duration(650);
        let backdrops = [gtk::Picture::new(), gtk::Picture::new()];
        for (i, picture) in backdrops.iter().enumerate() {
            picture.set_can_shrink(true);
            picture.set_content_fit(gtk::ContentFit::Cover);
            picture.set_hexpand(true);
            picture.set_vexpand(true);
            picture.set_can_target(false);
            backdrop_stack.add_named(picture, Some(&i.to_string()));
        }
        root.set_child(Some(&backdrop_stack));
        let weak_pictures = [backdrops[0].downgrade(), backdrops[1].downgrade()];
        backdrop_stack.connect_transition_running_notify(move |stack| {
            if !stack.is_transition_running() {
                for picture in weak_pictures.iter().filter_map(|picture| picture.upgrade()) {
                    if stack.visible_child().as_ref() != Some(picture.upcast_ref()) {
                        picture.set_paintable(None::<&gtk::gdk::Texture>);
                    }
                }
            }
        });

        // === WIDE LAYOUT ===
        let wide_scroll = gtk::ScrolledWindow::new();
        wide_scroll.set_hscrollbar_policy(gtk::PolicyType::Automatic);
        wide_scroll.set_vscrollbar_policy(gtk::PolicyType::Automatic);
        wide_scroll.set_hexpand(true);
        wide_scroll.set_vexpand(true);
        wide_scroll.set_margin_top(56);
        wide_scroll.set_margin_bottom(24);

        let composition = gtk::Box::new(gtk::Orientation::Horizontal, 88);
        composition.set_halign(gtk::Align::Center);
        composition.set_valign(gtk::Align::Center);
        composition.set_margin_start(32);
        composition.set_margin_end(32);

        let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
        column.set_halign(gtk::Align::Center);
        column.set_valign(gtk::Align::Center);

        let artwork = SquareArtwork::new(460, "nowplaying-artwork");
        artwork.set_margin_bottom(18);
        column.append(&artwork);

        let title = gtk::Label::new(Some("Not Playing"));
        title.add_css_class("nowplaying-title");
        title.set_xalign(0.0);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        title.set_max_width_chars(1);
        title.set_hexpand(true);
        title.set_margin_bottom(2);

        let subtitle_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        subtitle_box.set_hexpand(true);
        subtitle_box.set_halign(gtk::Align::Start);

        let text_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        text_box.set_hexpand(true);
        text_box.set_valign(gtk::Align::Center);
        text_box.append(&title);
        text_box.append(&subtitle_box);

        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        actions.set_valign(gtk::Align::Center);
        actions.set_halign(gtk::Align::End);
        let favorite = favorite_button(player, menu);
        favorite.add_css_class("np-control");
        favorite.set_focus_on_click(false);
        let more = more_button(player, menu);
        actions.append(&favorite);
        actions.append(&more);

        let meta_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        meta_row.set_hexpand(true);
        meta_row.set_valign(gtk::Align::Center);
        meta_row.set_margin_top(4);
        meta_row.set_margin_bottom(8);
        meta_row.append(&text_box);
        meta_row.append(&actions);
        column.append(&meta_row);

        let seek = SeekControl::new(player, send, true);
        column.append(&seek.root);

        let transport = Transport::new(player, send, true);
        transport.root.set_margin_top(6);
        transport.root.set_margin_bottom(4);
        column.append(&transport.root);

        let volume = VolumeControl::new_wide(send);
        volume.root.set_hexpand(true);
        volume.root.set_halign(gtk::Align::Fill);
        volume.root.set_margin_top(4);
        column.append(&volume.root);

        composition.append(&column);

        let lyrics = LyricsView::new(player, send, true);
        composition.append(&lyrics.root);
        lyrics.root.set_visible(false);

        let q_close1 = toggle_queue.clone();
        let queue = QueuePane::builder()
            .launch((client.clone(), artwork_service.clone(), q_close1, true))
            .detach();
        composition.append(queue.widget());
        queue.widget().set_visible(false);

        wide_scroll.set_child(Some(&composition));
        root.add_overlay(&wide_scroll);

        // === COMPACT LAYOUT ===
        let compact_root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        compact_root.set_hexpand(true);
        compact_root.set_vexpand(true);
        compact_root.set_visible(false);

        // --- Player Mode View (Artwork centered + Metadata Row below it) ---
        let compact_player_view = gtk::Box::new(gtk::Orientation::Vertical, 0);
        compact_player_view.set_vexpand(true);
        compact_player_view.set_valign(gtk::Align::Center);
        compact_player_view.set_halign(gtk::Align::Center);

        let compact_art_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        compact_art_box.set_halign(gtk::Align::Center);
        compact_art_box.set_valign(gtk::Align::Center);
        compact_art_box.set_margin_top(48);
        let compact_player_artwork = SquareArtwork::new(280, "nowplaying-artwork");
        compact_art_box.append(&compact_player_artwork);
        compact_player_view.append(&compact_art_box);

        let compact_meta_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        compact_meta_row.set_margin_top(4);
        compact_meta_row.set_margin_bottom(8);
        compact_meta_row.set_halign(gtk::Align::Fill);
        compact_meta_row.set_hexpand(true);

        let compact_meta = gtk::Box::new(gtk::Orientation::Vertical, 2);
        compact_meta.set_valign(gtk::Align::Center);
        compact_meta.set_hexpand(true);

        let compact_title = gtk::Label::new(Some("Not Playing"));
        compact_title.add_css_class("nowplaying-title");
        compact_title.set_xalign(0.0);
        compact_title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        compact_title.set_max_width_chars(1);
        compact_title.set_hexpand(true);

        let compact_subtitle_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        compact_subtitle_box.set_hexpand(true);
        compact_subtitle_box.set_halign(gtk::Align::Start);

        compact_meta.append(&compact_title);
        compact_meta.append(&compact_subtitle_box);
        compact_meta_row.append(&compact_meta);

        let compact_meta_actions = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        compact_meta_actions.set_valign(gtk::Align::Center);
        compact_meta_actions.set_halign(gtk::Align::End);
        let compact_favorite = favorite_button(player, menu);
        compact_favorite.add_css_class("np-control");
        compact_favorite.set_focus_on_click(false);
        let compact_more = more_button(player, menu);
        compact_more.add_css_class("np-control");
        compact_meta_actions.append(&compact_favorite);
        compact_meta_actions.append(&compact_more);
        compact_meta_row.append(&compact_meta_actions);

        compact_player_view.append(&compact_meta_row);
        compact_root.append(&compact_player_view);

        // --- Lyrics Mini Header (Small artwork + title/artist + fav/more) ---
        let compact_mini_header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        compact_mini_header.set_halign(gtk::Align::Center);
        compact_mini_header.set_margin_start(0);
        compact_mini_header.set_margin_end(0);
        compact_mini_header.set_margin_top(48);
        compact_mini_header.set_margin_bottom(8);
        compact_mini_header.set_visible(false);

        let compact_artwork = SquareArtwork::new(52, "compact-mini-artwork");
        compact_artwork.set_hexpand(false);
        compact_artwork.set_vexpand(false);
        compact_artwork.set_cursor_from_name(Some("pointer"));
        let exp1 = expand_player.clone();
        let art_click = gtk::GestureClick::new();
        art_click.connect_pressed(move |_, _, _, _| {
            exp1();
        });
        compact_artwork.add_controller(art_click);
        compact_mini_header.append(&compact_artwork);

        let compact_mini_meta = gtk::Box::new(gtk::Orientation::Vertical, 2);
        compact_mini_meta.set_valign(gtk::Align::Center);
        compact_mini_meta.set_hexpand(true);
        compact_mini_meta.set_cursor_from_name(Some("pointer"));
        let exp2 = expand_player.clone();
        let meta_click = gtk::GestureClick::new();
        meta_click.connect_pressed(move |_, _, _, _| {
            exp2();
        });
        compact_mini_meta.add_controller(meta_click);

        let compact_mini_title = gtk::Label::new(Some("Not Playing"));
        compact_mini_title.add_css_class("compact-mini-title");
        compact_mini_title.set_xalign(0.0);
        compact_mini_title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        compact_mini_title.set_max_width_chars(1);
        compact_mini_title.set_hexpand(true);

        let compact_mini_subtitle_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        compact_mini_subtitle_box.set_hexpand(true);
        compact_mini_subtitle_box.set_halign(gtk::Align::Start);

        compact_mini_meta.append(&compact_mini_title);
        compact_mini_meta.append(&compact_mini_subtitle_box);
        compact_mini_header.append(&compact_mini_meta);

        let compact_mini_actions = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        compact_mini_actions.set_valign(gtk::Align::Center);
        compact_mini_actions.set_halign(gtk::Align::End);
        let compact_mini_favorite = favorite_button(player, menu);
        compact_mini_favorite.add_css_class("np-control");
        compact_mini_favorite.set_focus_on_click(false);
        let compact_mini_more = more_button(player, menu);
        compact_mini_more.add_css_class("np-control");
        compact_mini_actions.append(&compact_mini_favorite);
        compact_mini_actions.append(&compact_mini_more);
        compact_mini_header.append(&compact_mini_actions);

        compact_root.append(&compact_mini_header);

        // Compact lyrics: its own LyricsView instance, vexpands to fill
        let compact_lyrics = LyricsView::new(player, send, true);
        compact_lyrics.root.set_vexpand(true);
        compact_lyrics.root.set_halign(gtk::Align::Center);
        compact_lyrics.root.set_visible(false);
        compact_root.append(&compact_lyrics.root);

        let q_close2 = toggle_queue.clone();
        let compact_queue = QueuePane::builder()
            .launch((client, artwork_service, q_close2, true))
            .detach();
        compact_queue.widget().set_vexpand(true);
        compact_queue.widget().set_halign(gtk::Align::Center);
        compact_queue.widget().set_visible(false);
        compact_queue.widget().set_margin_top(0);
        compact_root.append(compact_queue.widget());

        // --- Compact Controls Box (Scrubber, Transport, Volume) ---
        let compact_controls_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        compact_controls_box.set_halign(gtk::Align::Center);
        compact_controls_box.set_margin_top(12);

        // Compact seek bar
        let compact_seek = SeekControl::new(player, send, true);
        compact_controls_box.append(&compact_seek.root);

        // Compact transport: Prev / Play / Next / Shuffle / Repeat
        let compact_transport = Transport::new(player, send, true);
        compact_transport.root.set_margin_top(6);
        compact_transport.root.set_margin_bottom(4);
        compact_controls_box.append(&compact_transport.root);

        // Compact volume: wide with speaker icons on both sides
        let compact_volume = VolumeControl::new_wide(send);
        compact_volume.root.set_margin_top(4);
        compact_volume.root.set_margin_bottom(8);
        compact_controls_box.append(&compact_volume.root);

        compact_root.append(&compact_controls_box);

        // Bottom row: Lyrics / Queue toggle buttons
        let compact_bottom = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        compact_bottom.set_halign(gtk::Align::Center);
        compact_bottom.set_margin_bottom(16);
        compact_bottom.set_spacing(80);
        let compact_lyrics_button = icon_button(ICON_LYRICS, "Lyrics", "np-control");
        compact_lyrics_button.add_css_class("control-inactive");
        let compact_queue_button = icon_button(ICON_QUEUE, "Queue", "np-control");
        compact_queue_button.add_css_class("control-inactive");
        compact_bottom.append(&compact_lyrics_button);
        compact_bottom.append(&compact_queue_button);
        let toggle = toggle_lyrics.clone();
        compact_lyrics_button.connect_clicked(move |_| toggle());
        let q_compact = toggle_queue.clone();
        compact_queue_button.connect_clicked(move |_| q_compact());
        compact_root.append(&compact_bottom);

        root.add_overlay(&compact_root);

        // === HEADER OVERLAY ===
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        header.set_halign(gtk::Align::Fill);
        header.set_valign(gtk::Align::Start);
        header.set_margin_start(20);
        header.set_margin_end(20);
        header.set_margin_top(12);

        // Wide: X close left
        let close_x = icon_button(
            "window-close-symbolic",
            "Close Now Playing (Esc)",
            "np-control",
        );
        let close_rc = Rc::new(close);
        let c1 = close_rc.clone();
        close_x.connect_clicked(move |_| c1());
        header.append(&close_x);

        // Spacer (visible only in compact for centering chevron)
        let spacer_left = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer_left.set_hexpand(true);
        header.append(&spacer_left);

        // Compact: centered down-chevron
        let close_chevron = icon_button("go-down-symbolic", "Close Now Playing", "np-control");
        close_chevron.set_halign(gtk::Align::Center);
        let c2 = close_rc.clone();
        close_chevron.connect_clicked(move |_| c2());
        header.append(&close_chevron);

        let spacer_right = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer_right.set_hexpand(true);
        header.append(&spacer_right);

        // Wide: lyrics & queue toggle bottom right
        let bottom_bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        bottom_bar.set_halign(gtk::Align::End);
        bottom_bar.set_valign(gtk::Align::End);
        bottom_bar.add_css_class("np-bottom-bar");
        let lyrics_button = icon_button(ICON_LYRICS, "Toggle Now Playing Lyrics", "np-control");
        lyrics_button.add_css_class("control-inactive");
        let toggle_rc = toggle_lyrics;
        let t1 = toggle_rc.clone();
        lyrics_button.connect_clicked(move |_| t1());
        let queue_button = icon_button(ICON_QUEUE, "Toggle Now Playing Queue", "np-control");
        queue_button.add_css_class("control-inactive");
        let q1 = toggle_queue.clone();
        queue_button.connect_clicked(move |_| q1());
        bottom_bar.append(&lyrics_button);
        bottom_bar.append(&queue_button);
        root.add_overlay(&bottom_bar);

        let handle = gtk::WindowHandle::new();
        handle.set_valign(gtk::Align::Start);
        handle.set_hexpand(true);
        handle.set_child(Some(&header));
        root.add_overlay(&handle);

        let p_rc = player.clone();
        let m_rc = menu.clone();
        let root_weak = root.downgrade();
        let rc_gesture = gtk::GestureClick::new();
        rc_gesture.set_button(gtk::gdk::BUTTON_SECONDARY);
        rc_gesture.connect_pressed(move |_g, _n, x, y| {
            let Some(r) = root_weak.upgrade() else {
                return;
            };
            popup_track_context_menu(&r, x, y, &p_rc, &m_rc);
        });
        root.add_controller(rc_gesture);

        // === RESPONSIVE TICK ===
        let mode = Rc::new(Cell::new(NowPlayingMode::Player));
        let m = mode.clone();
        let art = artwork.clone();
        let l = lyrics.root.clone();
        let qw_ref = queue.widget().clone();
        let col_ref = column.clone();
        let comp_ref = composition.clone();
        let ws_ref = wide_scroll.clone();
        let cr_ref = compact_root.clone();
        let cx_ref = close_x.clone();
        let cv_ref = close_chevron.clone();
        let sr_ref = spacer_right.clone();
        let lb_ref = lyrics_button.clone();
        let qb_ref = queue_button.clone();
        let compact_lyrics_root = compact_lyrics.root.clone();
        let compact_qw_ref = compact_queue.widget().clone();
        let compact_player_view_ref = compact_player_view.clone();
        let compact_mini_hdr_ref = compact_mini_header.clone();
        let compact_art = compact_player_artwork.clone();
        let compact_art_box_ref = compact_art_box.clone();
        let compact_seek_ref = compact_seek.root.clone();
        let compact_trans_ref = compact_transport.root.clone();
        let compact_vol_ref = compact_volume.root.clone();
        let compact_controls_ref = compact_controls_box.clone();
        let compact_bottom_ref = compact_bottom.clone();
        let compact_meta_ref = compact_meta_row.clone();
        let state = player.clone();
        let last_layout = Rc::new(Cell::new((0, 0, NowPlayingMode::Player, false)));
        let last_clone = last_layout.clone();
        let ws_map = wide_scroll.clone();
        root.connect_map(move |_| {
            // Force re-layout on map so we never retain stale unallocated measurements
            last_clone.set((0, 0, NowPlayingMode::Player, false));
            ws_map.vadjustment().set_value(0.0);
        });

        let last = last_layout.clone();

        root.add_tick_callback(move |root, _| {
            let width = root.width();
            let height = root.height();
            let mode = m.get();
            let has_lyrics = state
                .borrow()
                .lyrics
                .content
                .as_ref()
                .is_some_and(|lyrics| !lyrics.lines.is_empty());
            if last.replace((width, height, mode, has_lyrics)) != (width, height, mode, has_lyrics)
            {
                let is_wide = width >= 880;
                let show_lyrics = mode == NowPlayingMode::Lyrics;
                let show_queue = mode == NowPlayingMode::Queue;
                let split = (show_lyrics || show_queue) && is_wide;

                // Toggle composition visibility
                ws_ref.set_visible(is_wide);
                cr_ref.set_visible(!is_wide);

                // Header visibility
                cx_ref.set_visible(is_wide);
                cv_ref.set_visible(!is_wide);
                // spacer_left always visible: centers chevron in compact
                sr_ref.set_visible(!is_wide);
                lb_ref.set_visible(is_wide);
                qb_ref.set_visible(is_wide);

                if is_wide {
                    root.remove_css_class("np-narrow");
                    if width >= 1200 && height >= 700 {
                        root.add_css_class("np-large");
                    } else {
                        root.remove_css_class("np-large");
                    }
                    comp_ref.set_spacing((width / 16).clamp(40, 96));
                    comp_ref.set_margin_start(32);
                    comp_ref.set_margin_end(32);

                    // Measure controls using natural size (.1) to accurately account for
                    // styled button paddings, icon glyph sizes, and metadata font sizes.
                    let measured_controls = col_ref
                        .measure(gtk::Orientation::Vertical, -1)
                        .1
                        .max(col_ref.measure(gtk::Orientation::Vertical, -1).0)
                        - art.measure(gtk::Orientation::Vertical, -1).1
                        + art.margin_top()
                        + art.margin_bottom();
                    // Safe floor: when unmapped or before CSS styles have settled, GTK measures unstyled
                    // controls (~176px). Fully styled with font-size 24/16 and button paddings, controls
                    // require at least 216px (or ~228px with np-large). Enforce a realistic floor so
                    // artwork is never sized with unstyled estimates that cause the volume slider to overflow.
                    let controls_floor = if width >= 1200 && height >= 700 {
                        228
                    } else {
                        216
                    };
                    let controls_height = measured_controls.max(controls_floor);

                    // 80px scroll margins (56 top + 24 bottom) + 16px safety headroom = 96px
                    let available_art = (height - 96 - controls_height).max(96);
                    let side = if split {
                        ((width - 176) / 2).min(available_art).clamp(96, 620)
                    } else {
                        available_art.min(width - 48).clamp(96, 680)
                    };
                    art.set_side(side);
                    col_ref.set_width_request(side);

                    // If total column fits in viewport, center vertically. If window is extremely short,
                    // align to start so scroll position 0 shows the top cleanly.
                    let total_col_height = side + controls_height + 80;
                    if height >= total_col_height {
                        comp_ref.set_valign(gtk::Align::Center);
                    } else {
                        comp_ref.set_valign(gtk::Align::Start);
                    }
                    ws_ref.vadjustment().set_value(0.0);

                    let right_width = if split {
                        (width - side - 176).clamp(320, 720)
                    } else {
                        side
                    };
                    let right_height = if split { (height - 112).max(400) } else { 460 };

                    l.set_visible(show_lyrics);
                    l.set_size_request(right_width, right_height);
                    l.set_valign(gtk::Align::Center);

                    qw_ref.set_visible(show_queue);
                    qw_ref.set_size_request(right_width, right_height);
                    qw_ref.set_valign(gtk::Align::Center);
                } else {
                    root.remove_css_class("np-large");
                    root.add_css_class("np-narrow");
                    let lyric_layout = show_lyrics;
                    let queue_layout = show_queue;

                    compact_vol_ref.set_visible(true);
                    compact_controls_ref.set_visible(true);
                    compact_bottom_ref.set_visible(true);

                    let controls_h = (compact_meta_ref.measure(gtk::Orientation::Vertical, -1).1
                        + compact_seek_ref.measure(gtk::Orientation::Vertical, -1).1
                        + compact_trans_ref.measure(gtk::Orientation::Vertical, -1).1
                        + compact_vol_ref.measure(gtk::Orientation::Vertical, -1).1
                        + compact_bottom_ref.measure(gtk::Orientation::Vertical, -1).1
                        + 60)
                        .max(280);
                    let available_compact_art = (height - controls_h).max(96);
                    let full_mode_art = (height - 96 - 216).max(96);
                    let side = (width - 48)
                        .min(available_compact_art)
                        .min(full_mode_art)
                        .clamp(96, 460);

                    compact_art.set_side(side);
                    compact_art_box_ref.set_width_request(side);
                    compact_art_box_ref.set_height_request(side);
                    compact_player_view_ref.set_width_request(side);
                    compact_controls_ref.set_width_request(side);
                    compact_mini_hdr_ref.set_width_request(side);
                    compact_lyrics_root.set_width_request(side);
                    compact_qw_ref.set_width_request(side);

                    if queue_layout {
                        compact_player_view_ref.set_visible(false);
                        compact_mini_hdr_ref.set_visible(true);
                        compact_lyrics_root.set_visible(false);
                        compact_qw_ref.set_visible(true);
                        compact_controls_ref.set_visible(true);
                    } else if lyric_layout {
                        compact_player_view_ref.set_visible(false);
                        compact_mini_hdr_ref.set_visible(true);
                        compact_lyrics_root.set_visible(true);
                        compact_qw_ref.set_visible(false);
                        compact_controls_ref.set_visible(true);
                    } else {
                        compact_player_view_ref.set_visible(true);
                        compact_mini_hdr_ref.set_visible(false);
                        compact_lyrics_root.set_visible(false);
                        compact_qw_ref.set_visible(false);
                        compact_controls_ref.set_visible(true);
                    }
                }
            }
            gtk::glib::ControlFlow::Continue
        });

        let navigate = Rc::new(navigate);

        Self {
            root,
            mode,
            wide_scroll,
            last_layout,
            artwork,
            title,
            subtitle_box,
            favorite,
            more,
            transport,
            seek,
            volume,
            lyrics,
            queue,
            compact_artwork,
            compact_player_artwork,
            compact_lyrics_button,
            compact_queue_button,
            compact_title,
            compact_subtitle_box,
            compact_favorite,
            compact_more,
            compact_mini_title,
            compact_mini_subtitle_box,
            compact_mini_favorite,
            compact_mini_more,
            compact_transport,
            compact_seek,
            compact_volume,
            compact_lyrics,
            compact_queue,
            backdrops,
            backdrop_stack,
            backdrop_index: Cell::new(0),
            lyrics_button,
            queue_button,
            navigate,
            last_track_fingerprint: std::cell::RefCell::new(None),
        }
    }
    pub fn set_mode(&self, mode: NowPlayingMode) {
        self.mode.set(mode);
        self.last_layout.set((0, 0, NowPlayingMode::Player, false));
        self.wide_scroll.vadjustment().set_value(0.0);
        self.root.queue_resize();
        match mode {
            NowPlayingMode::Lyrics => {
                self.lyrics_button.add_css_class("control-active");
                self.lyrics_button.remove_css_class("control-inactive");
                self.compact_lyrics_button.add_css_class("control-active");
                self.compact_lyrics_button
                    .remove_css_class("control-inactive");
                self.queue_button.remove_css_class("control-active");
                self.queue_button.add_css_class("control-inactive");
                self.compact_queue_button.remove_css_class("control-active");
                self.compact_queue_button.add_css_class("control-inactive");
                self.compact_lyrics.reveal_current();
                self.lyrics.reveal_current();
            }
            NowPlayingMode::Queue => {
                self.lyrics_button.remove_css_class("control-active");
                self.lyrics_button.add_css_class("control-inactive");
                self.compact_lyrics_button
                    .remove_css_class("control-active");
                self.compact_lyrics_button.add_css_class("control-inactive");
                self.queue_button.add_css_class("control-active");
                self.queue_button.remove_css_class("control-inactive");
                self.compact_queue_button.add_css_class("control-active");
                self.compact_queue_button
                    .remove_css_class("control-inactive");
                self.queue.emit(QueueInput::ScrollToNowPlaying);
                self.compact_queue.emit(QueueInput::ScrollToNowPlaying);
            }
            NowPlayingMode::Player => {
                self.lyrics_button.remove_css_class("control-active");
                self.lyrics_button.add_css_class("control-inactive");
                self.compact_lyrics_button
                    .remove_css_class("control-active");
                self.compact_lyrics_button.add_css_class("control-inactive");
                self.queue_button.remove_css_class("control-active");
                self.queue_button.add_css_class("control-inactive");
                self.compact_queue_button.remove_css_class("control-active");
                self.compact_queue_button.add_css_class("control-inactive");
            }
        }
    }
    pub fn set_queue(&self, queue: Queue) {
        self.queue.emit(QueueInput::SetQueue(queue.clone()));
        self.compact_queue.emit(QueueInput::SetQueue(queue));
    }
    pub fn set_playback_state(&self, state: PlaybackState) {
        self.queue.emit(QueueInput::SetPlaybackState(state));
        self.compact_queue.emit(QueueInput::SetPlaybackState(state));
    }
    pub fn set_autoplay(&self, autoplay: bool) {
        self.queue.emit(QueueInput::SetAutoplay(autoplay));
        self.compact_queue.emit(QueueInput::SetAutoplay(autoplay));
    }
    pub fn reload_queue(&self) {
        self.queue.emit(QueueInput::Reload);
        self.compact_queue.emit(QueueInput::Reload);
    }
    pub fn scroll_to_now_playing(&self) {
        self.queue.emit(QueueInput::ScrollToNowPlaying);
        self.compact_queue.emit(QueueInput::ScrollToNowPlaying);
    }
    pub fn refresh(&self, player: &SharedPlayer) {
        let state = player.borrow();
        let current_fingerprint = track_fingerprint(state.now.current_track.as_ref());
        let track_changed =
            self.last_track_fingerprint.borrow().as_ref() != Some(&current_fingerprint);

        if track_changed {
            *self.last_track_fingerprint.borrow_mut() = Some(current_fingerprint);

            let title_text = state
                .now
                .current_track
                .as_ref()
                .map(|t| t.title.as_str())
                .unwrap_or("Not Playing");

            self.title.set_text(title_text);
            self.compact_title.set_text(title_text);
            self.compact_mini_title.set_text(title_text);

            populate_artists_box(
                &self.subtitle_box,
                state.now.current_track.as_ref(),
                &self.navigate,
                "nowplaying-artist",
                "nowplaying-artist-sep",
            );
            populate_artists_box(
                &self.compact_subtitle_box,
                state.now.current_track.as_ref(),
                &self.navigate,
                "nowplaying-artist",
                "nowplaying-artist-sep",
            );
            populate_artists_box(
                &self.compact_mini_subtitle_box,
                state.now.current_track.as_ref(),
                &self.navigate,
                "compact-mini-artist",
                "nowplaying-artist-sep",
            );
        }

        let has_track = state.now.current_track.is_some();
        self.more.set_sensitive(has_track);
        self.compact_more.set_sensitive(has_track);
        self.compact_mini_more.set_sensitive(has_track);
        drop(state);

        self.transport.refresh(player);
        self.compact_transport.refresh(player);
        refresh_favorite(&self.favorite, player);
        refresh_favorite(&self.compact_favorite, player);
        refresh_favorite(&self.compact_mini_favorite, player);
        self.tick(player);
    }
    pub fn tick(&self, player: &SharedPlayer) {
        if self.root.width() >= 880 {
            self.seek.refresh(player);
            self.volume.refresh(player);
            self.lyrics.refresh();
        } else {
            self.compact_seek.refresh(player);
            self.compact_volume.refresh(player);
            self.compact_lyrics.refresh();
        }
    }
    pub fn set_artwork(&self, image: Option<&DecodedImage>) {
        let texture = image.map(artwork_texture);
        self.artwork.picture().set_paintable(texture.as_ref());
        self.compact_player_artwork
            .picture()
            .set_paintable(texture.as_ref());
        self.compact_artwork
            .picture()
            .set_paintable(texture.as_ref());
        let next = 1 - self.backdrop_index.get();
        let backdrop_texture = image.and_then(generate_backdrop_texture);
        self.backdrops[next].set_paintable(backdrop_texture.as_ref());
        self.backdrop_stack.set_visible_child(&self.backdrops[next]);
        self.backdrop_index.set(next);
        // GTK consumes both textures while crossfading. Unmapped/no-animation
        // updates don't emit a running transition; release that inactive image now.
        if !self.backdrop_stack.is_transition_running() {
            self.backdrops[1 - next].set_paintable(None::<&gtk::gdk::Texture>);
        }
    }
    pub fn clear_backdrops(&self) {
        self.backdrops[0].set_paintable(None::<&relm4::gtk::gdk::Texture>);
        self.backdrops[1].set_paintable(None::<&relm4::gtk::gdk::Texture>);
    }
    pub fn cancel_interactions(&self) {
        self.seek.slider.cancel();
        self.volume.slider.cancel();
        self.compact_seek.slider.cancel();
        self.compact_volume.slider.cancel();
    }
}

fn track_fingerprint(track: Option<&Track>) -> String {
    let Some(t) = track else {
        return String::new();
    };
    let album_id = t
        .album
        .as_ref()
        .and_then(|a| a.id.as_ref())
        .map(|r| r.to_string())
        .unwrap_or_default();
    let album_title = t.album_title().unwrap_or_default();
    let artists_str = t
        .artists
        .iter()
        .map(|a| {
            format!(
                "{}:{}",
                a.id.as_ref().map(|r| r.to_string()).unwrap_or_default(),
                a.name
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{}:{}:{}:{}:{}",
        t.id, t.title, artists_str, album_id, album_title
    )
}

fn populate_artists_box(
    container: &gtk::Box,
    track: Option<&Track>,
    navigate: &Rc<dyn Fn(AppDestination) + 'static>,
    artist_css_class: &str,
    sep_css_class: &str,
) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    let Some(t) = track else {
        let lbl = gtk::Label::new(Some("Choose a song from your library"));
        lbl.add_css_class(artist_css_class);
        lbl.set_xalign(0.0);
        container.append(&lbl);
        return;
    };

    if t.artists.is_empty() {
        let disp = t.artist_display();
        let name = if disp.is_empty() {
            "Unknown Artist".to_string()
        } else {
            disp
        };
        let lbl = gtk::Label::new(Some(&name));
        lbl.add_css_class(artist_css_class);
        lbl.set_xalign(0.0);
        lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
        container.append(&lbl);
    } else {
        for (i, artist) in t.artists.iter().enumerate() {
            if i > 0 {
                let sep = gtk::Label::new(Some(", "));
                sep.add_css_class(sep_css_class);
                container.append(&sep);
            }
            let lbl = gtk::Label::new(Some(&artist.name));
            lbl.add_css_class(artist_css_class);
            lbl.set_xalign(0.0);
            lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
            if let Some(ref id) = artist.id {
                lbl.add_css_class("metadata-link");
                lbl.set_cursor_from_name(Some("pointer"));
                let nav = navigate.clone();
                let dest = AppDestination::Page(PageRoute::Artist(id.id().to_string()));
                let g = gtk::GestureClick::new();
                g.connect_released(move |g, n, _, _| {
                    if n == 1 {
                        g.set_state(gtk::EventSequenceState::Claimed);
                        nav(dest.clone());
                    }
                });
                lbl.add_controller(g);
            }
            container.append(&lbl);
        }
    }

    if let Some(album) = t.album_title()
        && album != t.title
        && !album.is_empty()
    {
        let sep = gtk::Label::new(Some(" · "));
        sep.add_css_class(sep_css_class);
        container.append(&sep);

        let lbl = gtk::Label::new(Some(album));
        lbl.add_css_class(artist_css_class);
        lbl.set_xalign(0.0);
        lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
        if let Some(id) = t.album.as_ref().and_then(|a| a.id.as_ref()) {
            lbl.add_css_class("metadata-link");
            lbl.set_cursor_from_name(Some("pointer"));
            let nav = navigate.clone();
            let dest = AppDestination::Page(PageRoute::Album(id.id().to_string()));
            let g = gtk::GestureClick::new();
            g.connect_released(move |g, n, _, _| {
                if n == 1 {
                    g.set_state(gtk::EventSequenceState::Claimed);
                    nav(dest.clone());
                }
            });
            lbl.add_controller(g);
        }
        container.append(&lbl);
    }
}
