use super::widgets::*;
use crate::app::{AppState, Msg, Overlay, Screen};
use ratatui::layout::Rect;
use ratcn::{Theme, runtime::DeclareCtx};
pub fn render(ctx: &mut DeclareCtx<'_, AppState, Msg>, s: &AppState, t: &Theme, a: Rect) {
    let wide = a.width >= 82;
    let brand = if wide { 10 } else { 5 };
    text(
        ctx,
        if wide { "♫  malus" } else { "♫" },
        Rect::new(a.x, a.y, brand, 1),
        t.primary,
        true,
    );
    let labels = if wide {
        ["Listen Now", "Browse", "Radio", "Library"]
    } else {
        ["Home", "Browse", "Radio", "Library"]
    };
    let mut x = a.x + brand;
    for (i, (label, screen)) in labels
        .into_iter()
        .zip([
            Screen::ListenNow,
            Screen::Browse,
            Screen::Radio,
            Screen::Library,
        ])
        .enumerate()
    {
        let width = if a.width < 55 {
            6
        } else {
            label.len() as u16 + 3
        };
        button(
            ctx,
            &format!("nav{i}"),
            label,
            Rect::new(x, a.y, width, 1),
            Msg::Navigate(screen.clone()),
            s.active_screen == screen,
        );
        x += width;
    }
    let tools = if wide { 27 } else { 7 };
    let tx = a.right().saturating_sub(tools);
    if tx > x + 12 {
        let status = if s.demo {
            "Demo · no audio".to_string()
        } else if s.connecting {
            format!(
                "{} Connecting",
                ["◐", "◓", "◑", "◒"][(s.now.as_millis() / 140 % 4) as usize]
            )
        } else if s.is_authorized {
            "● Connected".into()
        } else if s.engine_error.is_some() {
            "○ Offline".into()
        } else {
            "○ Sign in".into()
        };
        text(
            ctx,
            status,
            Rect::new(x + 2, a.y, tx - x - 2, 1),
            t.muted_foreground,
            false,
        );
    }
    button(
        ctx,
        "search",
        if wide { "Search  /" } else { "/" },
        Rect::new(tx, a.y, if wide { 12 } else { 3 }, 1),
        Msg::OpenOverlay(Overlay::Search),
        false,
    );
    button(
        ctx,
        "settings",
        if wide { "Settings" } else { "," },
        Rect::new(
            tx + if wide { 12 } else { 3 },
            a.y,
            if wide { 10 } else { 2 },
            1,
        ),
        Msg::OpenOverlay(Overlay::Settings),
        false,
    );
    button(
        ctx,
        "help",
        "?",
        Rect::new(a.right() - 2, a.y, 2, 1),
        Msg::OpenOverlay(Overlay::Help),
        false,
    );
}
