use super::widgets::*;
use crate::{
    app::{AppState, LibrarySubTab, ListKind, Msg, Overlay, Screen},
    model::{PlaybackStatus, Track},
};
use ratatui::{layout::Rect, style::Style, widgets::Paragraph};
use ratcn::{Theme, runtime::DeclareCtx};

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
pub fn render(ctx: &mut DeclareCtx<'_, AppState, Msg>, s: &AppState, t: &Theme, a: Rect) {
    if a.is_empty() {
        return;
    }
    if s.active_screen == Screen::NowPlaying {
        now_playing(ctx, s, t, a);
        return;
    }
    let (title, subtitle) = match &s.active_screen {
        Screen::ListenNow => (
            "Listen Now".into(),
            if s.demo {
                "Demo library · playback is simulated".into()
            } else {
                format!(
                    "Your music, close at hand.  {} songs in your library.",
                    s.library.tracks.len()
                )
            },
        ),
        Screen::Library => (
            "Your library".into(),
            format!(
                "{} songs  ·  {} albums  ·  {} artists",
                s.library.tracks.len(),
                s.library.albums.len(),
                s.library.artists.len()
            ),
        ),
        Screen::Browse => (
            "Browse".into(),
            "Top songs on Apple Music in your storefront".into(),
        ),
        Screen::Radio => (
            "Radio".into(),
            "Live broadcasts and stations from Apple Music".into(),
        ),
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
    text(
        ctx,
        title,
        Rect::new(a.x, a.y, a.width.saturating_sub(12), 1),
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
    if a.height > 1 {
        text(ctx, subtitle, row(a, 1), t.muted_foreground, false);
    }
    let mut offset = 3;
    if s.active_screen == Screen::Library {
        let mut x = a.x;
        for (tab, i) in LibrarySubTab::ALL.into_iter().zip(0..) {
            let w = (tab.label().len() as u16 + 3).min(a.right().saturating_sub(x));
            button(
                ctx,
                &format!("tab{i}"),
                tab.label(),
                Rect::new(x, a.y + 3, w, u16::from(a.height > 3)),
                Msg::SelectLibrarySubTab(tab),
                s.library_subtab == tab,
            );
            x += w;
        }
        offset = 5;
    } else if matches!(
        s.active_screen,
        Screen::AlbumDetail(_) | Screen::PlaylistDetail(_)
    ) {
        let action = match &s.active_screen {
            Screen::AlbumDetail(id) => Msg::PlayAlbum(id.clone()),
            Screen::PlaylistDetail(id) => Msg::PlayPlaylist(id.clone()),
            _ => Msg::Noop,
        };
        button(
            ctx,
            "play_all",
            "Play all",
            Rect::new(a.x, a.y + 3, 11, u16::from(a.height > 3)),
            action,
            true,
        );
        offset = 5;
    }
    if s.active_screen == Screen::ListenNow
        && a.height >= 18
        && a.width >= 76
        && !s.library.albums.is_empty()
    {
        let count = 3.min(s.library.albums.len());
        let card_width = a.width / count as u16;
        for (i, album) in s.library.albums.iter().take(count).enumerate() {
            let x = a.x + i as u16 * card_width;
            let image_area = Rect::new(x, a.y + 4, 12, 6);
            let track = album.track_ids.first().and_then(|id| s.find_track(id));
            let cover = track
                .and_then(|t| t.artwork_url.as_ref())
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
                image_area,
            );
            let info = Rect::new(x + 14, a.y + 5, card_width.saturating_sub(16), 1);
            text(ctx, album.title.clone(), info, t.foreground, true);
            text(
                ctx,
                album.artist.clone(),
                Rect::new(info.x, info.y + 1, info.width, 1),
                t.muted_foreground,
                false,
            );
            button(
                ctx,
                &format!("album{i}"),
                "Open album",
                Rect::new(info.x, info.y + 3, info.width.min(14), 1),
                Msg::OpenAlbum(album.id.clone()),
                false,
            );
        }
        text(
            ctx,
            "FROM YOUR LIBRARY",
            row(a, 12),
            t.muted_foreground,
            true,
        );
        offset = 14;
    }
    let list_area = Rect::new(
        a.x,
        a.y + offset.min(a.height),
        a.width,
        a.height.saturating_sub(offset),
    );
    let items = match (&s.active_screen, s.library_subtab) {
        (Screen::Library, LibrarySubTab::Albums) => s
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
        (Screen::Library, LibrarySubTab::Artists) => s
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
        (Screen::Library, LibrarySubTab::Playlists) => s
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
        (Screen::Radio, _) => s
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
            .collect(),
        _ => track_rows(&s.screen_tracks(), s),
    };
    if Vec::<ListRow>::is_empty(&items) {
        let (heading, body) = if s.connecting {
            (
                "Connecting to Apple Music",
                "Your library will appear here when the player is ready.",
            )
        } else if s.library_loading || s.catalog_loading {
            ("Loading your music", "Fetching songs from Apple Music…")
        } else if let Some(error) = &s.engine_error {
            ("Apple Music is unavailable", error.as_str())
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
    let large = a.width >= 78;
    let art_h = if large {
        a.height.min(16)
    } else {
        a.height.saturating_sub(7).min(10)
    };
    let art_w = (art_h * 2).min(a.width);
    let art = Rect::new(
        if large {
            a.x + 2
        } else {
            a.x + (a.width - art_w) / 2
        },
        a.y,
        art_w,
        art_h,
    );
    let cover = track
        .artwork_url
        .as_ref()
        .and_then(|u| s.artwork.get(u))
        .cloned();
    ctx.paint_widget(
        crate::ui::artwork::ArtworkCard {
            title: track.title.clone(),
            artist: track.artist.clone(),
            album: Some(track.album.clone()),
            theme: *t,
            cover,
        },
        art,
    );
    let info = if large {
        Rect::new(
            art.right() + 5,
            a.y + 3,
            a.right().saturating_sub(art.right() + 5),
            a.height.saturating_sub(3),
        )
    } else {
        Rect::new(
            a.x,
            art.bottom() + 1,
            a.width,
            a.height.saturating_sub(art_h + 1),
        )
    };
    text(
        ctx,
        if s.player.status == PlaybackStatus::Playing {
            "NOW PLAYING"
        } else {
            "PAUSED"
        },
        row(info, 0),
        t.primary,
        true,
    );
    text(ctx, track.title.clone(), row(info, 2), t.foreground, true);
    text(ctx, track.artist.clone(), row(info, 3), t.foreground, false);
    text(
        ctx,
        track.album.clone(),
        row(info, 4),
        t.muted_foreground,
        false,
    );
    if info.height > 7 {
        button(
            ctx,
            "next_up",
            "Up next",
            Rect::new(info.x, info.y + 7, 12, 1),
            Msg::OpenOverlay(Overlay::Queue),
            true,
        );
    }
}
