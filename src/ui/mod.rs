//! One layout and one interaction vocabulary across every screen.
pub mod artwork;
pub mod overlays;
pub mod player_bar;
pub mod screens;
pub mod top_bar;
pub mod widgets;
use crate::app::{AppState, Msg};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Color,
    widgets::Block,
};
use ratcn::{
    Theme,
    runtime::{DeclareCtx, ScopeOptions},
};
use widgets::*;

pub fn theme(base: &Theme) -> Theme {
    let mut t = *base;
    // Keep the terminal's light/dark polarity, with a restrained Music accent.
    let light =
        matches!(base.background,Color::Rgb(r,g,b) if u16::from(r)+u16::from(g)+u16::from(b)>500);
    if light {
        t.background = Color::Rgb(250, 249, 247);
        t.foreground = Color::Rgb(32, 31, 36);
        t.muted_foreground = Color::Rgb(111, 107, 117);
        t.secondary = Color::Rgb(238, 232, 232);
        t.field = Color::Rgb(243, 237, 237);
        t.primary = Color::Rgb(192, 39, 70);
        t.border = Color::Rgb(220, 215, 220);
    } else {
        t.background = Color::Rgb(17, 18, 22);
        t.foreground = Color::Rgb(234, 233, 239);
        t.muted_foreground = Color::Rgb(147, 147, 160);
        t.secondary = Color::Rgb(43, 32, 41);
        t.field = Color::Rgb(36, 37, 45);
        t.primary = Color::Rgb(250, 99, 126);
        t.border = Color::Rgb(51, 52, 62);
    }
    t.secondary_foreground = t.foreground;
    t.primary_foreground = t.background;
    t
}
pub fn render_app(
    ctx: &mut DeclareCtx<'_, AppState, Msg>,
    state: &AppState,
    t: &Theme,
    area: Rect,
) {
    ctx.paint_widget(
        Block::default().style(ratatui::style::Style::default().bg(t.background)),
        area,
    );
    if area.width < 40 || area.height < 12 {
        text(
            ctx,
            "Malus needs at least 40 × 12 cells.",
            row(area, 0),
            t.foreground,
            true,
        );
        text(
            ctx,
            "Resize your terminal · Ctrl+C to quit",
            row(area, 2),
            t.muted_foreground,
            false,
        );
        if state.active_overlay.is_some() {
            ctx.modal_scope("overlay", area, ScopeOptions::default(), |_| {});
        }
        return;
    }
    let rows = Layout::vertical([
        Constraint::Length(if area.height >= 22 { 3 } else { 1 }),
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
        Constraint::Length(3),
    ])
    .split(area);
    let margin = if area.width >= 65 { 2 } else { 1 };
    top_bar::render(
        ctx,
        state,
        t,
        inset(rows[0], margin, if rows[0].height > 1 { 1 } else { 0 }),
    );
    line(ctx, inset(rows[1], margin, 0), t);
    ctx.scope(
        "content",
        inset(rows[2], margin, 1),
        ScopeOptions::default(),
        |ctx| {
            let content = ctx.area();
            screens::render(ctx, state, t, content);
            let progress = state.now.saturating_sub(state.transition_at).as_secs_f64() / 0.16;
            if !state.reduced_motion && progress < 1.0 {
                let background = t.background;
                ctx.paint(move |paint| {
                    paint.with_buffer(|buf| {
                        for y in content.y..content.bottom() {
                            for x in content.x..content.right() {
                                if let Some(c) = buf.cell_mut((x, y))
                                    && let (Color::Rgb(r, g, b), Color::Rgb(br, bg, bb)) =
                                        (c.fg, background)
                                {
                                    let mix = |f: u8, b: u8| {
                                        (f64::from(b)
                                            + (f64::from(f) - f64::from(b))
                                                * (0.35 + 0.65 * progress))
                                            as u8
                                    };
                                    c.set_fg(Color::Rgb(mix(r, br), mix(g, bg), mix(b, bb)));
                                }
                            }
                        }
                    });
                });
            }
        },
    );
    line(ctx, inset(rows[3], margin, 0), t);
    player_bar::render(ctx, state, t, inset(rows[4], margin, 0));
    if state.active_overlay.is_some() {
        ctx.modal_scope("overlay", area, ScopeOptions::default(), |ctx| {
            overlays::render(ctx, state, t, area)
        });
    }
}
