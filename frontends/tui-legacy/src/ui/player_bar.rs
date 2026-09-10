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
    // Row 0 — track title · artist  +  elapsed / total
    let time = if s.player.current_track.is_some() {
        format!(
            "{}  /  {}",
            duration(s.position_secs() as u64),
            duration(s.player.duration_ms() / 1000)
        )
    } else {
        "—:—  /  —:—".into()
    };
    let left = Rect::new(a.x, a.y, a.width.saturating_sub(18), 1);
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
        Rect::new(a.right() - 17, a.y, 17, 1),
        t.muted_foreground,
        false,
    );

    // Row 1 — seek bar
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

    // Row 2 — transport controls (left side)
    let mut x = a.x;
    let y = a.y + 2;

    // 󰒮 skip prev  󰐊 play  󰏤 pause  󰒭 skip next
    let play_label = if s.player.status == PlaybackStatus::Playing {
        "󰏤  Pause"
    } else {
        "󰐊  Play "
    };
    let buttons: &[(&str, &str, Msg, u16, bool)] = &[
        ("prev", "󰒮", Msg::PrevTrack, 3, false),
        ("play", play_label, Msg::TogglePlay, 9, true),
        ("next", "󰒭", Msg::NextTrack, 3, false),
    ];
    for (id, label, action, w, active) in buttons {
        button(
            ctx,
            id,
            label,
            Rect::new(x, y, *w, 1),
            action.clone(),
            *active,
        );
        x += w;
    }

    if a.width >= 70 {
        // 󰒟 shuffle  󰑖 repeat  󰑘 repeat-one
        button(
            ctx,
            "shuffle",
            "󰒟  Shuffle",
            Rect::new(x + 1, y, 11, 1),
            Msg::ToggleShuffle,
            s.player.shuffle,
        );
        x += 12;
        let repeat_label = match s.player.repeat {
            RepeatMode::One => "󰑘  Repeat 1",
            _ => "󰑖  Repeat ",
        };
        button(
            ctx,
            "repeat",
            repeat_label,
            Rect::new(x, y, 11, 1),
            Msg::CycleRepeat,
            s.player.repeat != RepeatMode::Off,
        );
    }

    // Right-side controls — volume + lyrics/queue/now-playing
    let right = if a.width >= 90 {
        46
    } else if a.width >= 65 {
        32
    } else {
        20
    };
    let mut x = a.right().saturating_sub(right);
    if a.width >= 65 {
        // 󰕾 volume high
        text(
            ctx,
            format!("󰕾 {:>3}%", s.player.volume),
            Rect::new(x, y, 8, 1),
            t.muted_foreground,
            false,
        );
        x += 8;
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
        // 󰋒 microphone / lyrics
        button(
            ctx,
            "lyrics",
            "󰋒  Lyrics",
            Rect::new(x, y, 10, 1),
            Msg::ToggleOverlay(Overlay::Lyrics),
            false,
        );
        x += 10;
    }
    // 󱍙 queue
    button(
        ctx,
        "queue",
        "󱍙  Queue",
        Rect::new(x, y, 9, 1),
        Msg::ToggleOverlay(Overlay::Queue),
        false,
    );
    x += 9;
    // 󰝚 now playing
    button(
        ctx,
        "now",
        "󰝚  Playing",
        Rect::new(x, y, a.right().saturating_sub(x), 1),
        Msg::Navigate(Screen::NowPlaying),
        s.active_screen == Screen::NowPlaying,
    );
}

pub fn duration(s: u64) -> String {
    format!("{}:{:02}", s / 60, s % 60)
}
