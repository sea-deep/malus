//! Shared ratcn controls: every list scrolls, every slider captures its drag.
use crate::{
    app::{AppState, Cursor, ListKind, Msg},
    components::{Button, ButtonVariant},
    model::LyricLine,
};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use ratcn::{
    Theme,
    runtime::{
        Component, DeclareCtx, Event, EventCtx, EventResult, KeyCode, MouseButton, MouseKind,
        PaintCtx, ScopeOptions, ScrollDirection,
    },
};

pub fn text(
    ctx: &mut DeclareCtx<'_, AppState, Msg>,
    value: impl Into<String>,
    area: Rect,
    color: ratatui::style::Color,
    bold: bool,
) {
    let style = Style::default().fg(color);
    ctx.paint_widget(
        Paragraph::new(value.into()).style(if bold {
            style.add_modifier(Modifier::BOLD)
        } else {
            style
        }),
        area,
    );
}
pub fn button(
    ctx: &mut DeclareCtx<'_, AppState, Msg>,
    id: &str,
    label: &str,
    area: Rect,
    action: Msg,
    active: bool,
) {
    if area.is_empty() {
        return;
    }
    ctx.component(
        id.to_string(),
        Button::new(label)
            .variant(if active {
                ButtonVariant::Secondary
            } else {
                ButtonVariant::Ghost
            })
            .on_press(move || action.clone()),
        area,
    );
}
pub fn line(ctx: &mut DeclareCtx<'_, AppState, Msg>, area: Rect, theme: &Theme) {
    text(
        ctx,
        "─".repeat(area.width as usize),
        area,
        theme.border,
        false,
    );
}
pub fn row(area: Rect, y: u16) -> Rect {
    Rect::new(
        area.x,
        area.y + y.min(area.height),
        area.width,
        u16::from(y < area.height),
    )
}
pub fn inset(area: Rect, x: u16, y: u16) -> Rect {
    area.inner(ratatui::layout::Margin::new(
        x.min(area.width / 2),
        y.min(area.height / 2),
    ))
}

#[derive(Clone)]
pub struct ListRow {
    pub title: String,
    pub subtitle: String,
    pub detail: String,
    pub duration: String,
    pub context: Option<Msg>,
    pub current: bool,
}
pub struct SongList {
    pub kind: ListKind,
    pub rows: Vec<ListRow>,
    pub cursor: Cursor,
    pub area: Rect,
    pub frame: u64,
}
#[derive(Default)]
struct Interaction {
    hover: Option<usize>,
    drag: Option<usize>,
}
impl Component<AppState, Msg> for SongList {
    fn declare(&mut self, ctx: &mut DeclareCtx<'_, AppState, Msg>) {
        self.area = ctx.area();
    }
    fn scope_options(&self) -> ScopeOptions {
        ScopeOptions::default().focusable(true)
    }
    fn paint(&mut self, ctx: &mut PaintCtx<'_, AppState>) {
        let a = ctx.area();
        if a.is_empty() {
            return;
        }
        let t = *ctx.theme;
        let selected = self.cursor.selected;
        let start = self.cursor.start(a.height as usize, self.rows.len());
        let hover = ctx
            .hover_position()
            .filter(|p| a.contains(*p))
            .map(|p| start + p.y.saturating_sub(a.y) as usize);
        let focused = ctx.focused();
        for (index, item) in self
            .rows
            .iter()
            .enumerate()
            .skip(start)
            .take(a.height as usize)
        {
            let y = a.y + (index - start) as u16;
            let area = Rect::new(a.x, y, a.width, 1);
            let chosen = index == selected;
            let bg = if hover == Some(index) {
                t.field
            } else if chosen {
                t.secondary
            } else {
                t.background
            };
            let fg = if item.current {
                t.primary
            } else {
                t.foreground
            };
            let style = Style::default().bg(bg).fg(fg);
            let title_style = if chosen || item.current {
                style.add_modifier(Modifier::BOLD)
            } else {
                style
            };
            let prefix = if item.current {
                if self.frame == u64::MAX {
                    "‖".to_string()
                } else {
                    ["▂▄▆", "▄▆▃", "▆▃▅", "▃▅▂"][(self.frame / 5 % 4) as usize].to_string()
                }
            } else if chosen && focused {
                " › ".to_string()
            } else {
                format!("{:>3}", index + 1)
            };
            let content = a.width.saturating_sub(15);
            let (title_width, artist_width, album_width) = if a.width >= 105 {
                (
                    content * 44 / 100,
                    content * 26 / 100,
                    content - content * 44 / 100 - content * 26 / 100,
                )
            } else if a.width >= 65 {
                (content * 60 / 100, content - content * 60 / 100, 0)
            } else {
                (content, 0, 0)
            };
            let mut spans = vec![Span::styled(
                format!("{prefix:3} "),
                Style::default().fg(if chosen || item.current {
                    t.primary
                } else {
                    t.muted_foreground
                }),
            )];
            spans.push(Span::styled(fit(&item.title, title_width), title_style));
            if artist_width > 0 {
                spans.push(Span::styled(
                    fit(&item.subtitle, artist_width),
                    Style::default().fg(t.muted_foreground),
                ));
            }
            if album_width > 0 {
                spans.push(Span::styled(
                    fit(&item.detail, album_width),
                    Style::default().fg(t.muted_foreground),
                ));
            }
            spans.push(Span::styled(
                format!(
                    "{:>5}  {}",
                    item.duration,
                    if item.context.is_some() {
                        "···"
                    } else {
                        "   "
                    }
                ),
                Style::default().fg(t.muted_foreground),
            ));
            ctx.widget(Paragraph::new(Line::from(spans)).style(style), area);
        }
        if self.rows.len() > a.height as usize && a.width > 0 {
            let height = a.height as usize;
            let thumb = (height * height / self.rows.len()).max(1);
            let top = start * (height - thumb) / self.rows.len().saturating_sub(height).max(1);
            let color = t.border;
            ctx.with_buffer(|buf| {
                for y in 0..height {
                    if let Some(c) = buf.cell_mut((a.right() - 1, a.y + y as u16)) {
                        c.set_char(if y >= top && y < top + thumb {
                            '┃'
                        } else {
                            '│'
                        })
                        .set_fg(color);
                    }
                }
            });
        }
    }
    fn handle_event(
        &mut self,
        event: &Event,
        state: &AppState,
        ctx: &mut EventCtx<'_>,
    ) -> EventResult<Msg> {
        let rows = self.area.height as usize;
        let start = self.cursor.start(rows, self.rows.len());
        let message = match event {
            Event::Key(k) if self.kind == ListKind::Queue && k.modifiers.alt => match k.code {
                KeyCode::Up => Some(Msg::MoveQueue(
                    self.cursor.selected,
                    self.cursor.selected.saturating_sub(1),
                )),
                KeyCode::Down => Some(Msg::MoveQueue(
                    self.cursor.selected,
                    (self.cursor.selected + 1).min(self.rows.len().saturating_sub(1)),
                )),
                _ => None,
            },
            Event::Key(k) => match k.code {
                KeyCode::Up | KeyCode::Char('k') => Some(Msg::MoveSelection(self.kind, -1, rows)),
                KeyCode::Down | KeyCode::Char('j') => Some(Msg::MoveSelection(self.kind, 1, rows)),
                KeyCode::PageUp => Some(Msg::MoveSelection(self.kind, -(rows as i32), rows)),
                KeyCode::PageDown => Some(Msg::MoveSelection(self.kind, rows as i32, rows)),
                KeyCode::Home => Some(Msg::SelectRow(self.kind, 0, rows)),
                KeyCode::End => Some(Msg::SelectRow(
                    self.kind,
                    self.rows.len().saturating_sub(1),
                    rows,
                )),
                KeyCode::Enter => state.selection_action(self.kind),
                KeyCode::Char('m') => self
                    .rows
                    .get(self.cursor.selected)
                    .and_then(|r| r.context.clone()),
                KeyCode::Delete if self.kind == ListKind::Queue => {
                    Some(Msg::RemoveFromQueue(self.cursor.selected))
                }
                _ => None,
            },
            Event::Mouse(m) => {
                let index = start + m.row.saturating_sub(self.area.y) as usize;
                match m.kind {
                    MouseKind::Scroll(ScrollDirection::Down) => {
                        Some(Msg::MoveSelection(self.kind, 3, rows))
                    }
                    MouseKind::Scroll(ScrollDirection::Up) => {
                        Some(Msg::MoveSelection(self.kind, -3, rows))
                    }
                    MouseKind::Moved => {
                        ctx.transient::<Interaction>().hover = Some(index);
                        return EventResult::Consumed;
                    }
                    MouseKind::Down(MouseButton::Left) if self.kind == ListKind::Queue => {
                        ctx.transient::<Interaction>().drag = Some(index);
                        ctx.capture_pointer(MouseButton::Left);
                        None
                    }
                    MouseKind::DragEnd(MouseButton::Left) if self.kind == ListKind::Queue => {
                        let from = ctx.transient::<Interaction>().drag.take();
                        from.filter(|i| *i != index).map(|from| {
                            Msg::MoveQueue(from, index.min(self.rows.len().saturating_sub(1)))
                        })
                    }
                    MouseKind::Click(MouseButton::Left) | MouseKind::Click(MouseButton::Right) => {
                        if let Some(item) = self.rows.get(index) {
                            if matches!(m.kind, MouseKind::Click(MouseButton::Right))
                                || m.column >= self.area.right().saturating_sub(4)
                            {
                                item.context.clone()
                            } else {
                                Some(Msg::ActivateRow(self.kind, index, rows))
                            }
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            }
            _ => None,
        };
        message.map_or(EventResult::Ignored, EventResult::Emit)
    }
}
pub fn fit(value: &str, width: u16) -> String {
    use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
    let width = usize::from(width);
    if width == 0 {
        return String::new();
    }
    let clean: String = value.chars().filter(|c| !c.is_control()).collect();
    let truncate = clean.width() > width.saturating_sub(1);
    let max = width.saturating_sub(if truncate { 2 } else { 1 });
    let mut result = String::new();
    let mut used = 0;
    for ch in clean.chars() {
        let w = ch.width().unwrap_or(0);
        if used + w > max {
            break;
        }
        result.push(ch);
        used += w;
    }
    if truncate && used < width {
        result.push('…');
        used += 1;
    }
    result.push_str(&" ".repeat(width - used));
    result
}

pub struct Slider {
    pub volume: bool,
    pub value: f64,
    pub max: f64,
    pub area: Rect,
}
impl Component<AppState, Msg> for Slider {
    fn declare(&mut self, ctx: &mut DeclareCtx<'_, AppState, Msg>) {
        self.area = ctx.area();
    }
    fn scope_options(&self) -> ScopeOptions {
        ScopeOptions::default().focusable(true)
    }
    fn paint(&mut self, ctx: &mut PaintCtx<'_, AppState>) {
        let a = ctx.area();
        let ratio = if self.max > 0.0 {
            (self.value / self.max).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let filled = (ratio * f64::from(a.width.saturating_sub(1))).round() as usize;
        let t = *ctx.theme;
        let mut spans = vec![];
        for i in 0..a.width as usize {
            let c = if i == filled && self.max > 0.0 {
                '●'
            } else {
                '─'
            };
            spans.push(Span::styled(
                c.to_string(),
                Style::default().fg(if i <= filled && self.max > 0.0 {
                    t.primary
                } else {
                    t.border
                }),
            ));
        }
        ctx.widget(Paragraph::new(Line::from(spans)), a);
    }
    fn handle_event(
        &mut self,
        event: &Event,
        _state: &AppState,
        ctx: &mut EventCtx<'_>,
    ) -> EventResult<Msg> {
        if self.max <= 0.0 {
            return EventResult::Ignored;
        }
        let value = match event {
            Event::Mouse(m) => match m.kind {
                MouseKind::Down(MouseButton::Left) | MouseKind::Drag(MouseButton::Left) => {
                    if matches!(m.kind, MouseKind::Down(MouseButton::Left)) {
                        ctx.capture_pointer(MouseButton::Left);
                    }
                    Some(
                        f64::from(
                            m.column
                                .saturating_sub(self.area.x)
                                .min(self.area.width.saturating_sub(1)),
                        ) / f64::from(self.area.width.saturating_sub(1).max(1))
                            * self.max,
                    )
                }
                MouseKind::Scroll(ScrollDirection::Up) => Some(self.value + 5.0),
                MouseKind::Scroll(ScrollDirection::Down) => Some(self.value - 5.0),
                _ => None,
            },
            Event::Key(k) => match k.code {
                KeyCode::Left => Some(self.value - 5.0),
                KeyCode::Right => Some(self.value + 5.0),
                KeyCode::Home => Some(0.0),
                KeyCode::End => Some(self.max),
                _ => None,
            },
            _ => None,
        };
        value
            .map(|v| {
                if self.volume {
                    Msg::SetVolume(v.clamp(0.0, 100.0).round() as u8)
                } else {
                    Msg::EngineSeek(v.clamp(0.0, self.max))
                }
            })
            .map_or(EventResult::Ignored, EventResult::Emit)
    }
}

pub struct LyricsList {
    pub lyrics: Vec<LyricLine>,
    pub current_time: f64,
    pub scroll_offset: Option<usize>,
    pub area: Rect,
}

impl Component<AppState, Msg> for LyricsList {
    fn declare(&mut self, ctx: &mut DeclareCtx<'_, AppState, Msg>) {
        self.area = ctx.area();
    }
    fn scope_options(&self) -> ScopeOptions {
        ScopeOptions::default().focusable(true)
    }
    fn paint(&mut self, ctx: &mut PaintCtx<'_, AppState>) {
        let a = ctx.area();
        if a.is_empty() || self.lyrics.is_empty() {
            return;
        }
        let t = *ctx.theme;
        let height = a.height as usize;
        let center = height / 2;

        let mut active_idx = None;
        for (i, line) in self.lyrics.iter().enumerate() {
            if self.current_time >= line.start_secs {
                active_idx = Some(i);
            } else {
                break;
            }
        }

        let auto_start = active_idx.unwrap_or(0).saturating_sub(center);
        let max_start = self.lyrics.len().saturating_sub(height);
        let start = self.scroll_offset.unwrap_or(auto_start).min(max_start);

        let hover = ctx
            .hover_position()
            .filter(|p| a.contains(*p))
            .map(|p| start + p.y.saturating_sub(a.y) as usize);

        for (row_offset, (index, line)) in self
            .lyrics
            .iter()
            .enumerate()
            .skip(start)
            .take(height)
            .enumerate()
        {
            let y = a.y + row_offset as u16;
            let line_area = Rect::new(a.x, y, a.width, 1);
            let is_active = Some(index) == active_idx;
            let is_past = active_idx.is_some_and(|cur| index < cur);
            let is_hovered = hover == Some(index);

            let bg = if is_hovered {
                t.field
            } else if is_active {
                t.secondary
            } else {
                t.background
            };

            let fg = if is_active {
                t.primary
            } else if is_past {
                t.muted_foreground
            } else {
                t.foreground
            };

            let mut style = Style::default().bg(bg).fg(fg);
            if is_active {
                style = style.add_modifier(Modifier::BOLD);
            }

            let prefix = if is_active {
                "▶ "
            } else if is_hovered {
                "↳ "
            } else {
                "  "
            };

            let prefix_width = 2;
            let content_width = a.width.saturating_sub(prefix_width);
            let text_fit = fit(&line.text, content_width);

            let spans = vec![
                Span::styled(
                    prefix,
                    Style::default().fg(if is_active || is_hovered {
                        t.primary
                    } else {
                        t.muted_foreground
                    }),
                ),
                Span::styled(text_fit, style),
            ];

            ctx.widget(Paragraph::new(Line::from(spans)).style(style), line_area);
        }

        if self.lyrics.len() > height && a.width > 0 {
            let thumb = (height * height / self.lyrics.len()).max(1);
            let top = start * (height - thumb) / self.lyrics.len().saturating_sub(height).max(1);
            let color = t.border;
            ctx.with_buffer(|buf| {
                for y in 0..height {
                    if let Some(c) = buf.cell_mut((a.right() - 1, a.y + y as u16)) {
                        c.set_char(if y >= top && y < top + thumb {
                            '┃'
                        } else {
                            '│'
                        })
                        .set_fg(color);
                    }
                }
            });
        }
    }

    fn handle_event(
        &mut self,
        event: &Event,
        _state: &AppState,
        _ctx: &mut EventCtx<'_>,
    ) -> EventResult<Msg> {
        let height = self.area.height as usize;
        let center = height / 2;
        let mut active_idx = None;
        for (i, line) in self.lyrics.iter().enumerate() {
            if self.current_time >= line.start_secs {
                active_idx = Some(i);
            } else {
                break;
            }
        }
        let auto_start = active_idx.unwrap_or(0).saturating_sub(center);
        let max_start = self.lyrics.len().saturating_sub(height);
        let start = self.scroll_offset.unwrap_or(auto_start).min(max_start);

        let message = match event {
            Event::Key(k) => match k.code {
                KeyCode::Up | KeyCode::Char('k') => Some(Msg::ScrollLyrics(-1)),
                KeyCode::Down | KeyCode::Char('j') => Some(Msg::ScrollLyrics(1)),
                KeyCode::PageUp => Some(Msg::ScrollLyrics(-(height as i32))),
                KeyCode::PageDown => Some(Msg::ScrollLyrics(height as i32)),
                KeyCode::Home => Some(Msg::ScrollLyrics(-(self.lyrics.len() as i32))),
                KeyCode::End => Some(Msg::ScrollLyrics(self.lyrics.len() as i32)),
                KeyCode::Char('c') if !k.modifiers.ctrl => Some(Msg::ResetLyricsScroll),
                KeyCode::Enter | KeyCode::Char(' ') => {
                    let target_idx = (start + center).min(self.lyrics.len().saturating_sub(1));
                    self.lyrics
                        .get(target_idx)
                        .map(|line| Msg::EngineSeek(line.start_secs))
                }
                _ => None,
            },
            Event::Mouse(m) => {
                let index = start + m.row.saturating_sub(self.area.y) as usize;
                match m.kind {
                    MouseKind::Scroll(ScrollDirection::Down) => Some(Msg::ScrollLyrics(2)),
                    MouseKind::Scroll(ScrollDirection::Up) => Some(Msg::ScrollLyrics(-2)),
                    MouseKind::Click(MouseButton::Left) => self
                        .lyrics
                        .get(index)
                        .map(|line| Msg::EngineSeek(line.start_secs)),
                    _ => None,
                }
            }
            _ => None,
        };
        message.map_or(EventResult::Ignored, EventResult::Emit)
    }
}
