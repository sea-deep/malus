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
    panes::lyrics::LyricsView,
    services::{DecodedImage, artwork_texture, generate_backdrop_texture},
    state::{NowPlayingMode, SharedPlayer},
    widgets::{player_controls::*, square_artwork::SquareArtwork},
};
use relm4::gtk::{self, prelude::*};
use std::{cell::Cell, rc::Rc};

pub struct NowPlayingPage {
    pub root: gtk::Overlay,
    mode: Rc<Cell<NowPlayingMode>>,
    // Wide layout widgets
    artwork: SquareArtwork,
    title: gtk::Label,
    subtitle: gtk::Label,
    favorite: gtk::Button,
    more: gtk::MenuButton,
    transport: Transport,
    seek: SeekControl,
    volume: VolumeControl,
    pub lyrics: LyricsView,
    // Compact layout widgets (separate instances)
    compact_artwork: SquareArtwork,
    compact_player_artwork: SquareArtwork,
    compact_lyrics_button: gtk::Button,
    compact_title: gtk::Label,
    compact_subtitle: gtk::Label,
    compact_favorite: gtk::Button,
    compact_more: gtk::MenuButton,
    compact_transport: Transport,
    compact_seek: SeekControl,
    compact_volume: VolumeControl,
    compact_lyrics: LyricsView,
    // Shared
    backdrops: [gtk::Picture; 2],
    backdrop_stack: gtk::Stack,
    backdrop_index: Cell<usize>,
    lyrics_button: gtk::Button,
}
impl NowPlayingPage {
    pub fn new(
        player: &SharedPlayer,
        send: &CommandHandler,
        menu: &MenuHandler,
        close: impl Fn() + 'static,
        toggle_lyrics: impl Fn() + 'static,
        open_queue: impl Fn() + 'static,
    ) -> Self {
        let toggle_lyrics = Rc::new(toggle_lyrics);
        let open_queue = Rc::new(open_queue);
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
        wide_scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
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
        let subtitle = gtk::Label::new(None);
        subtitle.add_css_class("nowplaying-artist");
        for label in [&title, &subtitle] {
            label.set_xalign(0.0);
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            label.set_max_width_chars(1);
            label.set_hexpand(true);
        }
        title.set_margin_bottom(4);
        column.append(&title);
        column.append(&subtitle);

        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        actions.set_margin_top(6);
        actions.set_margin_bottom(6);
        let favorite = favorite_button(player, menu);
        favorite.add_css_class("np-control");
        let more = more_button(player, menu);
        actions.append(&favorite);
        actions.append(&more);
        column.append(&actions);

        let seek = SeekControl::new(player, send, true);
        column.append(&seek.root);

        let transport = Transport::new(player, send, true);
        transport.root.set_margin_top(6);
        transport.root.set_margin_bottom(4);
        column.append(&transport.root);

        let volume = VolumeControl::new(send, 180);
        volume.root.set_halign(gtk::Align::Center);
        column.append(&volume.root);

        composition.append(&column);

        let lyrics = LyricsView::new(player, send, true);
        composition.append(&lyrics.root);
        lyrics.root.set_visible(false);

        wide_scroll.set_child(Some(&composition));
        root.add_overlay(&wide_scroll);

        // === COMPACT LAYOUT ===
        let compact_root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        compact_root.set_hexpand(true);
        compact_root.set_vexpand(true);
        compact_root.set_visible(false);

        // Compact header: small artwork + metadata + fav/more
        let compact_header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        compact_header.set_margin_start(20);
        compact_header.set_margin_end(20);
        compact_header.set_margin_top(48);
        compact_header.set_margin_bottom(8);

        let compact_artwork = SquareArtwork::new(64, "nowplaying-artwork");
        compact_artwork.set_hexpand(false);
        compact_artwork.set_vexpand(false);
        compact_header.append(&compact_artwork);

        let compact_meta = gtk::Box::new(gtk::Orientation::Vertical, 2);
        compact_meta.set_valign(gtk::Align::Center);
        compact_meta.set_hexpand(true);

        let compact_title = gtk::Label::new(Some("Not Playing"));
        compact_title.add_css_class("nowplaying-title");
        compact_title.set_xalign(0.0);
        compact_title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        compact_title.set_max_width_chars(1);
        compact_title.set_hexpand(true);

        let compact_subtitle = gtk::Label::new(None);
        compact_subtitle.add_css_class("nowplaying-artist");
        compact_subtitle.set_xalign(0.0);
        compact_subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);
        compact_subtitle.set_max_width_chars(1);
        compact_subtitle.set_hexpand(true);

        compact_meta.append(&compact_title);
        compact_meta.append(&compact_subtitle);
        compact_header.append(&compact_meta);

        let compact_favorite = favorite_button(player, menu);
        compact_favorite.add_css_class("np-control");
        let compact_more = more_button(player, menu);
        compact_header.append(&compact_favorite);
        compact_header.append(&compact_more);

        compact_root.append(&compact_header);

        let compact_player_artwork = SquareArtwork::new(280, "nowplaying-artwork");
        let compact_player = gtk::Box::new(gtk::Orientation::Vertical, 12);
        compact_player.set_valign(gtk::Align::Center);
        compact_player.set_halign(gtk::Align::Center);
        compact_player.set_vexpand(true);
        compact_player.append(&compact_player_artwork);
        compact_root.append(&compact_player);

        // Compact lyrics: its own LyricsView instance, vexpands to fill
        let compact_lyrics = LyricsView::new(player, send, true);
        compact_lyrics.root.set_vexpand(true);
        compact_lyrics.root.set_hexpand(true);
        compact_root.append(&compact_lyrics.root);

        // Compact seek bar
        let compact_seek = SeekControl::new(player, send, true);
        compact_seek.root.set_margin_start(20);
        compact_seek.root.set_margin_end(20);
        compact_root.append(&compact_seek.root);

        // Compact transport: prev / play / next only (no shuffle/repeat)
        // Pass immersive=false to hide shuffle/repeat, but style as NP
        let compact_transport = Transport::new(player, send, false);
        compact_transport.root.add_css_class("compact-transport");
        compact_transport.root.set_margin_top(4);
        compact_transport.root.set_margin_bottom(4);
        compact_transport.root.set_spacing(28);
        compact_root.append(&compact_transport.root);

        // Compact volume: wide with speaker icons on both sides
        let compact_volume = VolumeControl::new_wide(send);
        compact_volume.root.set_margin_start(20);
        compact_volume.root.set_margin_end(20);
        compact_volume.root.set_margin_bottom(8);
        compact_root.append(&compact_volume.root);

        // Bottom row: Lyrics / Queue toggle buttons
        let compact_bottom = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        compact_bottom.set_halign(gtk::Align::Center);
        compact_bottom.set_margin_bottom(12);
        compact_bottom.set_spacing(80);
        let compact_lyrics_button = icon_button(ICON_LYRICS, "Lyrics", "np-control");
        let compact_queue_button = icon_button(ICON_QUEUE, "Queue", "np-control");
        compact_bottom.append(&compact_lyrics_button);
        compact_bottom.append(&compact_queue_button);
        let toggle = toggle_lyrics.clone();
        compact_lyrics_button.connect_clicked(move |_| toggle());
        let queue = open_queue.clone();
        compact_queue_button.connect_clicked(move |_| queue());
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

        // Wide: lyrics toggle right
        let lyrics_button = icon_button(ICON_LYRICS, "Toggle Now Playing Lyrics", "np-control");
        let toggle_rc = toggle_lyrics;
        let t1 = toggle_rc.clone();
        lyrics_button.connect_clicked(move |_| t1());
        header.append(&lyrics_button);

        let queue_button = icon_button(ICON_QUEUE, "Playing Next", "np-control");
        queue_button.connect_clicked(move |_| open_queue());
        header.append(&queue_button);
        let handle = gtk::WindowHandle::new();
        handle.set_valign(gtk::Align::Start);
        handle.set_hexpand(true);
        handle.set_child(Some(&header));
        root.add_overlay(&handle);

        // === RESPONSIVE TICK ===
        let mode = Rc::new(Cell::new(NowPlayingMode::Player));
        let m = mode.clone();
        let art = artwork.clone();
        let l = lyrics.root.clone();
        let col_ref = column.clone();
        let comp_ref = composition.clone();
        let ws_ref = wide_scroll.clone();
        let cr_ref = compact_root.clone();
        let cx_ref = close_x.clone();
        let cv_ref = close_chevron.clone();
        let sr_ref = spacer_right.clone();
        let lb_ref = lyrics_button.clone();
        let compact_lyrics_root = compact_lyrics.root.clone();
        let compact_art = compact_player_artwork.clone();
        let compact_small_art = compact_artwork.clone();
        let compact_seek_ref = compact_seek.root.clone();
        let compact_trans_ref = compact_transport.root.clone();
        let compact_vol_ref = compact_volume.root.clone();
        let compact_bottom_ref = compact_bottom.clone();
        let compact_hdr_ref = compact_header.clone();
        let state = player.clone();
        let last = Cell::new((0, 0, NowPlayingMode::Player, false));

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
                let split = show_lyrics && is_wide;

                // Toggle composition visibility
                ws_ref.set_visible(is_wide);
                cr_ref.set_visible(!is_wide);

                // Header visibility
                cx_ref.set_visible(is_wide);
                cv_ref.set_visible(!is_wide);
                // spacer_left always visible: pushes lyrics_button right in
                // wide, centers chevron in compact
                sr_ref.set_visible(!is_wide);
                lb_ref.set_visible(is_wide);
                queue_button.set_visible(is_wide);

                if is_wide {
                    root.remove_css_class("np-narrow");
                    comp_ref.set_spacing((width / 16).clamp(40, 96));
                    comp_ref.set_margin_start(32);
                    comp_ref.set_margin_end(32);

                    // Reserve the measured controls and scroll margins before
                    // sizing artwork; fixed estimates clipped volume on short windows.
                    let controls_height = col_ref.measure(gtk::Orientation::Vertical, -1).0
                        - art.measure(gtk::Orientation::Vertical, -1).0
                        + art.margin_top()
                        + art.margin_bottom();
                    let available_art = (height - 80 - controls_height).max(96);
                    let side = if split {
                        ((width - 176) / 2).min(available_art).clamp(96, 500)
                    } else {
                        available_art.min(width - 48).clamp(96, 560)
                    };
                    art.set_side(side);
                    col_ref.set_width_request(side);

                    l.set_visible(show_lyrics);
                    l.set_size_request(
                        if split {
                            (width - side - 176).clamp(320, 560)
                        } else {
                            side
                        },
                        if split { (height - 112).max(400) } else { 460 },
                    );
                    l.set_valign(gtk::Align::Center);
                } else {
                    root.add_css_class("np-narrow");
                    let lyric_layout = show_lyrics;
                    compact_lyrics_root.set_visible(lyric_layout);
                    compact_player.set_visible(!lyric_layout);
                    compact_small_art.set_visible(lyric_layout);
                    let controls_h = compact_seek_ref.measure(gtk::Orientation::Vertical, -1).0
                        + compact_trans_ref.measure(gtk::Orientation::Vertical, -1).0
                        + compact_vol_ref.measure(gtk::Orientation::Vertical, -1).0
                        + compact_bottom_ref.measure(gtk::Orientation::Vertical, -1).0
                        + compact_hdr_ref.measure(gtk::Orientation::Vertical, -1).0;
                    let available_compact_art = (height - controls_h - 48).max(96);
                    compact_art.set_side((width - 48).min(available_compact_art).clamp(96, 340));
                }
            }
            gtk::glib::ControlFlow::Continue
        });

        Self {
            root,
            mode,
            artwork,
            title,
            subtitle,
            favorite,
            more,
            transport,
            seek,
            volume,
            lyrics,
            compact_artwork,
            compact_player_artwork,
            compact_lyrics_button,
            compact_title,
            compact_subtitle,
            compact_favorite,
            compact_more,
            compact_transport,
            compact_seek,
            compact_volume,
            compact_lyrics,
            backdrops,
            backdrop_stack,
            backdrop_index: Cell::new(0),
            lyrics_button,
        }
    }
    pub fn set_mode(&self, mode: NowPlayingMode) {
        self.mode.set(mode);
        if mode == NowPlayingMode::Lyrics {
            self.lyrics_button.add_css_class("control-active");
            self.compact_lyrics_button.add_css_class("control-active");
            self.compact_lyrics.reveal_current();
            self.lyrics.reveal_current();
        } else {
            self.lyrics_button.remove_css_class("control-active");
            self.compact_lyrics_button
                .remove_css_class("control-active");
        }
    }
    pub fn refresh(&self, player: &SharedPlayer) {
        let state = player.borrow();
        let title_text = state
            .now
            .current_track
            .as_ref()
            .map(|t| t.title.as_str())
            .unwrap_or("Not Playing");
        let subtitle_text = state
            .now
            .current_track
            .as_ref()
            .map(|t| {
                let artist = t.artist_display();
                match t.album_title() {
                    Some(album) if album != t.title && !album.is_empty() => {
                        format!("{artist} · {album}")
                    }
                    _ => artist,
                }
            })
            .unwrap_or_else(|| "Choose a song from your library".to_string());

        self.title.set_text(title_text);
        self.subtitle.set_text(&subtitle_text);
        self.compact_title.set_text(title_text);
        self.compact_subtitle.set_text(&subtitle_text);

        let has_track = state.now.current_track.is_some();
        self.more.set_sensitive(has_track);
        self.compact_more.set_sensitive(has_track);
        drop(state);

        self.transport.refresh(player);
        self.compact_transport.refresh(player);
        refresh_favorite(&self.favorite, player);
        refresh_favorite(&self.compact_favorite, player);
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
