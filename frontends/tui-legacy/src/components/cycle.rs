// Copied from ratcn 0.0.3: src/components/cycle.rs

// A one-of-many control that cycles: it shows the current option, and every
// act — a click, Enter, Space, an arrow — advances to the next one.
//
// ```text
// Medium
// ```
//
// A Cycle is the settings-row control for more than two options. Two options
// are a [`Checkbox`](ratcn::Checkbox) wearing the words as its markers; three
// or more, or an ordered scale (Small/Medium/Large), are a Cycle.
//
// The value sits in a subtle field at rest, then brightens while hovered or
// focused; it mutes while disabled. That resting field hints that the value
// is clickable without competing with the setting label.

use std::{fmt, rc::Rc};

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect, Size},
    style::{Color, Style},
    text::Line,
    widgets::Widget,
};

use ratcn::{
    ListStyle, Theme,
    button_shape::filled_middle,
    linear_nav::{Axis, step_key},
    runtime::{
        Component, DeclareCtx, Event, EventCtx, EventResult, KeyCode, KeyEvent, MeasuredComponent,
        MouseButton, MouseKind, PaintCtx, ScopeOptions, Step,
    },
    text_width,
    theme::resolve_style,
};

/// A cycle's colors, sharing the List's three background states.
///
/// At rest [`foreground`](Self::foreground) sits on
/// [`background`](Self::background). Hover and focus use the corresponding
/// List backgrounds, with hover beating focus. Disabled mutes the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CycleStyle {
    /// Value color at rest.
    pub foreground: Color,
    /// Background at rest.
    pub background: Color,
    /// Value color while focused.
    pub focused_foreground: Color,
    /// Background while focused.
    pub focused_background: Color,
    /// Value color while hovered.
    pub hovered_foreground: Color,
    /// Background while hovered.
    pub hovered_background: Color,
    /// Value color while disabled.
    pub disabled_foreground: Color,
}

impl CycleStyle {
    /// The no-theme starting point: plain ANSI colors that render on any
    /// terminal, with the List's three fallback backgrounds and readable text.
    #[must_use]
    pub const fn fallback() -> Self {
        Self {
            foreground: Color::Gray,
            background: Color::Reset,
            focused_foreground: Color::Black,
            focused_background: Color::Reset,
            hovered_foreground: Color::Black,
            hovered_background: Color::DarkGray,
            disabled_foreground: Color::DarkGray,
        }
    }

    /// Colors derived from a theme: the List's three backdrops, with the value
    /// keeping the theme's foreground on them.
    #[must_use]
    pub fn from_theme(theme: &Theme) -> Self {
        let list = ListStyle::from_theme(theme);
        Self {
            foreground: list.foreground,
            background: list.background,
            focused_foreground: theme.foreground,
            focused_background: list.focused_background,
            hovered_foreground: theme.foreground,
            hovered_background: list.hovered_background,
            disabled_foreground: theme.muted_foreground,
        }
    }

    /// One paint pass's colors (see [`Self::from_theme`]). Disabled wins over
    /// hover, which wins over focus, which wins over rest.
    fn resolve(self, focused: bool, hovered: bool, disabled: bool) -> Style {
        if disabled {
            Style::default().fg(self.disabled_foreground)
        } else if hovered {
            Style::default()
                .fg(self.hovered_foreground)
                .bg(self.hovered_background)
        } else if focused {
            Style::default()
                .fg(self.focused_foreground)
                .bg(self.focused_background)
        } else {
            Style::default().fg(self.foreground).bg(self.background)
        }
    }
}

/// A cycle that only draws — an ordinary ratatui [`Widget`] with no focus,
/// events, or state. It paints the current option across `area`, the way the
/// closed [`SelectWidget`](ratcn::SelectWidget) paints its value: the widget
/// takes the one string it shows, and which option that is stays the caller's
/// business.
#[derive(Debug)]
pub struct CycleWidget<'a> {
    value: &'a str,
    focused: bool,
    hovered: bool,
    disabled: bool,
    theme: Option<Theme>,
    style: Option<CycleStyle>,
}

impl<'a> CycleWidget<'a> {
    /// A cycle showing `value`, its current option.
    #[must_use]
    pub const fn new(value: &'a str) -> Self {
        Self {
            value,
            focused: false,
            hovered: false,
            disabled: false,
            theme: None,
            style: None,
        }
    }

    /// Take colors from `theme`.
    #[must_use]
    pub const fn themed(mut self, theme: &Theme) -> Self {
        self.theme = Some(*theme);
        self
    }

    /// Exact colors, taking precedence over [`themed`](Self::themed).
    #[must_use]
    pub const fn style(mut self, style: CycleStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Paint the focused background.
    #[must_use]
    pub const fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Paint the hovered background.
    #[must_use]
    pub const fn hovered(mut self, hovered: bool) -> Self {
        self.hovered = hovered;
        self
    }

    /// Paint muted.
    #[must_use]
    pub const fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    fn resolved_style(&self) -> CycleStyle {
        match (self.style, self.theme) {
            (Some(style), _) => style,
            (None, Some(theme)) => CycleStyle::from_theme(&theme),
            (None, None) => CycleStyle::fallback(),
        }
    }
}

impl Widget for CycleWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let style = self
            .resolved_style()
            .resolve(self.focused, self.hovered, self.disabled);
        Line::from(filled_middle(self.value, area.width as usize))
            .style(style)
            .render(area, buf);
    }
}

type ReadSelectedFn<S> = Rc<dyn Fn(&S) -> usize>;
type OnChangeFn<M> = Rc<dyn Fn(usize) -> M>;
type StyleFn = Rc<dyn Fn(&Theme) -> CycleStyle>;

/// A control that cycles through its options in place: the current value is
/// all it shows, and every click, <kbd>Enter</kbd>, <kbd>Space</kbd>,
/// <kbd>Right</kbd>/<kbd>l</kbd>, or <kbd>Ctrl+N</kbd> advances to the next,
/// wrapping at the end. <kbd>Left</kbd>/<kbd>h</kbd> walks backward, as does
/// <kbd>Ctrl+P</kbd>. Home and End step nothing: a ring has no ends.
/// Shift belongs to the app, so Shift+Space passes through untouched.
///
/// The row paints in a subtle field at rest, then brightens on hover or
/// focus, so a column of cycles reads as values without hiding that each one
/// is actionable.
///
/// The selection lives in app state and arrives through
/// [`selection`](Self::selection) as an index; without that binding the Cycle
/// paints but is not focusable and answers no events.
pub struct Cycle<S, M> {
    options: Vec<String>,
    selection: Option<(ReadSelectedFn<S>, OnChangeFn<M>)>,
    disabled: bool,
    style: Option<StyleFn>,
    /// Which edge of the declared area the value hugs.
    align: Alignment,
    /// The bound selection, resolved and clamped once per declaration.
    resolved_selected: usize,
    /// The columns the current option paints — the Cycle is as wide as the
    /// value it shows, no wider.
    resolved_width: u16,
}

impl<S, M> fmt::Debug for Cycle<S, M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Cycle")
            .field("options", &self.options)
            .field("selection", &self.selection.is_some())
            .field("disabled", &self.disabled)
            .field("style", &self.style.is_some())
            .finish_non_exhaustive()
    }
}

impl<S, M> Cycle<S, M> {
    /// Construct a Cycle from its options, in cycling order.
    #[must_use]
    pub fn new(options: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            options: options.into_iter().map(Into::into).collect(),
            selection: None,
            disabled: false,
            style: None,
            align: Alignment::Left,
            resolved_selected: 0,
            resolved_width: 0,
        }
    }

    /// Which edge of the declared area the value hugs; left by default.
    ///
    /// The Cycle stays exactly as wide as its value — this moves that value,
    /// paint and hit target together, within the area the app declares. With
    /// `Alignment::Right` a settings row is one declaration: the name painted
    /// at the left edge, the Cycle hugging the right.
    #[must_use]
    pub const fn align(mut self, align: Alignment) -> Self {
        self.align = align;
        self
    }

    /// The columns that fit every option — the widest value, so an area
    /// reserved from this never truncates and never shifts as the value
    /// cycles. What the Cycle paints each frame is narrower: exactly its
    /// current value.
    #[must_use]
    pub fn width(&self) -> u16 {
        self.options
            .iter()
            .map(|option| text_width::display_width_u16(option))
            .max()
            .unwrap_or(0)
    }

    /// Bind the selection and the message that moves it.
    ///
    /// `read` returns the index of the option shown; an out-of-range answer
    /// clamps to the last option rather than panicking. `on_change` receives
    /// the index the user advanced or backed up to. Without this binding the
    /// Cycle is not focusable and answers no events.
    #[must_use]
    pub fn selection(
        mut self,
        read: impl Fn(&S) -> usize + 'static,
        on_change: impl Fn(usize) -> M + 'static,
    ) -> Self {
        self.selection = Some((Rc::new(read), Rc::new(on_change)));
        self
    }

    /// Paint and answer muted, ignoring every event.
    #[must_use]
    pub const fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Supply exact colors, taking precedence over the theme.
    #[must_use]
    pub fn style(mut self, style: impl Fn(&Theme) -> CycleStyle + 'static) -> Self {
        self.style = Some(Rc::new(style));
        self
    }

    fn can_act(&self) -> bool {
        !self.disabled && !self.options.is_empty() && self.selection.is_some()
    }

    /// The rect the current value paints in and answers events in: one column
    /// of the List-derived field surrounds either side of the value, hugging
    /// the [`align`](Self::align) edge of the declared area.
    fn value_area(&self, area: Rect) -> Rect {
        let width = self.resolved_width.saturating_add(2).min(area.width);
        let x = self.aligned_x(area, width);
        ratcn::geometry::fixed_height(Rect { x, width, ..area }, 1)
    }

    fn aligned_x(&self, area: Rect, width: u16) -> u16 {
        match self.align {
            Alignment::Left => area.x,
            Alignment::Center => area.x + (area.width - width) / 2,
            Alignment::Right => area.x + (area.width - width),
        }
    }

    /// Advance one option. Forward past the end wraps to the first; backward
    /// before the start wraps to the last.
    fn step(&self, backward: bool) -> EventResult<M> {
        let Some((_, on_change)) = &self.selection else {
            return EventResult::Ignored;
        };
        let len = self.options.len();
        let next = if backward {
            (self.resolved_selected + len - 1) % len
        } else {
            (self.resolved_selected + 1) % len
        };
        EventResult::Emit(on_change(next))
    }

    /// The key map: the one horizontal step map every item control shares —
    /// Left/Right, their vi letters, and the readline chords, asked first
    /// because it owns Ctrl+N/Ctrl+P — then the commit keys, which advance.
    /// Home and End step nothing: a ring has no ends. Shift belongs to the
    /// app, so Shift+Space passes through untouched.
    fn handle_key(&self, key: KeyEvent) -> EventResult<M> {
        if let Some(step) = step_key(key, Axis::Horizontal) {
            return self.step(matches!(step, Step::Backward));
        }
        if key.modifiers.any() {
            return EventResult::Ignored;
        }
        match key.code {
            KeyCode::Enter | KeyCode::Char(' ') => self.step(false),
            _ => EventResult::Ignored,
        }
    }
}

impl<S: 'static, M: 'static> Component<S, M> for Cycle<S, M> {
    fn prepare(&mut self, state: &S) {
        self.resolved_selected = self
            .selection
            .as_ref()
            .map_or(0, |(read, _)| read(state))
            .min(self.options.len().saturating_sub(1));
        self.resolved_width = self
            .options
            .get(self.resolved_selected)
            .map_or(0, |option| text_width::display_width_u16(option));
    }

    fn declare(&mut self, _ctx: &mut DeclareCtx<'_, S, M>) {
        // Everything a Cycle is lives on its own node: the paint below and
        // the events answered here. There is nothing to declare inside it.
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_, S>) {
        // `prepare` clamped the selection, so this is `None` only with no
        // options at all — nothing to show, nothing to paint.
        let Some(value) = self.options.get(self.resolved_selected) else {
            return;
        };
        let style = resolve_style(self.style.as_deref(), ctx.theme, CycleStyle::from_theme);
        let widget = CycleWidget::new(value)
            .focused(ctx.focused())
            .hovered(ctx.hovered())
            .disabled(self.disabled)
            .style(style);
        ctx.widget(widget, self.value_area(ctx.area()));
    }

    fn handle_event(
        &mut self,
        event: &Event,
        _state: &S,
        _ctx: &mut EventCtx<'_>,
    ) -> EventResult<M> {
        if !self.can_act() {
            return EventResult::Ignored;
        }
        match event {
            Event::Mouse(mouse) => match mouse.kind {
                MouseKind::Click(MouseButton::Left) => self.step(false),
                _ => EventResult::Ignored,
            },
            Event::Key(key) => self.handle_key(*key),
            _ => EventResult::Ignored,
        }
    }

    fn scope_options(&self) -> ScopeOptions {
        ScopeOptions::default().focusable(self.can_act())
    }

    fn interaction_area(&self, area: Rect) -> Rect {
        self.value_area(area)
    }
}

impl<S: 'static, M: 'static> MeasuredComponent<S, M> for Cycle<S, M> {
    fn measure(&self) -> Size {
        Size::new(self.width(), 1)
    }
}
