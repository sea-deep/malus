use super::{
    screens::{empty, render_list, track_rows},
    widgets::*,
};
use crate::app::{AppState, ListKind, Msg, Overlay};
use ratatui::{
    layout::Rect,
    style::Style,
    widgets::{Block, Paragraph, Wrap},
};
use ratcn::{Theme, runtime::DeclareCtx};

pub fn bounds(area: Rect, overlay: &Overlay) -> Rect {
    let width = if matches!(overlay, Overlay::Queue) {
        64
    } else {
        84
    }
    .min(area.width.saturating_sub(4));
    let height = if matches!(
        overlay,
        Overlay::Settings | Overlay::ContextMenu(_) | Overlay::Lyrics
    ) {
        14
    } else {
        25
    }
    .min(area.height.saturating_sub(2));
    Rect::new(
        if matches!(overlay, Overlay::Queue) {
            area.right().saturating_sub(width + 1)
        } else {
            area.x + (area.width - width) / 2
        },
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}
pub fn render(ctx: &mut DeclareCtx<'_, AppState, Msg>, s: &AppState, t: &Theme, area: Rect) {
    let Some(overlay) = &s.active_overlay else {
        return;
    };
    let panel = bounds(area, overlay);
    ctx.paint_widget(
        Block::default().style(Style::default().bg(t.background)),
        panel,
    );
    let a = inset(panel, 2, 1);
    if a.is_empty() {
        return;
    }
    let title = match overlay {
        Overlay::Search => "Search",
        Overlay::Queue => "Up next",
        Overlay::CommandPalette => "Command palette",
        Overlay::Help => "Make yourself at home",
        Overlay::Settings => "Settings",
        Overlay::Lyrics => "Lyrics",
        Overlay::ContextMenu(_) => "Song actions",
    };
    text(
        ctx,
        title,
        Rect::new(a.x, a.y, a.width.saturating_sub(8), 1),
        t.foreground,
        true,
    );
    button(
        ctx,
        "close",
        "× Close",
        Rect::new(a.right().saturating_sub(8), a.y, 8.min(a.width), 1),
        Msg::CloseOverlay,
        false,
    );
    line(ctx, row(a, 1), t);
    let content = Rect::new(
        a.x,
        a.y + 3.min(a.height),
        a.width,
        a.height.saturating_sub(3),
    );
    match overlay {
        Overlay::Search => {
            let cursor = if s.reduced_motion || (s.now.as_millis() / 600).is_multiple_of(2) {
                "▏"
            } else {
                " "
            };
            let input = if s.search_query.is_empty() {
                "Find songs, artists, albums…".to_string()
            } else {
                format!("{}{}", s.search_query, cursor)
            };
            text(
                ctx,
                input,
                row(content, 0),
                if s.search_query.is_empty() {
                    t.muted_foreground
                } else {
                    t.foreground
                },
                false,
            );
            text(
                ctx,
                if s.search_error.is_some() {
                    "Search failed; edit the query to retry".into()
                } else if s.search_loading {
                    "Searching Apple Music…".into()
                } else {
                    format!("{} songs", s.filtered_search_results().len())
                },
                row(content, 2),
                t.muted_foreground,
                false,
            );
            let list = Rect::new(
                content.x,
                content.y + 4.min(content.height),
                content.width,
                content.height.saturating_sub(4),
            );
            let tracks = s.filtered_search_results();
            if tracks.is_empty() {
                text(
                    ctx,
                    if s.search_loading {
                        ""
                    } else {
                        "No matching songs."
                    },
                    row(list, 0),
                    t.muted_foreground,
                    false,
                );
            } else {
                render_list(ctx, s, ListKind::Search, track_rows(&tracks, s), list);
            }
        }
        Overlay::CommandPalette | Overlay::Help => {
            text(
                ctx,
                if *overlay == Overlay::CommandPalette {
                    format!("> {}▏", s.command_filter)
                } else {
                    "Tab to focus · Enter to activate · Esc to go back".into()
                },
                row(content, 0),
                t.muted_foreground,
                false,
            );
            let list = Rect::new(
                content.x,
                content.y + 2.min(content.height),
                content.width,
                content.height.saturating_sub(4),
            );
            let items = s
                .filtered_commands()
                .iter()
                .map(|c| ListRow {
                    title: c.title.into(),
                    subtitle: c.category.into(),
                    detail: String::new(),
                    duration: c.shortcut.unwrap_or("").into(),
                    context: None,
                    current: false,
                })
                .collect();
            render_list(ctx, s, ListKind::Commands, items, list);
            text(
                ctx,
                "Wheel: scroll · Drag: seek / volume / reorder queue",
                row(content, content.height.saturating_sub(1)),
                t.muted_foreground,
                false,
            );
        }
        Overlay::Queue => {
            button(
                ctx,
                "clear",
                "Clear queue",
                Rect::new(content.x, content.y, 14, 1),
                Msg::ClearQueue,
                false,
            );
            text(
                ctx,
                format!("{} songs", s.player.queue.len()),
                Rect::new(content.right().saturating_sub(12), content.y, 12, 1),
                t.muted_foreground,
                false,
            );
            let list = Rect::new(
                content.x,
                content.y + 2.min(content.height),
                content.width,
                content.height.saturating_sub(3),
            );
            if s.player.queue.is_empty() {
                empty(
                    ctx,
                    t,
                    list,
                    "Room for another song.",
                    "Open a song's ··· menu and choose Play next or Add to queue.",
                );
            } else {
                let mut items = track_rows(&s.player.queue, s);
                for (i, item) in items.iter_mut().enumerate() {
                    item.context = Some(Msg::RemoveFromQueue(i));
                }
                render_list(ctx, s, ListKind::Queue, items, list);
            }
            text(
                ctx,
                "Drag to reorder · ··· or Delete to remove",
                row(content, content.height.saturating_sub(1)),
                t.muted_foreground,
                false,
            );
        }
        Overlay::ContextMenu(id) => {
            if let Some(track) = s.find_track(id) {
                text(
                    ctx,
                    format!("{} · {}", track.title, track.artist),
                    row(content, 0),
                    t.muted_foreground,
                    false,
                );
                let mut actions = vec![
                    ("Play now", Msg::PlayTrack(id.clone())),
                    ("Play next", Msg::PlayNext(id.clone())),
                    ("Add to queue", Msg::AddToQueue(id.clone())),
                ];
                if s.library.find_album(&track.album_id).is_some() {
                    actions.push(("View album", Msg::OpenAlbum(track.album_id.clone())));
                }
                if s.library.find_artist(&track.artist_id).is_some() {
                    actions.push(("View artist", Msg::OpenArtist(track.artist_id.clone())));
                }
                for (i, (label, action)) in actions.into_iter().enumerate() {
                    button(
                        ctx,
                        &format!("action{i}"),
                        label,
                        row(content, i as u16 + 2),
                        action,
                        i == 0,
                    );
                }
            }
        }
        Overlay::Settings => {
            let engine = if s.demo {
                "Demo mode; no browser or audio.".into()
            } else if s.connecting {
                "Connecting…".into()
            } else {
                s.browser_name.clone().unwrap_or("Unavailable".into())
            };
            let mut lines = vec![
                format!("Audio engine    {engine}"),
                format!(
                    "Apple Music     {}",
                    if s.is_authorized {
                        "Signed in"
                    } else {
                        "Not signed in"
                    }
                ),
                format!(
                    "Motion          {}",
                    if s.reduced_motion {
                        "Reduced"
                    } else {
                        "Enabled"
                    }
                ),
                String::new(),
                "Audio output follows your system's default device.".into(),
            ];
            if let Some(e) = &s.engine_error {
                lines.push(e.clone());
            }
            if !s.is_authorized && !s.demo {
                lines.push("Sign in with: malus --login".into());
            }
            ctx.paint_widget(
                Paragraph::new(lines.join("\n"))
                    .style(Style::default().fg(t.muted_foreground))
                    .wrap(Wrap { trim: true }),
                content,
            );
        }
        Overlay::Lyrics => {
            empty(
                ctx,
                t,
                content,
                "Lyrics aren't available here yet.",
                "Apple Music's current bridge does not supply synchronized lyrics for this track.",
            );
        }
    }
}
