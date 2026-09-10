// Copied from ratcn 0.0.3: src/components/button.rs

use std::fmt;

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect, Size},
    style::{Color, Style},
    text::Line,
    widgets::{Block, Widget},
};

use ratcn::Theme;
use ratcn::button_shape::{BOTTOM_CAP, TOP_CAP, cap_row, filled_middle, shape_width};
use ratcn::color::{
    DISABLED_DIM, FOCUS_SHIFT, HOVER_SHIFT, away_from, dim, ghost_fills, nearest_to,
};
use ratcn::geometry::fixed_height;
use ratcn::runtime::{
    Component, DeclareCtx, Event, EventCtx, EventResult, KeyCode, MeasuredComponent, MouseButton,
    MouseKind, PaintCtx, ScopeOptions,
};
use ratcn::theme::resolve_style;

/// How tall a button is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum ButtonSize {
    /// One row: label only, no room for a border.
    #[default]
    Small,
    /// Three rows: label with a border or fill cap above and below.
    Large,
}

impl ButtonSize {
    /// Rows this size occupies — 1 for `Small`, 3 for `Large`.
    #[must_use]
    pub const fn height(self) -> u16 {
        match self {
            Self::Small => 1,
            Self::Large => 3,
        }
    }
}

/// The visual weight of a button, in the shadcn sense.
///
/// A variant is a shorthand for a set of theme colors, not a behavior: all
/// variants are pressed the same way. Picking one is about how much the button
/// should pull the eye and whether its action is dangerous.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum ButtonVariant {
    /// Filled with the theme's primary color. The main action on a screen.
    #[default]
    Default,
    /// Border only, no fill. A secondary action that should stay quiet.
    ///
    /// Needs [`ButtonSize::Large`]: a `Small` button is a single row with no
    /// space for a border, so at that size `Outline` paints the same cells
    /// focused, hovered, and at rest. Use [`Ghost`](Self::Ghost) for a quiet
    /// `Small` button — it fills on focus and hover.
    Outline,
    /// Filled with the muted secondary color. Less emphasis than `Default`.
    Secondary,
    /// No fill and no border until focused or hovered. The quietest option, and
    /// the one to reach for at [`ButtonSize::Small`], where
    /// [`Outline`](Self::Outline) has no edge to draw.
    Ghost,
    /// Filled with the theme's destructive color. Deleting, discarding.
    Destructive,
}

/// Every color a button can paint, for each interaction state.
///
/// Normally derived for you: [`from_theme`](Self::from_theme) turns a
/// [`Theme`] and a [`ButtonVariant`] into one of these. Build or modify one
/// directly only when a button needs colors the variants do not offer, and pass
/// it through [`Button::style`] or [`ButtonWidget::style`].
///
/// The four sets — base, focused, hovered, disabled — are computed up front, so
/// a custom style names the colors for every state. Disabled style wins first,
/// followed by hovered, focused, and finally the base style.
///
/// A state's `border` decides how that state paints: `Some` draws a border
/// around the label (at [`ButtonSize::Large`]; `Small` has no room for one),
/// `None` fills the button with its background, capped above and below at
/// `Large`. A `Color::Reset` background leaves the surface behind the button
/// untouched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonStyle {
    /// Label color at rest.
    pub foreground: Color,
    /// Fill color at rest.
    pub background: Color,
    /// Border color at rest, or `None` to fill instead.
    pub border: Option<Color>,
    /// Label color while focused.
    pub focused_foreground: Color,
    /// Fill color while focused.
    pub focused_background: Color,
    /// Border color while focused, or `None` to fill instead.
    pub focused_border: Option<Color>,
    /// Label color while the pointer is over it.
    pub hovered_foreground: Color,
    /// Fill color while hovered.
    pub hovered_background: Color,
    /// Border color while hovered, or `None` to fill instead.
    pub hovered_border: Option<Color>,
    /// Label color while disabled.
    pub disabled_foreground: Color,
    /// Fill color while disabled.
    pub disabled_background: Color,
    /// Border color while disabled, or `None` to fill instead.
    pub disabled_border: Option<Color>,
}

impl ButtonStyle {
    /// A neutral style using plain ANSI colors, for painting without a
    /// [`Theme`].
    ///
    /// Plain ANSI colors paint on any terminal, including ones without
    /// truecolor. Use [`from_theme`](Self::from_theme) whenever a theme is
    /// available.
    #[must_use]
    pub const fn fallback() -> Self {
        Self {
            foreground: Color::Gray,
            background: Color::Reset,
            border: Some(Color::Gray),
            focused_foreground: Color::Black,
            focused_background: Color::Cyan,
            focused_border: Some(Color::Cyan),
            hovered_foreground: Color::Black,
            hovered_background: Color::LightCyan,
            hovered_border: Some(Color::LightCyan),
            disabled_foreground: Color::DarkGray,
            disabled_background: Color::Reset,
            disabled_border: Some(Color::DarkGray),
        }
    }

    /// Derive the full style for `variant` from `theme`.
    ///
    /// Focus and hover colors are computed by shifting the base fill rather than
    /// being separate theme entries, so a custom theme only has to supply the
    /// base colors and every variant stays consistent with it.
    ///
    /// This is what [`Button`] and [`ButtonWidget`] call for you. Call it
    /// directly when you want a variant's colors as a starting point to tweak.
    #[must_use]
    pub fn from_theme(theme: &Theme, variant: ButtonVariant) -> Self {
        // Focus shifts a fill by a fixed amount, in the direction the theme's
        // own polarity gives it. A loud fill (primary, destructive) deepens
        // toward the end the screen sits at, so it reads as pressed; a quiet
        // one (secondary, ghost) climbs away from the screen, so it reads as
        // raised. On a light terminal both are the other way round in absolute
        // terms, which is why the direction comes from the background.
        let pressed = nearest_to(theme.background);
        let raised = away_from(theme.background);
        match variant {
            ButtonVariant::Default => Self::filled(
                theme.primary,
                theme.primary_foreground,
                dim(theme.primary, pressed, FOCUS_SHIFT),
                dim(theme.primary, pressed, HOVER_SHIFT),
                theme,
            ),
            ButtonVariant::Secondary => Self::filled(
                theme.secondary,
                theme.secondary_foreground,
                dim(theme.secondary, raised, FOCUS_SHIFT),
                dim(theme.secondary, raised, HOVER_SHIFT),
                theme,
            ),
            ButtonVariant::Destructive => Self::filled(
                theme.destructive,
                theme.destructive_foreground,
                dim(theme.destructive, pressed, FOCUS_SHIFT),
                dim(theme.destructive, pressed, HOVER_SHIFT),
                theme,
            ),
            ButtonVariant::Outline => Self {
                foreground: theme.foreground,
                background: Color::Reset,
                border: Some(theme.border),
                focused_foreground: theme.foreground,
                focused_background: Color::Reset,
                focused_border: Some(theme.primary),
                hovered_foreground: theme.foreground,
                hovered_background: Color::Reset,
                hovered_border: Some(dim(theme.primary, pressed, HOVER_SHIFT)),
                disabled_foreground: theme.muted_foreground,
                disabled_background: Color::Reset,
                disabled_border: Some(theme.border),
            },
            ButtonVariant::Ghost => {
                let (focused_background, hovered_background) =
                    ghost_fills(theme.secondary, theme.background);
                Self {
                    foreground: theme.foreground,
                    background: Color::Reset,
                    border: None,
                    focused_foreground: theme.foreground,
                    focused_background,
                    focused_border: None,
                    hovered_foreground: theme.foreground,
                    hovered_background,
                    hovered_border: None,
                    disabled_foreground: theme.muted_foreground,
                    disabled_background: Color::Reset,
                    disabled_border: None,
                }
            }
        }
    }

    const fn filled(
        background: Color,
        foreground: Color,
        focused_background: Color,
        hovered_background: Color,
        theme: &Theme,
    ) -> Self {
        // Disabled keeps the variant's hue, dimmed toward the surface, so a
        // disabled destructive button still reads as destructive.
        let disabled_background = dim(background, theme.surface, DISABLED_DIM);
        Self {
            foreground,
            background,
            border: None,
            focused_foreground: foreground,
            focused_background,
            focused_border: None,
            hovered_foreground: foreground,
            hovered_background,
            hovered_border: None,
            disabled_foreground: theme.muted_foreground,
            disabled_background,
            disabled_border: None,
        }
    }

    /// The colors and emphasis for one paint pass — the single place
    /// interaction state is turned into style. Precedence: disabled, then
    /// hovered, then focused, then base; hover wins visually when a focused
    /// button is also hovered.
    const fn resolve(&self, focused: bool, hovered: bool, disabled: bool) -> ResolvedButtonStyle {
        if disabled {
            ResolvedButtonStyle {
                foreground: self.disabled_foreground,
                background: self.disabled_background,
                border: self.disabled_border,
            }
        } else if hovered {
            ResolvedButtonStyle {
                foreground: self.hovered_foreground,
                background: self.hovered_background,
                border: self.hovered_border,
            }
        } else if focused {
            ResolvedButtonStyle {
                foreground: self.focused_foreground,
                background: self.focused_background,
                border: self.focused_border,
            }
        } else {
            ResolvedButtonStyle {
                foreground: self.foreground,
                background: self.background,
                border: self.border,
            }
        }
    }
}

/// One paint pass's resolved colors (see `ButtonStyle::resolve`). A `Some`
/// border paints bordered; `None` paints filled, with `background` as the cap
/// fill too.
struct ResolvedButtonStyle {
    foreground: Color,
    background: Color,
    border: Option<Color>,
}

impl ResolvedButtonStyle {
    /// The label style. A `Reset` background is left unset so the surface the
    /// button sits on shows through instead of the terminal default.
    fn content_style(&self) -> Style {
        let style = Style::default().fg(self.foreground);
        if self.background == Color::Reset {
            style
        } else {
            style.bg(self.background)
        }
    }
}

/// A button that only draws — an ordinary ratatui [`Widget`] with no focus,
/// events, or state.
///
/// **Usable in any ratatui app.** Nothing here depends on
/// [`Ratcn`](ratcn::runtime::Ratcn) or the component layer: hand it to
/// `frame.render_widget(...)` alongside your own widgets and keep managing focus
/// and events however you already do. Take the look without the runtime.
///
/// You tell it what to paint, including whether to draw as focused or hovered,
/// and it paints. Nothing is tracked between frames.
///
/// Use [`Button`] instead when you want a button that joins ratcn's focus
/// traversal and emits a message when pressed; it paints through this widget
/// internally, so both look identical.
///
/// ```
/// use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};
/// use ratcn::{ButtonWidget, Theme};
///
/// let area = Rect::new(0, 0, 12, 1);
/// let mut buffer = Buffer::empty(area);
/// Widget::render(
///     ButtonWidget::new("Save")
///         .themed(&Theme::default_dark())
///         .focused(true),
///     area,
///     &mut buffer,
/// );
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonWidget<'a> {
    label: &'a str,
    focused: bool,
    hovered: bool,
    variant: ButtonVariant,
    /// Colors to derive from at render, with the variant. `None` paints with
    /// [`ButtonStyle::fallback`].
    theme: Option<Theme>,
    /// Exact colors, taking precedence over `theme` and the variant.
    style: Option<ButtonStyle>,
    disabled: bool,
    size: ButtonSize,
}

impl<'a> ButtonWidget<'a> {
    /// A small, default-variant button labelled `label`, using
    /// [`ButtonStyle::fallback`] until [`themed`](Self::themed) or
    /// [`style`](Self::style) says otherwise.
    #[must_use]
    pub const fn new(label: &'a str) -> Self {
        Self {
            label,
            focused: false,
            hovered: false,
            variant: ButtonVariant::Default,
            theme: None,
            style: None,
            disabled: false,
            size: ButtonSize::Small,
        }
    }

    /// Take colors from `theme`, combined with the variant in force at render.
    #[must_use]
    pub const fn themed(mut self, theme: &Theme) -> Self {
        self.theme = Some(*theme);
        self.style = None;
        self
    }

    /// Shorthand for [`ButtonVariant::Outline`].
    #[must_use]
    pub const fn outline(self) -> Self {
        self.variant(ButtonVariant::Outline)
    }

    /// Shorthand for [`ButtonVariant::Secondary`].
    #[must_use]
    pub const fn secondary(self) -> Self {
        self.variant(ButtonVariant::Secondary)
    }

    /// Shorthand for [`ButtonVariant::Ghost`].
    #[must_use]
    pub const fn ghost(self) -> Self {
        self.variant(ButtonVariant::Ghost)
    }

    /// Shorthand for [`ButtonVariant::Destructive`].
    #[must_use]
    pub const fn destructive(self) -> Self {
        self.variant(ButtonVariant::Destructive)
    }

    /// Set the variant. See [`ButtonVariant`].
    #[must_use]
    pub const fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Use these exact colors, ignoring theme and variant.
    #[must_use]
    pub const fn style(mut self, style: ButtonStyle) -> Self {
        self.style = Some(style);
        self.theme = None;
        self
    }

    /// Paint with the focused colors. The widget has no idea what is actually
    /// focused — pass the answer in.
    #[must_use]
    pub const fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Render the button as hovered (pointer over it). Hover is distinct from
    /// focus and wins visually when both are true.
    #[must_use]
    pub const fn hovered(mut self, hovered: bool) -> Self {
        self.hovered = hovered;
        self
    }

    /// Paint with the disabled colors. Purely visual here — a paint widget
    /// receives no events to suppress.
    #[must_use]
    pub const fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Set the height, one row or three. See [`ButtonSize`].
    ///
    /// A large button needs all three rows to paint. When the supplied area is
    /// taller, only the first three rows are painted. Any nonzero width remains
    /// usable.
    #[must_use]
    pub const fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }

    /// Columns this button needs: the label in terminal cells, plus two cells
    /// of padding on each side.
    ///
    /// Use it to build layout constraints from the same instance you are about
    /// to render, so the constraint and the paint cannot disagree.
    #[must_use]
    pub fn width(&self) -> u16 {
        shape_width(self.label)
    }

    /// The colors in force: an explicit style, else the variant from the
    /// theme, else the fallback.
    fn resolved_style(&self) -> ButtonStyle {
        match (self.style, self.theme) {
            (Some(style), _) => style,
            (None, Some(theme)) => ButtonStyle::from_theme(&theme, self.variant),
            (None, None) => ButtonStyle::fallback(),
        }
    }
}

impl Widget for ButtonWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // A button is a fixed-height shape. Rendering into a shorter area
        // produces broken caps/content, so skip instead of drawing a partial
        // button. Flexible widgets can still degrade within smaller areas.
        let height = self.size.height();
        if area.width == 0 || area.height < height {
            return;
        }
        let area = Rect { height, ..area };

        let resolved = self
            .resolved_style()
            .resolve(self.focused, self.hovered, self.disabled);
        match resolved.border {
            Some(border) => self.paint_bordered(&resolved, border, area, buf),
            None => self.paint_filled(&resolved, area, buf),
        }
    }
}

impl ButtonWidget<'_> {
    fn paint_filled(self, resolved: &ResolvedButtonStyle, area: Rect, buf: &mut Buffer) {
        let width = area.width as usize;

        if self.size == ButtonSize::Large {
            Line::from(cap_row(resolved.background, TOP_CAP, width))
                .style(Style::default().fg(resolved.background))
                .render(Rect::new(area.x, area.y, area.width, 1), buf);

            Line::from(cap_row(resolved.background, BOTTOM_CAP, width))
                .style(Style::default().fg(resolved.background))
                .render(Rect::new(area.x, area.y + 2, area.width, 1), buf);
        }

        Line::from(filled_middle(self.label, width))
            .style(resolved.content_style())
            .render(
                Rect::new(area.x, area.y + self.content_y_offset(), area.width, 1),
                buf,
            );
    }

    fn paint_bordered(
        self,
        resolved: &ResolvedButtonStyle,
        border: Color,
        area: Rect,
        buf: &mut Buffer,
    ) {
        let content_style = resolved.content_style();

        if self.size == ButtonSize::Large {
            let block = Block::bordered().border_style(Style::default().fg(border));
            let inner = block.inner(area);
            block.render(area, buf);

            Line::from(self.label)
                .alignment(Alignment::Center)
                .style(content_style)
                .render(inner, buf);
            return;
        }

        Line::from(filled_middle(self.label, area.width as usize))
            .alignment(Alignment::Center)
            .style(content_style)
            .render(area, buf);
    }

    const fn content_y_offset(&self) -> u16 {
        match self.size {
            ButtonSize::Small => 0,
            ButtonSize::Large => 1,
        }
    }
}

/// Resolves the button's style from the active theme (the style override).
type StyleFn = Box<dyn Fn(&Theme) -> ButtonStyle>;
/// Builds the message emitted when the button is pressed.
type OnPressFn<M> = Box<dyn Fn() -> M>;

/// A button that can be focused and pressed, declared with
/// [`component`](ratcn::runtime::DeclareCtx::component).
///
/// After [`on_press`](Self::on_press) is set, pressing it — Enter, Space, or a
/// left click — returns the message built by that handler. Without a handler the
/// button is not focusable and ignores activation keys and clicks; use
/// [`ButtonWidget`] when only painting is needed. Right and middle clicks do
/// nothing. Painting is delegated to `ButtonWidget`; this half adds focus and
/// event handling.
///
/// A button holds no state of its own. Its label and disabledness are values you
/// pass at declaration, which means they come from app state and there is
/// nothing to keep in sync. Those declared values also stay in effect for event
/// handling until the next successful render, so a click arriving between an
/// update and a redraw is judged against the button the user actually saw.
///
/// ```
/// use ratcn::Button;
///
/// # struct AppState { can_delete: bool }
/// # enum Msg { Delete }
/// # let state = AppState { can_delete: true };
/// let _button = Button::new("Delete")
///     .destructive()
///     .disabled(!state.can_delete)
///     .on_press(|| Msg::Delete);
/// ```
pub struct Button<M> {
    label: String,
    variant: ButtonVariant,
    size: ButtonSize,
    style: Option<StyleFn>,
    on_press: Option<OnPressFn<M>>,
    disabled: bool,
}

impl<M> fmt::Debug for Button<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Button")
            .field("label", &self.label)
            .field("variant", &self.variant)
            .field("size", &self.size)
            .field("style", &self.style.is_some())
            .field("on_press", &self.on_press.is_some())
            .field("disabled", &self.disabled)
            .finish_non_exhaustive()
    }
}

impl<M> Button<M> {
    /// A default-variant button labelled `label`.
    ///
    /// It does nothing until [`on_press`](Self::on_press) gives it a message to
    /// emit. Without one it is not focusable and ignores activation keys and
    /// clicks. Use [`ButtonWidget`] instead for paint-only presentation.
    #[must_use]
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            variant: ButtonVariant::Default,
            size: ButtonSize::Small,
            style: None,
            on_press: None,
            disabled: false,
        }
    }

    /// What to emit when the button is pressed: Enter, Space, or a left click.
    #[must_use]
    pub fn on_press(mut self, on_press: impl Fn() -> M + 'static) -> Self {
        self.on_press = Some(Box::new(on_press));
        self
    }

    /// Grey the button out and stop it responding.
    ///
    /// A disabled button is not focusable, so Tab skips it, and it ignores
    /// events rather than consuming them. Focus already parked on it stays
    /// there. Pass the value from app state (`.disabled(!state.can_save)`).
    #[must_use]
    pub const fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Set the visual variant. See [`ButtonVariant`].
    #[must_use]
    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Shorthand for [`ButtonVariant::Outline`].
    #[must_use]
    pub fn outline(self) -> Self {
        self.variant(ButtonVariant::Outline)
    }

    /// Shorthand for [`ButtonVariant::Secondary`].
    #[must_use]
    pub fn secondary(self) -> Self {
        self.variant(ButtonVariant::Secondary)
    }

    /// Shorthand for [`ButtonVariant::Ghost`].
    #[must_use]
    pub fn ghost(self) -> Self {
        self.variant(ButtonVariant::Ghost)
    }

    /// Shorthand for [`ButtonVariant::Destructive`].
    #[must_use]
    pub fn destructive(self) -> Self {
        self.variant(ButtonVariant::Destructive)
    }

    /// Set the height, one row or three. See [`ButtonSize`].
    #[must_use]
    pub const fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }

    /// Replace the [`ButtonStyle`] the theme and [`variant`](Button::variant)
    /// derive. Resolved from the active theme at render time, so a style built
    /// from `theme` follows theme switches; a fixed style ignores the argument
    /// (`|_| STYLE`).
    ///
    /// ```
    /// use ratcn::{Button, ButtonStyle, ButtonVariant};
    ///
    /// # enum Msg { Archive }
    /// let _button = Button::new("Archive").on_press(|| Msg::Archive).style(|theme| {
    ///     let mut style = ButtonStyle::from_theme(theme, ButtonVariant::Default);
    ///     style.background = theme.accent;
    ///     style
    /// });
    /// ```
    #[must_use]
    pub fn style(mut self, style: impl Fn(&Theme) -> ButtonStyle + 'static) -> Self {
        self.style = Some(Box::new(style));
        self
    }

    /// Columns this button needs: the label in terminal cells, plus two cells
    /// of padding on each side.
    ///
    /// Build layout constraints from the same instance you are about to
    /// declare, so the constraint and the paint cannot disagree.
    #[must_use]
    pub fn width(&self) -> u16 {
        shape_width(&self.label)
    }
}

impl<S, M> Component<S, M> for Button<M> {
    fn declare(&mut self, _ctx: &mut DeclareCtx<'_, S, M>) {}

    fn paint(&mut self, ctx: &mut PaintCtx<'_, S>) {
        let area = ctx.area();
        let style = resolve_style(self.style.as_deref(), ctx.theme, |theme| {
            ButtonStyle::from_theme(theme, self.variant)
        });
        let widget = ButtonWidget::new(&self.label)
            .style(style)
            .size(self.size)
            .focused(ctx.focused())
            .hovered(ctx.hovered())
            .disabled(self.disabled);
        ctx.widget(widget, area);
    }

    fn handle_event(
        &mut self,
        event: &Event,
        _state: &S,
        _ctx: &mut EventCtx<'_>,
    ) -> EventResult<M> {
        if self.disabled {
            return EventResult::Ignored;
        }
        let Some(on_press) = &self.on_press else {
            return EventResult::Ignored;
        };

        // A press is Enter/Space (unmodified) or a primary mouse Click. Focus on
        // Down is the runtime's job, so the button ignores Down/Up itself.
        let pressed = match event {
            Event::Key(key) if !key.modifiers.any() => {
                matches!(key.code, KeyCode::Enter | KeyCode::Char(' '))
            }
            Event::Mouse(mouse) => {
                matches!(mouse.kind, MouseKind::Click(MouseButton::Left))
            }
            _ => false,
        };
        if !pressed {
            return EventResult::Ignored;
        }
        EventResult::Emit(on_press())
    }

    fn scope_options(&self) -> ScopeOptions {
        ScopeOptions::default().focusable(!self.disabled && self.on_press.is_some())
    }

    fn interaction_area(&self, area: Rect) -> Rect {
        fixed_height(area, self.size.height())
    }
}

impl<S, M> MeasuredComponent<S, M> for Button<M> {
    fn measure(&self) -> Size {
        Size::new(self.width(), self.size.height())
    }
}
