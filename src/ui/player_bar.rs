use super::widgets::*;
use crate::{
    app::{AppState, Msg, Overlay, Screen},
    model::{PlaybackStatus, RepeatMode},
};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use ratcn::{Theme, runtime::DeclareCtx};
pub fn render(ctx: &mut DeclareCtx<'_, AppState, Msg>, s: &AppState, t: &Theme, a: Rect) {
    if a.height < 3 {
        return;
    }
    let time = if s.player.current_track.is_some() {
        format!(
            "{} / {}",
            duration(s.position_secs() as u64),
            duration(s.player.duration_ms() / 1000)
        )
    } else {
        "—:— / —:—".into()
    };
    let left = Rect::new(a.x, a.y, a.width.saturating_sub(17), 1);
    if let Some(track) = s.player.current_track() {
        ctx.paint_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    track.title.clone(),
                    Style::default()
                        .fg(t.foreground)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  ·  {}", track.artist),
                    Style::default().fg(t.muted_foreground),
                ),
            ])),
            left,
        );
    } else {
        text(
            ctx,
            if s.demo {
                "Demo mode · choose a song to preview the UI"
            } else {
                "Nothing playing yet"
            },
            left,
            t.muted_foreground,
            false,
        );
    }
    text(
        ctx,
        time,
        Rect::new(a.right() - 16, a.y, 16, 1),
        t.muted_foreground,
        false,
    );
    ctx.component(
        "seek",
        Slider {
            volume: false,
            value: s.position_secs(),
            max: s.player.duration_ms() as f64 / 1000.0,
            area: Rect::default(),
        },
        row(a, 1),
    );
    let mut x = a.x;
    let y = a.y + 2;
    let buttons = [
        ("prev", " ‹ ", Msg::PrevTrack, 4, false),
        (
            "play",
            if s.player.status == PlaybackStatus::Playing {
                "Pause"
            } else {
                "Play"
            },
            Msg::TogglePlay,
            7,
            true,
        ),
        ("next", " › ", Msg::NextTrack, 4, false),
    ];
    for (id, label, action, w, active) in buttons {
        button(ctx, id, label, Rect::new(x, y, w, 1), action, active);
        x += w;
    }
    if a.width >= 70 {
        button(
            ctx,
            "shuffle",
            "Shuffle",
            Rect::new(x + 1, y, 9, 1),
            Msg::ToggleShuffle,
            s.player.shuffle,
        );
        x += 10;
        button(
            ctx,
            "repeat",
            match s.player.repeat {
                RepeatMode::One => "Repeat 1",
                _ => "Repeat",
            },
            Rect::new(x, y, 10, 1),
            Msg::CycleRepeat,
            s.player.repeat != RepeatMode::Off,
        );
    }
    let right = if a.width >= 90 {
        43
    } else if a.width >= 65 {
        30
    } else {
        19
    };
    let mut x = a.right().saturating_sub(right);
    if a.width >= 65 {
        text(
            ctx,
            format!("{:>3}%", s.player.volume),
            Rect::new(x, y, 5, 1),
            t.muted_foreground,
            false,
        );
        x += 5;
        ctx.component(
            "volume",
            Slider {
                volume: true,
                value: s.player.volume as f64,
                max: 100.0,
                area: Rect::default(),
            },
            Rect::new(x, y, if a.width >= 90 { 10 } else { 6 }, 1),
        );
        x += if a.width >= 90 { 12 } else { 8 };
    }
    if a.width >= 90 {
        button(
            ctx,
            "lyrics",
            "Lyrics",
            Rect::new(x, y, 8, 1),
            Msg::ToggleOverlay(Overlay::Lyrics),
            false,
        );
        x += 8;
    }
    button(
        ctx,
        "queue",
        "Queue",
        Rect::new(x, y, 8, 1),
        Msg::ToggleOverlay(Overlay::Queue),
        false,
    );
    x += 8;
    button(
        ctx,
        "now",
        "Playing",
        Rect::new(x, y, a.right().saturating_sub(x), 1),
        Msg::Navigate(Screen::NowPlaying),
        s.active_screen == Screen::NowPlaying,
    );
}
pub fn duration(s: u64) -> String {
    format!("{}:{:02}", s / 60, s % 60)
}
