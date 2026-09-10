use super::widgets::*;
use crate::{
    app::{AppState, LibrarySubTab, ListKind, Msg, Overlay, Screen},
    model::{PlaybackStatus, Track},
};
use ratatui::{layout::Rect, style::Style, widgets::Paragraph};
use ratcn::{Theme, runtime::DeclareCtx};

// ─── helpers ────────────────────────────────────────────────────────────────

pub fn track_rows(tracks: &[Track], s: &AppState) -> Vec<ListRow> {
    tracks
        .iter()
        .map(|t| ListRow {
            title: t.title.clone(),
            subtitle: t.artist.clone(),
            detail: t.album.clone(),
            duration: t.formatted_duration(),
            context: Some(Msg::OpenContextMenu(t.id.clone())),
            current: s
                .player
                .current_track()
                .is_some_and(|p| p.playback_id() == t.playback_id()),
        })
        .collect()
}

pub fn render_list(
    ctx: &mut DeclareCtx<'_, AppState, Msg>,
    s: &AppState,
    kind: ListKind,
    items: Vec<ListRow>,
    area: Rect,
) {
    ctx.component(
        "list",
        SongList {
            kind,
            rows: items,
            cursor: s.cursor_for(kind).clone(),
            area,
            frame: if s.player.status == PlaybackStatus::Playing && !s.reduced_motion {
                s.now.as_millis() as u64 / 40
            } else {
                u64::MAX
            },
        },
        area,
    );
}

pub fn empty(ctx: &mut DeclareCtx<'_, AppState, Msg>, t: &Theme, a: Rect, title: &str, body: &str) {
    text(ctx, title, row(a, 1), t.foreground, true);
    ctx.paint_widget(
        Paragraph::new(body.to_string())
            .style(Style::default().fg(t.muted_foreground))
            .wrap(ratatui::widgets::Wrap { trim: true }),
        Rect::new(
            a.x,
            a.y + 3.min(a.height),
            a.width,
            a.height.saturating_sub(3),
        ),
    );
}

// ─── top-level dispatcher ────────────────────────────────────────────────────

pub fn render(ctx: &mut DeclareCtx<'_, AppState, Msg>, s: &AppState, t: &Theme, a: Rect) {
    if a.is_empty() {
        return;
    }
    match &s.active_screen {
        Screen::NowPlaying => now_playing(ctx, s, t, a),
        Screen::ListenNow => listen_now(ctx, s, t, a),
        Screen::Browse => browse(ctx, s, t, a),
        Screen::Radio => radio(ctx, s, t, a),
        Screen::Library => library(ctx, s, t, a),
        Screen::AlbumDetail(_) | Screen::PlaylistDetail(_) | Screen::ArtistDetail(_) => {
            detail(ctx, s, t, a)
        }
    }
}

// ─── screen header ───────────────────────────────────────────────────────────

fn screen_header(
    ctx: &mut DeclareCtx<'_, AppState, Msg>,
    s: &AppState,
    t: &Theme,
    a: Rect,
    title: &str,
    subtitle: &str,
) {
    let title_w = a
        .width
        .saturating_sub(if s.nav_history.is_empty() { 0 } else { 10 });
    text(
        ctx,
        title,
        Rect::new(a.x, a.y, title_w, 1),
        t.foreground,
        true,
    );
    if !s.nav_history.is_empty() {
        button(
            ctx,
            "back",
            "‹ Back",
            Rect::new(a.right() - 10, a.y, 10, 1),
            Msg::NavigateBack,
            false,
        );
    }
    if a.height > 1 && !subtitle.is_empty() {
        text(ctx, subtitle, row(a, 1), t.muted_foreground, false);
    }
}

// ─── Listen Now ──────────────────────────────────────────────────────────────

fn listen_now(ctx: &mut DeclareCtx<'_, AppState, Msg>, s: &AppState, t: &Theme, a: Rect) {
    text(
        ctx,
        "LISTEN NOW",
        Rect::new(a.x, a.y, a.width, 1),
        t.foreground,
        true,
    );

    let mut y = a.y + 2u16;

    // ── TOP PICKS FOR YOU — from personalized recommendations ────────────────
    // Uses personal_mixes; falls back to library albums if not yet loaded.
    let has_mixes = !s.library.personal_mixes.is_empty();
    let has_albums = !s.library.albums.is_empty();

    if (has_mixes || has_albums) && a.height > y - a.y + 10 && a.width >= 52 {
        text(
            ctx,
            "TOP PICKS FOR YOU",
            Rect::new(a.x, y, a.width, 1),
            t.muted_foreground,
            true,
        );
        y += 1;

        let card_h: u16 = 8;
        let max_cards = 3usize;

        if has_mixes {
            // Show personalised mixes as clickable playlist cards
            let count = max_cards.min(s.library.personal_mixes.len());
            let total_gaps = (count as u16).saturating_sub(1) * 2;
            let card_w = ((a.width.saturating_sub(total_gaps)) / count as u16).clamp(18, 28);

            for (i, mix) in s.library.personal_mixes.iter().take(count).enumerate() {
                let cx = a.x + i as u16 * (card_w + 2);
                if cx >= a.right() {
                    break;
                }
                let actual_w = card_w.min(a.right() - cx);
                let card_area = Rect::new(cx, y, actual_w, card_h);

                // Placeholder card with mix name
                ctx.paint_widget(
                    crate::ui::artwork::ArtworkCard {
                        title: mix.name.clone(),
                        artist: mix.subtitle.clone(),
                        album: None,
                        theme: *t,
                        cover: None,
                    },
                    card_area,
                );

                // Open playlist button below card
                if a.height > card_h + y - a.y + 1 {
                    button(
                        ctx,
                        &format!("mix{i}"),
                        "▶  Play",
                        Rect::new(cx, y + card_h + 1, card_w.min(10), 1),
                        Msg::PlayPlaylist(mix.id.clone()),
                        false,
                    );
                }
            }
        } else {
            // Fallback to library albums until recommendations load
            let count = max_cards.min(s.library.albums.len());
            let total_gaps = (count as u16).saturating_sub(1) * 2;
            let card_w = ((a.width.saturating_sub(total_gaps)) / count as u16).clamp(18, 28);

            for (i, album) in s.library.albums.iter().take(count).enumerate() {
                let cx = a.x + i as u16 * (card_w + 2);
                if cx >= a.right() {
                    break;
                }
                let actual_w = card_w.min(a.right() - cx);
                let card_area = Rect::new(cx, y, actual_w, card_h);

                let cover = album
                    .track_ids
                    .first()
                    .and_then(|id| s.find_track(id))
                    .and_then(|tr| tr.artwork_url.as_ref())
                    .and_then(|u| s.artwork.get(u))
                    .cloned();

                ctx.paint_widget(
                    crate::ui::artwork::ArtworkCard {
                        title: album.title.clone(),
                        artist: album.artist.clone(),
                        album: None,
                        theme: *t,
                        cover,
                    },
                    card_area,
                );

                if a.height > card_h + y - a.y + 1 {
                    button(
                        ctx,
                        &format!("album{i}"),
                        "Open album",
                        Rect::new(cx, y + card_h + 1, card_w.min(12), 1),
                        Msg::OpenAlbum(album.id.clone()),
                        false,
                    );
                }
            }
        }

        y += card_h + 2;
    }

    // ── MADE FOR YOU — personalised playlists ────────────────────────────────
    if !s.library.personal_mixes.is_empty() && a.height > y - a.y + 3 {
        y += 1;
        text(
            ctx,
            "MADE FOR YOU",
            Rect::new(a.x, y, a.width, 1),
            t.muted_foreground,
            true,
        );
        y += 1;

        // Render as a horizontal button row of playlist names
        let mut px = a.x;
        for (i, mix) in s.library.personal_mixes.iter().enumerate().skip(3).take(5) {
            let label = &mix.name;
            let w = (label.chars().count() as u16 + 4).min(a.right().saturating_sub(px));
            if w == 0 || px >= a.right() {
                break;
            }
            button(
                ctx,
                &format!("madeforyou{i}"),
                label,
                Rect::new(px, y, w, 1),
                Msg::PlayPlaylist(mix.id.clone()),
                false,
            );
            px += w + 2;
        }
        y += 2;
    }

    // ── RECENTLY PLAYED — track list ─────────────────────────────────────────
    if a.height > y - a.y + 2 {
        let has_recent = !s.library.recently_played.is_empty();
        let heading = if has_recent || s.library.tracks.is_empty() {
            "RECENTLY PLAYED"
        } else {
            "LIBRARY SONGS"
        };
        text(
            ctx,
            heading,
            Rect::new(a.x, y, a.width, 1),
            t.muted_foreground,
            true,
        );
        y += 1;

        let recent_area = Rect::new(a.x, y, a.width, a.height.saturating_sub(y - a.y));
        if s.library.tracks.is_empty() && !has_recent {
            if !s.is_authorized && !s.demo {
                empty(
                    ctx,
                    t,
                    recent_area,
                    "Make yourself at home",
                    "Run malus --login to sign in, then reopen Malus.",
                );
            } else if s.connecting || s.library_loading {
                empty(
                    ctx,
                    t,
                    recent_area,
                    "Loading your music",
                    "Fetching songs from Apple Music…",
                );
            } else {
                empty(
                    ctx,
                    t,
                    recent_area,
                    "Nothing here yet",
                    "Search Apple Music or browse the catalog.",
                );
            }
            if recent_area.height > 5 {
                button(
                    ctx,
                    "empty_search",
                    "Search music",
                    Rect::new(recent_area.x, recent_area.y + 5, 16, 1),
                    Msg::OpenOverlay(Overlay::Search),
                    true,
                );
            }
        } else {
            let items = if has_recent {
                track_rows(&s.library.recently_played, s)
            } else {
                track_rows(&s.screen_tracks(), s)
            };
            render_list(ctx, s, ListKind::Main, items, recent_area);
        }
    }
}

// ─── Browse & Charts ─────────────────────────────────────────────────────────

fn browse(ctx: &mut DeclareCtx<'_, AppState, Msg>, s: &AppState, t: &Theme, a: Rect) {
    screen_header(
        ctx,
        s,
        t,
        a,
        "BROWSE & CHARTS",
        "Top songs on Apple Music in your storefront",
    );

    let list_area = Rect::new(a.x, a.y + 3, a.width, a.height.saturating_sub(3));
    let items = track_rows(&s.screen_tracks(), s);

    if items.is_empty() {
        let (heading, body) = if s.catalog_loading {
            ("Loading charts", "Fetching top songs from Apple Music…")
        } else if let Some(e) = &s.engine_error {
            ("Apple Music is unavailable", e.as_str())
        } else if !s.is_authorized && !s.demo {
            ("Not signed in", "Run malus --login to access the catalog.")
        } else {
            (
                "Nothing here yet",
                "Search Apple Music or try again shortly.",
            )
        };
        empty(ctx, t, list_area, heading, body);
        if list_area.height > 5 {
            button(
                ctx,
                "search",
                "Search music",
                Rect::new(list_area.x, list_area.y + 5, 14, 1),
                Msg::OpenOverlay(Overlay::Search),
                true,
            );
            button(
                ctx,
                "refresh",
                "Refresh",
                Rect::new(list_area.x + 16, list_area.y + 5, 10, 1),
                Msg::Refresh,
                false,
            );
        }
    } else {
        render_list(ctx, s, ListKind::Main, items, list_area);
    }
}

// ─── Radio ───────────────────────────────────────────────────────────────────

fn radio(ctx: &mut DeclareCtx<'_, AppState, Msg>, s: &AppState, t: &Theme, a: Rect) {
    screen_header(
        ctx,
        s,
        t,
        a,
        "RADIO",
        "Live broadcasts and stations from Apple Music",
    );

    let list_area = Rect::new(a.x, a.y + 3, a.width, a.height.saturating_sub(3));
    let items: Vec<ListRow> = s
        .library
        .radio_stations
        .iter()
        .map(|v| ListRow {
            title: v.name.clone(),
            subtitle: if v.is_live {
                "Live broadcast".into()
            } else {
                "Station".into()
            },
            detail: String::new(),
            duration: String::new(),
            context: None,
            current: false,
        })
        .collect();

    if items.is_empty() {
        empty(
            ctx,
            t,
            list_area,
            "No stations found",
            "Stations will appear here once connected.",
        );
    } else {
        render_list(ctx, s, ListKind::Main, items, list_area);
    }
}

// ─── Library ─────────────────────────────────────────────────────────────────

fn library(ctx: &mut DeclareCtx<'_, AppState, Msg>, s: &AppState, t: &Theme, a: Rect) {
    let subtitle = format!(
        "{} songs  ·  {} albums  ·  {} artists",
        s.library.tracks.len(),
        s.library.albums.len(),
        s.library.artists.len()
    );
    screen_header(ctx, s, t, a, "MY LIBRARY", &subtitle);

    // Sub-tab bar
    if a.height > 3 {
        let mut x = a.x;
        // glyphs: 󰎵 songs  󰗌 albums  󱑽 artists  󰽐 playlists
        let tab_glyphs = ["󰎵", "󰗌", "󱑽", "󰽐"];
        for (tab, (i, glyph)) in LibrarySubTab::ALL
            .into_iter()
            .zip(tab_glyphs.iter().enumerate())
        {
            let label = format!("{}  {}", glyph, tab.label());
            let w = (label.chars().count() as u16 + 3).min(a.right().saturating_sub(x));
            if w == 0 {
                break;
            }
            button(
                ctx,
                &format!("tab{i}"),
                &label,
                Rect::new(x, a.y + 3, w, 1),
                Msg::SelectLibrarySubTab(tab),
                s.library_subtab == tab,
            );
            x += w;
        }

        // Persistent sort controls for Songs tab
        if s.library_subtab == LibrarySubTab::Songs && a.width >= 58 {
            let sort_field_label = format!("Sort: {}", s.library_sort.field.label());
            let sort_w = sort_field_label.chars().count() as u16 + 3;
            let order_w = 4u16;
            let sort_x = a.right().saturating_sub(sort_w + order_w + 1);
            if sort_x > x + 2 {
                button(
                    ctx,
                    "sort_field",
                    &sort_field_label,
                    Rect::new(sort_x, a.y + 3, sort_w, 1),
                    Msg::CycleLibrarySortField,
                    false,
                );
                button(
                    ctx,
                    "sort_order",
                    s.library_sort.order.symbol(),
                    Rect::new(sort_x + sort_w + 1, a.y + 3, order_w, 1),
                    Msg::ToggleLibrarySortOrder,
                    false,
                );
            }
        }
    }

    let list_area = Rect::new(a.x, a.y + 5, a.width, a.height.saturating_sub(5));
    let items: Vec<ListRow> = match s.library_subtab {
        LibrarySubTab::Songs => track_rows(&s.screen_tracks(), s),
        LibrarySubTab::Albums => s
            .library
            .albums
            .iter()
            .map(|v| ListRow {
                title: v.title.clone(),
                subtitle: v.artist.clone(),
                detail: format!("{} songs", v.track_ids.len()),
                duration: String::new(),
                context: None,
                current: false,
            })
            .collect(),
        LibrarySubTab::Artists => s
            .library
            .artists
            .iter()
            .map(|v| ListRow {
                title: v.name.clone(),
                subtitle: format!("{} songs", v.top_track_ids.len()),
                detail: String::new(),
                duration: String::new(),
                context: None,
                current: false,
            })
            .collect(),
        LibrarySubTab::Playlists => s
            .library
            .playlists
            .iter()
            .map(|v| ListRow {
                title: v.name.clone(),
                subtitle: v.description.clone(),
                detail: String::new(),
                duration: String::new(),
                context: None,
                current: false,
            })
            .collect(),
    };

    if items.is_empty() {
        let (heading, body) = if s.connecting {
            (
                "Connecting to Apple Music",
                "Your library will appear here when the player is ready.",
            )
        } else if s.library_loading {
            ("Loading your music", "Fetching songs from Apple Music…")
        } else if let Some(e) = &s.engine_error {
            ("Apple Music is unavailable", e.as_str())
        } else if !s.is_authorized && !s.demo {
            (
                "Make yourself at home",
                "Run malus --login to sign in, then reopen Malus.",
            )
        } else {
            (
                "Nothing here yet",
                "Search Apple Music, or refresh your library.",
            )
        };
        empty(ctx, t, list_area, heading, body);
        if list_area.height > 5 {
            button(
                ctx,
                "empty_search",
                "Search music",
                Rect::new(list_area.x, list_area.y + 5, 16, 1),
                Msg::OpenOverlay(Overlay::Search),
                true,
            );
            button(
                ctx,
                "refresh",
                "Refresh",
                Rect::new(list_area.x + 18, list_area.y + 5, 11, 1),
                Msg::Refresh,
                false,
            );
        }
    } else {
        render_list(ctx, s, ListKind::Main, items, list_area);
    }
}

// ─── Detail (Album / Artist / Playlist) ──────────────────────────────────────

fn detail(ctx: &mut DeclareCtx<'_, AppState, Msg>, s: &AppState, t: &Theme, a: Rect) {
    let (title, subtitle) = match &s.active_screen {
        Screen::AlbumDetail(id) => s
            .library
            .find_album(id)
            .map(|v| {
                (
                    v.title.clone(),
                    format!("{}  ·  {} songs", v.artist, v.track_ids.len()),
                )
            })
            .unwrap_or(("Album unavailable".into(), String::new())),
        Screen::ArtistDetail(id) => s
            .library
            .find_artist(id)
            .map(|v| {
                (
                    v.name.clone(),
                    format!("{} songs in your library", v.top_track_ids.len()),
                )
            })
            .unwrap_or(("Artist unavailable".into(), String::new())),
        Screen::PlaylistDetail(id) => s
            .library
            .find_playlist(id)
            .map(|v| (v.name.clone(), format!("{} songs", v.track_ids.len())))
            .unwrap_or(("Playlist unavailable".into(), String::new())),
        _ => (String::new(), String::new()),
    };
    screen_header(ctx, s, t, a, &title, &subtitle);

    if a.height > 3 {
        let action = match &s.active_screen {
            Screen::AlbumDetail(id) => Msg::PlayAlbum(id.clone()),
            Screen::PlaylistDetail(id) => Msg::PlayPlaylist(id.clone()),
            _ => Msg::Noop,
        };
        button(
            ctx,
            "play_all",
            "▶  Play all",
            Rect::new(a.x, a.y + 3, 13, 1),
            action,
            true,
        );
    }

    let list_area = Rect::new(a.x, a.y + 5, a.width, a.height.saturating_sub(5));
    let items = track_rows(&s.screen_tracks(), s);
    if items.is_empty() {
        empty(
            ctx,
            t,
            list_area,
            "Nothing here yet",
            "This collection appears to be empty.",
        );
    } else {
        render_list(ctx, s, ListKind::Main, items, list_area);
    }
}

// ─── Now Playing ─────────────────────────────────────────────────────────────
//
// Wide (≥ 78 cols): left pane = artwork + metadata below it.
//                   right pane = LYRICS directly rendered (no overlay needed).
// Narrow (<78 cols): artwork centred, metadata below.

fn now_playing(ctx: &mut DeclareCtx<'_, AppState, Msg>, s: &AppState, t: &Theme, a: Rect) {
    let Some(track) = s.player.current_track() else {
        empty(
            ctx,
            t,
            a,
            "A little room for your music.",
            "Choose a song from your library or search Apple Music.",
        );
        return;
    };

    let cover = track
        .artwork_url
        .as_ref()
        .and_then(|u| s.artwork.get(u))
        .cloned();

    if a.width >= 78 {
        // ── Wide layout ───────────────────────────────────────────────────────
        let left_w: u16 = (a.width * 38 / 100)
            .clamp(34, 44)
            .min(a.width.saturating_sub(20));
        let left = Rect::new(a.x, a.y, left_w, a.height);
        let right = Rect::new(
            a.x + left_w + 2,
            a.y,
            a.width.saturating_sub(left_w + 3),
            a.height,
        );

        // Artwork — fills left width, height capped to leave room for metadata
        let art_h = left_w.min(a.height.saturating_sub(4));
        let art_rect = Rect::new(left.x, left.y, left_w, art_h);
        ctx.paint_widget(
            crate::ui::artwork::ArtworkCard {
                title: track.title.clone(),
                artist: track.artist.clone(),
                album: Some(track.album.clone()),
                theme: *t,
                cover,
            },
            art_rect,
        );

        // Metadata below artwork — title, artist, album · AAC 256
        let meta_y = art_rect.bottom() + 1;
        if meta_y < a.bottom() {
            text(
                ctx,
                track.title.clone(),
                Rect::new(left.x, meta_y, left_w, 1),
                t.foreground,
                true,
            );
        }
        if meta_y + 1 < a.bottom() {
            text(
                ctx,
                track.artist.clone(),
                Rect::new(left.x, meta_y + 1, left_w, 1),
                t.foreground,
                false,
            );
        }
        if meta_y + 2 < a.bottom() {
            text(
                ctx,
                format!("{}  ·  AAC 256", track.album),
                Rect::new(left.x, meta_y + 2, left_w, 1),
                t.muted_foreground,
                false,
            );
        }

        // Right pane — render lyrics directly; no overlay needed here
        if !right.is_empty() {
            if s.lyrics_loading {
                text(
                    ctx,
                    "Loading lyrics…",
                    row(right, 0),
                    t.muted_foreground,
                    false,
                );
            } else if !s.player.lyrics.is_empty() {
                // Render live-scrolling lyrics directly in the right pane
                let lyrics_area = Rect::new(
                    right.x,
                    right.y,
                    right.width.saturating_sub(1), // leave 1 col for scrollbar
                    right.height.saturating_sub(1),
                );
                ctx.component(
                    "np_lyrics",
                    LyricsList {
                        lyrics: s.player.lyrics.clone(),
                        current_time: s.position_secs(),
                        scroll_offset: s.lyrics_scroll_offset,
                        area: lyrics_area,
                    },
                    lyrics_area,
                );
                let hint = if s.lyrics_scroll_offset.is_some() {
                    "[c] Re-center (resumes shortly)  ·  Click line to seek"
                } else {
                    "[c] Re-center  ·  Click line to seek"
                };
                text(
                    ctx,
                    hint,
                    row(right, right.height.saturating_sub(1)),
                    t.muted_foreground,
                    false,
                );
            } else {
                // No lyrics for this track — show a single clean line
                text(
                    ctx,
                    "Lyrics unavailable for this track.",
                    row(right, 0),
                    t.muted_foreground,
                    false,
                );
            }
        }
    } else {
        // ── Narrow layout ─────────────────────────────────────────────────────
        let art_h = a.height.saturating_sub(5).min(14);
        let art_w = (art_h * 2).min(a.width);
        let art_x = a.x + (a.width.saturating_sub(art_w)) / 2;
        let art_rect = Rect::new(art_x, a.y, art_w, art_h);

        ctx.paint_widget(
            crate::ui::artwork::ArtworkCard {
                title: track.title.clone(),
                artist: track.artist.clone(),
                album: Some(track.album.clone()),
                theme: *t,
                cover,
            },
            art_rect,
        );

        let meta_start = art_rect.bottom() + 1;
        let meta_dy = meta_start.saturating_sub(a.y);
        text(
            ctx,
            track.title.clone(),
            row(a, meta_dy),
            t.foreground,
            true,
        );
        text(
            ctx,
            track.artist.clone(),
            row(a, meta_dy + 1),
            t.foreground,
            false,
        );
        text(
            ctx,
            track.album.clone(),
            row(a, meta_dy + 2),
            t.muted_foreground,
            false,
        );

        if !s.player.lyrics.is_empty() && a.height > meta_dy + 4 {
            let lyrics_area = Rect::new(
                a.x,
                a.y + meta_dy + 4,
                a.width,
                a.height.saturating_sub(meta_dy + 4),
            );
            ctx.component(
                "np_lyrics_narrow",
                LyricsList {
                    lyrics: s.player.lyrics.clone(),
                    current_time: s.position_secs(),
                    scroll_offset: s.lyrics_scroll_offset,
                    area: lyrics_area,
                },
                lyrics_area,
            );
        } else if s.player.lyrics.is_empty() && a.height > meta_dy + 4 {
            button(
                ctx,
                "lyrics_narrow",
                "Lyrics",
                Rect::new(a.x, a.y + meta_dy + 4, 10, 1),
                Msg::OpenOverlay(Overlay::Lyrics),
                false,
            );
        }
    }
}
