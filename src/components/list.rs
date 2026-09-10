// Copied from ratcn 0.0.3: src/components/list.rs

use std::fmt;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Span, Text},
    widgets::Widget,
};

use ratcn::Theme;
use ratcn::color::{
    DISABLED_DIM, FIELD_FOCUS_SHIFT, FIELD_HOVER_SHIFT, ROW_FOCUS_SHIFT, away_from, blendable, dim,
};
use ratcn::linear_nav::{self, Axis};
use ratcn::list_core::{
    self, KeyIntent, ListItem, ListItemState, RowIntent, RowViewport, WheelHold,
};
use ratcn::runtime::{
    Component, DeclareCtx, Event, EventCtx, EventResult, KeyEvent, MouseKind, PaintCtx,
    ScopeOptions, ScrollDirection,
};
use ratcn::selection_indicator;
use ratcn::text_width::display_width;
use ratcn::theme::resolve_style;

/// Every color a list can paint.
///
/// Two meanings of "focused" appear in these names:
///
/// - **The list itself has focus** — the whole widget is the active control.
///   That picks the list's own backdrop: `focused_background` instead of
///   `background`.
/// - **The list is hovered** — `hovered_background` replaces either backdrop,
///   so moving the pointer remains visible even when the list already has focus.
/// - **A row is the focused row** — the cursor is on it. That styles one row:
///   `focused_foreground` on `focused_row_background`.
/// - **The list is disabled** — `disabled_background` replaces all three
///   backdrops, whatever the focus and hover state.
///
/// Rows are then colored by two independent facts, whether the cursor is on the
/// row and whether the row is selected, giving four combinations. Disabled
/// overrides all of them. Note that a row only counts as focused while the
/// cursor is shown — the list has focus or is hovered — so a list that is
/// neither leaves the cursor row painted as an ordinary (or selected) row
/// rather than as a highlight.
///
/// [`from_theme`](Self::from_theme) derives all of this from a [`Theme`]; build
/// one by hand only for colors the theme cannot express, and pass it via
/// [`List::style`] or [`ListWidget::style`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListStyle {
    /// Text color of an ordinary row.
    pub foreground: Color,
    /// Backdrop of the list while it does not have focus. Disabled wins over
    /// all three backdrops: a disabled list uses
    /// [`disabled_background`](Self::disabled_background) throughout.
    pub background: Color,
    /// Backdrop of the list while it has focus.
    pub focused_background: Color,
    /// Backdrop of the list while it is hovered. Hover wins over focus.
    pub hovered_background: Color,
    /// Text color of a selected row the cursor is not on. Selection has no fill
    /// of its own; the row keeps the list's backdrop.
    pub selected_foreground: Color,
    /// Text color of the cursor row when it is not selected.
    pub focused_foreground: Color,
    /// Fill behind the cursor row when it is not selected.
    pub focused_row_background: Color,
    /// Text color of the cursor row when it is also selected.
    pub selected_focused_foreground: Color,
    /// Fill behind the cursor row when it is also selected.
    pub selected_focused_background: Color,
    /// The `●`/`■` marker on a selected row, in the default row rendering.
    pub selected_marker: Color,
    /// The `○`/`□` marker on an unselected row, in the default row rendering.
    pub unselected_marker: Color,
    /// Text color of a disabled row, and of its marker.
    pub disabled_foreground: Color,
    /// Fill behind a disabled row.
    pub disabled_background: Color,
}

impl ListStyle {
    /// A neutral style using plain ANSI colors, for painting without a
    /// [`Theme`]. Prefer [`from_theme`](Self::from_theme) when one is available.
    #[must_use]
    pub const fn fallback() -> Self {
        Self {
            foreground: Color::Reset,
            background: Color::Reset,
            focused_background: Color::Reset,
            hovered_background: Color::DarkGray,
            selected_foreground: Color::Black,
            focused_foreground: Color::Reset,
            focused_row_background: Color::Cyan,
            selected_focused_foreground: Color::Black,
            selected_focused_background: Color::Cyan,
            selected_marker: Color::LightGreen,
            unselected_marker: Color::DarkGray,
            disabled_foreground: Color::DarkGray,
            disabled_background: Color::Reset,
        }
    }

    /// Derive every list color from `theme`.
    ///
    /// The focus and cursor-row fills are shifted from the theme's field color
    /// rather than being separate theme entries, so a custom theme only
    /// supplies the base colors and the list stays consistent with the rest of
    /// the UI. Which way they shift comes from the theme's background: away
    /// from it, so a light theme's well deepens where a dark theme's
    /// brightens.
    ///
    /// The cursor row takes the theme's ordinary `foreground`, not its muted
    /// one: it is the fill furthest from the background, so muted text is where
    /// the list's contrast is thinnest, and the row the cursor is on is the row
    /// least in need of de-emphasis. `Select` derives its focused option the
    /// same way.
    ///
    /// Disabled dims toward the *background*, both the fill and the text. A
    /// well sits one tone off the background by design, and the surface sits
    /// between the two, so dimming a well toward the surface — which is what a
    /// button does — would move it almost nowhere and leave a disabled list
    /// looking enabled. A theme that leaves its background to the terminal
    /// dims toward its surface instead: [`Color::Reset`] carries no channels to
    /// blend, and blending toward it would leave the well where it was.
    #[must_use]
    pub fn from_theme(theme: &Theme) -> Self {
        let backdrop = blendable(theme.background, theme.surface);
        let away = away_from(theme.background);
        let focused_background = dim(theme.field, away, FIELD_FOCUS_SHIFT);
        let hovered_background = dim(theme.field, away, FIELD_HOVER_SHIFT);
        let focused_row_background = dim(focused_background, away, ROW_FOCUS_SHIFT);
        Self {
            foreground: theme.muted_foreground,
            background: theme.field,
            focused_background,
            hovered_background,
            selected_foreground: theme.foreground,
            focused_foreground: theme.foreground,
            focused_row_background,
            selected_focused_foreground: theme.foreground,
            selected_focused_background: focused_row_background,
            selected_marker: theme.primary,
            unselected_marker: theme.muted_foreground,
            disabled_foreground: dim(theme.muted_foreground, backdrop, DISABLED_DIM),
            disabled_background: dim(theme.field, backdrop, DISABLED_DIM),
        }
    }

    const fn resolve_surface(&self, focused: bool, hovered: bool, disabled: bool) -> Color {
        if disabled {
            // The same backdrop the rows get, so a list shorter than its area
            // does not show a seam past its last item — and the same answer
            // `SelectStyle` gives for a disabled trigger.
            self.disabled_background
        } else if hovered {
            self.hovered_background
        } else if focused {
            self.focused_background
        } else {
            self.background
        }
    }

    fn resolve_row(
        &self,
        focused: bool,
        selected: bool,
        disabled: bool,
        background: Color,
    ) -> Style {
        if disabled {
            return Style::default()
                .fg(self.disabled_foreground)
                .bg(self.disabled_background);
        }
        let (foreground, background) = match (focused, selected) {
            (true, true) => (
                self.selected_focused_foreground,
                self.selected_focused_background,
            ),
            (true, false) => (self.focused_foreground, self.focused_row_background),
            // Selection has no fill: keep whichever backdrop the list has.
            (false, true) => (self.selected_foreground, background),
            (false, false) => (self.foreground, background),
        };
        Style::default().fg(foreground).bg(background)
    }
}

/// A list that only draws — an ordinary ratatui [`Widget`] with no focus,
/// events, or state.
///
/// **Usable in any ratatui app.** Nothing here depends on
/// [`Ratcn`](ratcn::runtime::Ratcn) or the component layer: render it directly
/// and keep driving selection and scrolling however you already do.
///
/// Rows are pre-rendered [`Text`]s, each free to span several lines, and
/// everything else is addressed by *item index*: which item the cursor is on,
/// which are selected, which are disabled.
/// Explicit colors in the supplied text are preserved; row styles provide the
/// colors for text that does not set its own.
/// That makes it easy to drive from any data you like, but it also means the
/// widget has no idea what a row means — reorder your data and the indices refer
/// to different things.
///
/// Use [`List`] instead when you want that handled: it keys focus and selection
/// by a value you choose, and adds keyboard and mouse handling. It paints
/// through this widget internally.
///
/// Scrolling is the caller's: hand over the rows that are on screen and say
/// where they start with [`first_item`](Self::first_item). The widget holds no
/// scroll position and never adjusts one, so the scroll policy stays with
/// whatever owns it — your app, or the [`List`] component — and you build
/// [`Text`]s for the rows you hand over.
///
/// # Sizing and row heights
///
/// There is no measurement method, because there is nothing to measure: the
/// widget is area-driven. It paints into the area it is given, top to bottom,
/// stopping when the area runs out. How much room a list deserves is a layout
/// question the caller answers with a ratatui `Constraint`, not something the
/// list can answer from its items.
///
/// Rows may be any height — each item is a [`Text`], so one item may be one
/// line and the next three, and the widget paints each at its own height.
/// Keeping them uniform is the caller's job whenever anything maps a screen row
/// back to an item, because the arithmetic that does so counts *items*, not
/// lines: with mixed heights, the row a click lands on names a different item.
/// The [`List`] component takes that job on — it normalizes every item to its
/// [`row_height`](List::row_height) with
/// [`fit_to_height`](ratcn::list_core::fit_to_height), padding short rows and
/// truncating tall ones — which is why clicking, paging, and wheel scrolling
/// are exact there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListWidget<'a> {
    items: &'a [Text<'static>],
    first_item: usize,
    focused_item: Option<usize>,
    selected_items: &'a [usize],
    disabled_items: &'a [bool],
    focused: bool,
    hovered: bool,
    disabled: bool,
    style: ListStyle,
    focus_symbol: &'a str,
}

impl<'a> ListWidget<'a> {
    /// The rows to paint, top to bottom, nothing focused or selected, using
    /// [`ListStyle::fallback`].
    ///
    /// These are the rows on screen, not a whole list to scroll through:
    /// a scrolled caller passes the window and names its position with
    /// [`first_item`](Self::first_item).
    #[must_use]
    pub const fn new(items: &'a [Text<'static>]) -> Self {
        Self {
            items,
            first_item: 0,
            focused_item: None,
            selected_items: &[],
            disabled_items: &[],
            focused: false,
            hovered: false,
            disabled: false,
            style: ListStyle::fallback(),
            focus_symbol: "",
        }
    }

    /// Take colors from `theme`.
    #[must_use]
    pub fn themed(mut self, theme: &Theme) -> Self {
        self.style = ListStyle::from_theme(theme);
        self
    }

    /// Use these exact colors, ignoring any theme.
    #[must_use]
    pub const fn style(mut self, style: ListStyle) -> Self {
        self.style = style;
        self
    }

    /// The index the first row given to [`new`](Self::new) has in the whole
    /// list. Defaults to 0.
    ///
    /// It is what lines the painted window up with the index-addressed state:
    /// [`focused_item`](Self::focused_item), [`selected_items`](Self::selected_items),
    /// and [`disabled_items`](Self::disabled_items) all count from the start of
    /// the list, not from the top of the window, so scrolling changes only this
    /// number and the rows. The widget never adjusts it — keep-visible and
    /// clamping policy belong to the caller (see
    /// [`linear_nav`](ratcn::linear_nav) for the arithmetic [`List`] uses).
    #[must_use]
    pub const fn first_item(mut self, first_item: usize) -> Self {
        self.first_item = first_item;
        self
    }

    /// Which row the cursor is on, by index.
    ///
    /// Only highlighted while the list is also [`focused`](Self::focused) — an
    /// unfocused list shows no cursor, so two lists side by side cannot both
    /// look active.
    #[must_use]
    pub const fn focused_item(mut self, focused_item: Option<usize>) -> Self {
        self.focused_item = focused_item;
        self
    }

    /// Indices of the selected rows, counting from the start of the list. Pass
    /// one index for single selection, or several for multi-selection; the
    /// widget does not care which you mean. Only rows it paints are consulted,
    /// so a windowed caller need only name the selected rows in its window.
    ///
    /// This is an index list, not a mask: selection is sparse — usually zero or
    /// one row, any number under multi-selection — so you name the selected
    /// rows rather than flag every row. Contrast
    /// [`disabled_items`](Self::disabled_items), which describes every row and is
    /// therefore a positional mask.
    #[must_use]
    pub const fn selected_items(mut self, selected_items: &'a [usize]) -> Self {
        self.selected_items = selected_items;
        self
    }

    /// A disabled flag per item, counting from the start of the list. Entries
    /// past the end of the slice read as enabled, so a short slice is fine.
    ///
    /// This is a positional mask, not an index list: disabledness is a property
    /// of every row, usually derived straight from the items, so one flag per
    /// item lines up without a lookup. It matches
    /// [`TabsWidget::disabled_items`](ratcn::TabsWidget::disabled_items), while
    /// sparse [`selected_items`](Self::selected_items) stays an index list.
    #[must_use]
    pub const fn disabled_items(mut self, disabled_items: &'a [bool]) -> Self {
        self.disabled_items = disabled_items;
        self
    }

    /// Paint as the focused control: the focus backdrop, the cursor row
    /// highlighted, and the focus symbol shown. This is "the cursor is shown"
    /// rather than keyboard focus alone — [`List`] passes focused *or*
    /// hovered, and lets [`hovered`](Self::hovered) pick the backdrop.
    #[must_use]
    pub const fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Paint the list's hovered backdrop. Hover wins when the list is also
    /// focused, while cursor-row visibility remains controlled by
    /// [`focused`](Self::focused).
    #[must_use]
    pub const fn hovered(mut self, hovered: bool) -> Self {
        self.hovered = hovered;
        self
    }

    /// Paint the whole list as disabled, overriding per-row styling.
    #[must_use]
    pub const fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// A marker drawn in front of the cursor row, such as `"> "`. Empty by
    /// default. Only shown while the list is focused and enabled.
    ///
    /// It occupies columns to the left of every row, so a wide symbol narrows
    /// the space available for labels.
    #[must_use]
    pub const fn focus_symbol(mut self, focus_symbol: &'a str) -> Self {
        self.focus_symbol = focus_symbol;
        self
    }
}

impl Widget for ListWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let area = area.intersection(buf.area);
        if area.width == 0 || area.height == 0 {
            return;
        }
        // The whole widget area first, so rows past the last item keep the
        // list backdrop.
        buf.set_style(
            area,
            Style::default()
                .fg(self.style.foreground)
                .bg(self
                    .style
                    .resolve_surface(self.focused, self.hovered, self.disabled)),
        );
        // The focus symbol occupies a column in front of every row, reserved
        // only while there is a cursor to point at — the same frames the
        // cursor row is highlighted.
        let cursor_shown = self.focused && !self.disabled && self.focused_item.is_some();
        let symbol_width = if cursor_shown {
            u16::try_from(display_width(self.focus_symbol)).unwrap_or(u16::MAX)
        } else {
            0
        };
        let text_x = area.x.saturating_add(symbol_width).min(area.right());
        let text_width = area.width.saturating_sub(symbol_width);

        let mut y = area.y;
        for (row, item) in self.items.iter().enumerate() {
            if y >= area.bottom() {
                break;
            }
            let index = self.first_item.saturating_add(row);
            let height = u16::try_from(item.lines.len().max(1))
                .unwrap_or(u16::MAX)
                .min(area.bottom() - y);
            let row_area = Rect::new(area.x, y, area.width, height);
            // The row's colors fill its full width; the item's own spans then
            // patch over them, so explicit colors in the text are preserved
            // and everything else inherits the row style.
            buf.set_style(row_area, self.row_style(index));
            if cursor_shown && self.focused_item == Some(index) {
                Span::raw(self.focus_symbol).render(row_area, buf);
            }
            item.render(Rect::new(text_x, y, text_width, height), buf);
            y = y.saturating_add(height);
        }
    }
}

impl ListWidget<'_> {
    fn row_style(&self, index: usize) -> Style {
        self.style.resolve_row(
            self.focused && self.focused_item == Some(index),
            self.selected_items.contains(&index),
            self.disabled || self.disabled_items.get(index).copied().unwrap_or(false),
            self.style
                .resolve_surface(self.focused, self.hovered, self.disabled),
        )
    }
}

type ReadFn<S, T> = Box<dyn Fn(&S) -> Option<T>>;
type MultiSelectionFn<S, T> = Box<dyn Fn(&S, &T) -> bool>;
type OnChangeFn<T, M> = Box<dyn Fn(T) -> M>;
type OnFocusChangeFn<T, M> = Box<dyn Fn(T, usize) -> M>;
type ScrollFn<S> = Box<dyn Fn(&S) -> usize>;
type PaintItemFn<S, T> = Box<dyn for<'a> Fn(&S, ListItemState<'a, T>) -> Text<'static>>;
type StyleFn = Box<dyn Fn(&Theme) -> ListStyle>;

/// A scrollable list of items, declared with
/// [`component`](ratcn::runtime::DeclareCtx::component).
///
/// # Cursor and selection are different things
///
/// The **cursor** (called *item focus* here) is where the user is looking. Arrow
/// keys, Home, End, and Page keys move it, and moving it is not a choice —
/// nothing is committed. **Selection** is the choice: Enter, Space, or a left
/// click commits the row under the cursor.
///
/// Keeping them apart is what lets a user browse a list without changing
/// anything, which is why they are two separate bindings rather than one
/// "current item".
///
/// # Bindings
///
/// Each binding is a pair — a reader for the current value and a constructor for
/// the message that changes it — passed in one call so a reader and writer
/// pointing at different fields cannot drift apart. Everything is keyed by your
/// item value, never by row index, so filtering or reordering the list keeps the
/// same item current.
///
/// Item values must be unique within each declaration, or focus, selection, and
/// pointer actions are ambiguous. A debug build panics on duplicates during
/// declaration.
///
/// - [`item_focus`](Self::item_focus) — the cursor. Without it the list is
///   paint- and pointer-only, is not a keyboard focus stop, and ignores keys.
/// - [`selection`](Self::selection) — single selection.
/// - [`multi_selection`](Self::multi_selection) — checkbox-style selection.
///   Mutually exclusive with `selection`.
/// - [`scroll`](Self::scroll) — the scroll offset, if the app wants to own it.
///
/// Disabled items are dimmed and skipped by both keyboard and mouse.
///
/// ```
/// use ratcn::{List, ListItem};
///
/// # #[derive(Clone, Copy, PartialEq)]
/// # struct TaskId(u64);
/// # struct AppState { focused_task: Option<TaskId>, selected_task: Option<TaskId> }
/// # enum Msg { TaskFocused(TaskId, usize), TaskSelected(TaskId) }
/// let _list = List::new([
///     ListItem::new(TaskId(7), "Write spec"),
///     ListItem::new(TaskId(3), "Ship it").disabled(true),
/// ])
/// .item_focus(|s: &AppState| s.focused_task, Msg::TaskFocused)
/// .selection(|s: &AppState| s.selected_task, Msg::TaskSelected);
/// ```
pub struct List<T, S, M> {
    items: Vec<ListItem<T>>,
    focused_item: Option<ReadFn<S, T>>,
    on_focus_change: Option<OnFocusChangeFn<T, M>>,
    selected: Option<ReadFn<S, T>>,
    on_select: Option<OnChangeFn<T, M>>,
    selected_many: Option<MultiSelectionFn<S, T>>,
    on_toggle: Option<OnChangeFn<T, M>>,
    scroll: Option<ScrollFn<S>>,
    on_scroll_change: Option<Box<dyn Fn(usize) -> M>>,
    disabled: bool,
    paint_item: Option<PaintItemFn<S, T>>,
    style: Option<StyleFn>,
    focus_symbol: String,
    /// The markers selection rows show, overriding the radio/checkbox defaults.
    selected_marker: Option<String>,
    unselected_marker: Option<String>,
    /// Row height plus the painted offset — render-derived runtime state kept
    /// so hit-testing and wheel arithmetic work against what is on screen,
    /// never a second copy of app-owned scroll.
    viewport: RowViewport,
}

impl<T: fmt::Debug, S, M> fmt::Debug for List<T, S, M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("List")
            .field("items", &self.items)
            .field("item_focus", &self.focused_item.is_some())
            .field("selection", &self.selected.is_some())
            .field("multi_selection", &self.selected_many.is_some())
            .field("style", &self.style.is_some())
            .field("disabled", &self.disabled)
            .finish_non_exhaustive()
    }
}

impl<T, S, M> List<T, S, M> {
    /// A list of `items`, with no bindings yet.
    ///
    /// Accepts anything that converts into a [`ListItem`], so `["one", "two"]`
    /// works for a quick list of strings and
    /// `[ListItem::new(id, label), ...]` for anything keyed by a real value.
    #[must_use]
    pub fn new(items: impl IntoIterator<Item = impl Into<ListItem<T>>>) -> Self {
        Self {
            items: items.into_iter().map(Into::into).collect(),
            focused_item: None,
            on_focus_change: None,
            selected: None,
            on_select: None,
            selected_many: None,
            on_toggle: None,
            scroll: None,
            on_scroll_change: None,
            disabled: false,
            paint_item: None,
            style: None,
            focus_symbol: String::new(),
            selected_marker: None,
            unselected_marker: None,
            viewport: RowViewport::new(1),
        }
    }

    /// Bind the cursor: where to read it from, and what message moves it.
    ///
    /// `read` returns the value of the item the cursor is on, or `None` when it
    /// is nowhere yet — in which case the first arrow key moves it onto the
    /// first enabled item. `on_change` is called with the value moved to and the
    /// resulting top-item scroll offset. Store both in one update when
    /// [`scroll`](Self::scroll) is bound; an unbound list can ignore the offset.
    ///
    /// Moving the cursor commits nothing; see [`selection`](Self::selection).
    /// Without this binding the list remains available for painting, bound
    /// scrolling, and pointer selection, but is not focusable and does not
    /// consume keyboard navigation or Enter.
    #[must_use]
    pub fn item_focus(
        mut self,
        read: impl Fn(&S) -> Option<T> + 'static,
        on_change: impl Fn(T, usize) -> M + 'static,
    ) -> Self {
        self.focused_item = Some(Box::new(read));
        self.on_focus_change = Some(Box::new(on_change));
        self
    }

    /// Bind single selection: at most one item chosen at a time.
    ///
    /// `read` returns the selected value, or `None` for nothing selected.
    /// `on_select` is called with the item the user committed — Enter or Space
    /// on the cursor row, or a left click on a row.
    ///
    /// A pointer click can commit a row without a preceding pointer-move event.
    /// When [`item_focus`](Self::item_focus) is also bound, the update handling
    /// this message should store the committed value as both selection and item
    /// focus so later keyboard input continues from the clicked row.
    ///
    /// Selecting is an action, not a movement, which is why this is `on_select`
    /// and not `on_selection_change`.
    ///
    /// # Panics
    ///
    /// Rendering panics if this is combined with
    /// [`multi_selection`](Self::multi_selection): a list is one mode or the
    /// other, and Enter commits through whichever one is bound.
    #[must_use]
    pub fn selection(
        mut self,
        read: impl Fn(&S) -> Option<T> + 'static,
        on_select: impl Fn(T) -> M + 'static,
    ) -> Self {
        self.selected = Some(Box::new(read));
        self.on_select = Some(Box::new(on_select));
        self
    }

    /// Bind multi-selection: any number of items chosen, checkbox style.
    ///
    /// `read` is a predicate asked "is this one selected?" for each row the list
    /// draws, rather than a function returning a collection. That way your app
    /// can store the selection however it likes — a `HashSet`, a flag on each
    /// record, a computed rule — without converting it every frame, and a list
    /// scrolled to fifteen of a thousand rows asks fifteen times. `on_toggle`
    /// is called with the item the user flipped; your update function decides
    /// whether that means adding or removing.
    ///
    /// When [`item_focus`](Self::item_focus) is also bound, the update handling
    /// a pointer toggle should store this value as item focus as well. A click
    /// need not be preceded by a pointer-move event.
    ///
    /// Switches the default row markers from `●`/`○` to `■`/`□`.
    ///
    /// # Panics
    ///
    /// Rendering panics if this is combined with
    /// [`selection`](Self::selection).
    #[must_use]
    pub fn multi_selection(
        mut self,
        read: impl Fn(&S, &T) -> bool + 'static,
        on_toggle: impl Fn(T) -> M + 'static,
    ) -> Self {
        self.selected_many = Some(Box::new(read));
        self.on_toggle = Some(Box::new(on_toggle));
        self
    }

    /// Bind the requested scroll offset — the index of the topmost visible item.
    ///
    /// Before paint, the list adjusts the requested value to keep the focused
    /// item visible. Focus movement computes the same resulting offset from the
    /// current app value and passes it to the [`item_focus`](Self::item_focus)
    /// message constructor, so its update can persist cursor and scroll
    /// atomically. Hit-testing and wheel input use the actual painted offset.
    ///
    /// The wheel is the exception to keeping the cursor visible: it scrolls
    /// the view and leaves the cursor where it is, so the cursor may scroll
    /// out of sight — the behavior a scrollable list has everywhere else. It
    /// emits the offset one notch away from the painted one, and consumes the
    /// event without emitting when the app already holds that value. Paint
    /// resumes scrolling the cursor into view as soon as anything moves — the
    /// cursor, or the items under it; see
    /// [`WheelHold`](ratcn::list_core::WheelHold) for exactly when the wheeled
    /// view stops being honored. Render-driven adjustment itself cannot emit a
    /// message.
    ///
    /// Only needed when something outside the list has to know or change where
    /// it is scrolled to, such as a scrollbar drawn alongside it. Left unbound,
    /// the list owns the offset itself — keyboard movement and the wheel both
    /// still scroll it, exactly as they do when bound; there is simply no
    /// message and no app-held value.
    #[must_use]
    pub fn scroll(
        mut self,
        read: impl Fn(&S) -> usize + 'static,
        on_change: impl Fn(usize) -> M + 'static,
    ) -> Self {
        self.scroll = Some(Box::new(read));
        self.on_scroll_change = Some(Box::new(on_change));
        self
    }

    /// Draw each row yourself instead of using the default marker-and-label
    /// line.
    ///
    /// The closure gets app state and a [`ListItemState`] describing the row,
    /// and returns what to paint. Use it for columns, secondary text, per-row
    /// icons — anything the default cannot express. The resolved [`ListStyle`]
    /// is painted beneath the returned text, so unstyled text inherits row-state
    /// colors while explicit `Text`, `Line`, and `Span` colors are preserved.
    ///
    /// Return a [`Line`](ratatui::text::Line) for the usual one-line row, or a
    /// [`Text`] for a taller one — a name above a subtitle, say. A multi-line
    /// row also needs [`row_height`](Self::row_height) set to match, since every
    /// item must be the same height for clicks to land on the right one.
    #[must_use]
    pub fn paint_item<R: Into<Text<'static>>>(
        mut self,
        f: impl for<'a> Fn(&S, ListItemState<'a, T>) -> R + 'static,
    ) -> Self {
        self.paint_item = Some(Box::new(move |state, row| f(state, row).into()));
        self
    }

    /// How many terminal rows each item occupies. Defaults to 1.
    ///
    /// Raise it when [`paint_item`](Self::paint_item) returns more than one
    /// line — a name above a subtitle, say. Every item gets the same height,
    /// which is what keeps clicking, paging, and scrolling exact: a returned
    /// [`Text`] is padded with blank lines or truncated to fit, so the row the
    /// user clicks is always the item the runtime thinks it is.
    ///
    /// A height of 0 is treated as 1.
    #[must_use]
    pub const fn row_height(mut self, rows: u16) -> Self {
        self.viewport = RowViewport::new(rows);
        self
    }

    /// A marker drawn in front of the cursor row, such as `"> "`. Empty by
    /// default, and only shown while the list has keyboard focus or hover.
    #[must_use]
    pub fn focus_symbol(mut self, symbol: impl Into<String>) -> Self {
        self.focus_symbol = symbol.into();
        self
    }

    /// The marker shown on a selected row, instead of the shape selection
    /// picks: `●` for a pick-one list, `■` for a multi-select.
    ///
    /// Any string works, including multi-character pairs like `[x]`; the row
    /// indents the label by the marker's width plus one space. Pair it with
    /// [`unselected_marker`](Self::unselected_marker) so both states read as
    /// one style of control — `[x]`/`[ ]` turns a multi-select into an ASCII
    /// checklist.
    #[must_use]
    pub fn selected_marker(mut self, marker: impl Into<String>) -> Self {
        self.selected_marker = Some(marker.into());
        self
    }

    /// The marker shown on an unselected row, instead of `○` or `□`.
    #[must_use]
    pub fn unselected_marker(mut self, marker: impl Into<String>) -> Self {
        self.unselected_marker = Some(marker.into());
        self
    }

    /// Replace the theme-derived [`ListStyle`].
    ///
    /// The closure receives the active theme each render, so a style built from
    /// its argument follows theme switches. Ignore the argument (`|_| STYLE`)
    /// for colors that should stay fixed.
    #[must_use]
    pub fn style(mut self, style: impl Fn(&Theme) -> ListStyle + 'static) -> Self {
        self.style = Some(Box::new(style));
        self
    }

    /// Dim the whole list and stop it responding.
    ///
    /// A disabled list is not focusable, so Tab skips it — as does a list that
    /// is empty or has every row disabled, since there would be nothing for the
    /// cursor to land on. Disable individual rows with [`ListItem::disabled`].
    #[must_use]
    pub const fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl<T: Clone + PartialEq + 'static, S, M> List<T, S, M> {
    fn disabled_at(&self, index: usize) -> bool {
        list_core::disabled_at(&self.items, index)
    }

    fn focused_index(&self, state: &S) -> Option<usize> {
        list_core::index_of(&self.items, &(self.focused_item.as_ref()?)(state)?)
    }

    fn selected_index(&self, state: &S) -> Option<usize> {
        list_core::index_of(&self.items, &(self.selected.as_ref()?)(state)?)
    }

    fn move_focus(&self, index: usize, state: &S, area: Rect) -> EventResult<M> {
        match &self.on_focus_change {
            Some(on_change) => EventResult::Emit(on_change(
                self.items[index].value().clone(),
                self.offset_for_focus(state, area, index),
            )),
            None => EventResult::Ignored,
        }
    }

    fn offset_for_focus(&self, state: &S, area: Rect, index: usize) -> usize {
        let current = self
            .scroll
            .as_ref()
            .map_or(self.viewport.painted_offset(), |read| read(state));
        self.viewport
            .cursor_visible_offset(area, self.items.len(), current, Some(index))
    }

    fn select(&self, index: usize) -> EventResult<M> {
        if let Some(on_select) = &self.on_select {
            return EventResult::Emit(on_select(self.items[index].value().clone()));
        }
        if let Some(on_toggle) = &self.on_toggle {
            return EventResult::Emit(on_toggle(self.items[index].value().clone()));
        }
        EventResult::Ignored
    }

    fn scroll_view(
        &mut self,
        direction: ScrollDirection,
        state: &S,
        area: Rect,
        ctx: &mut EventCtx<'_>,
    ) -> EventResult<M> {
        let painted = self.viewport.painted_offset();
        // The wheel moves the view, never the cursor, and holds it there on
        // the list's identity — which outlives this instance, and is what
        // lets an unbound list scroll at all.
        let cursor = self.focused_index(state);
        let Some(next) = self
            .viewport
            .wheel(direction, &self.items, cursor, area, ctx)
        else {
            return EventResult::Ignored;
        };
        let Some(on_change) = &self.on_scroll_change else {
            return EventResult::Consumed;
        };
        let current = self.scroll.as_ref().map_or(painted, |read| read(state));
        if current == next {
            EventResult::Consumed
        } else {
            EventResult::Emit(on_change(next))
        }
    }

    fn handle_key(&self, key: KeyEvent, state: &S, area: Rect) -> EventResult<M> {
        if self.focused_item.is_none() {
            return EventResult::Ignored;
        }
        let cursor = self.focused_index(state);
        let page = self.viewport.visible_items(area);
        // Anything the shared key map does not claim — including every letter
        // but `j` and `k` — bubbles, so the app keeps its hotkeys.
        match list_core::key_intent(key, Axis::Vertical, &self.items, cursor, page) {
            Some(KeyIntent::Move(target)) => self.move_focus(target, state, area),
            Some(KeyIntent::Stay) => EventResult::Consumed,
            Some(KeyIntent::Commit(index)) => self.select(index),
            None => EventResult::Ignored,
        }
    }
}

impl<T: Clone + PartialEq + 'static, S, M> Component<S, M> for List<T, S, M> {
    fn prepare(&mut self, _state: &S) {
        // Quadratic in the item count and re-derived on every frame's fresh
        // instance, so a release build takes the items on trust.
        if cfg!(debug_assertions) {
            list_core::assert_unique_values(self.items.iter().map(ListItem::value), "List");
        }
        assert!(
            !(self.selected.is_some() && self.selected_many.is_some()),
            "List::selection(...) and List::multi_selection(...) cannot be used together; choose one selection mode"
        );
    }

    fn declare(&mut self, ctx: &mut DeclareCtx<'_, S, M>) {
        let area = ctx.area();
        let state = ctx.state();
        let focused_item = self.focused_index(state);
        let requested = self.scroll.as_ref().map(|scroll| scroll(state));
        // The wheel holds the view against the list as it stood. While nothing
        // has moved under it, the held offset is painted as it is — the wheel
        // may leave the cursor off-screen. Once the cursor moves, or the items
        // do, it is scrolled back into view. A bound scroll offset always wins
        // over the hold: the app owns it.
        WheelHold::settle_transient(
            ctx,
            &self.items,
            focused_item,
            requested,
            &mut self.viewport,
            area,
        );
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_, S>) {
        let area = ctx.area();
        let state = ctx.state();
        let focused_item = self.focused_index(state);
        let selected_row = self.selected_index(state);
        let disabled_items: Vec<bool> = self.items.iter().map(ListItem::is_disabled).collect();
        let style = resolve_style(self.style.as_deref(), ctx.theme, ListStyle::from_theme);
        // One cursor, shown while the list is either focused or under the
        // pointer; where it sits was decided during declaration.
        let cursor_visible = ctx.focused() || ctx.hovered();
        let selection_mode = if self.selected_many.is_some() {
            Some(true)
        } else if self.selected.is_some() {
            Some(false)
        } else {
            None
        };
        // The pair selection rows show: the shape selection picks, with any
        // marker overrides the app supplied taking each half. No selection, no
        // markers.
        let glyphs = selection_mode.map(|multiple| {
            let defaults = if multiple {
                selection_indicator::MarkerGlyphs::checkbox()
            } else {
                selection_indicator::MarkerGlyphs::radio()
            };
            selection_indicator::MarkerGlyphs {
                selected: self.selected_marker.as_deref().unwrap_or(defaults.selected),
                unselected: self
                    .unselected_marker
                    .as_deref()
                    .unwrap_or(defaults.unselected),
            }
        });
        let rows_per_item = self.viewport.rows_per_item();
        // Only the rows on screen are built, however long the list is. The
        // count rounds up because the area's trailing rows still paint the item
        // that starts in them, clipped; every index below counts from the start
        // of the list, so a custom `paint_item` and retained hit-testing see
        // the same positions whatever the view is scrolled to.
        let first_item = self.viewport.painted_offset().min(self.items.len());
        let last_item = first_item
            .saturating_add(usize::from(area.height.div_ceil(rows_per_item)))
            .min(self.items.len());
        let mut selected_items: Vec<usize> = Vec::new();
        let items = list_core::windowed_rows(
            &self.items,
            first_item..last_item,
            rows_per_item,
            cursor_visible.then_some(focused_item).flatten(),
            self.disabled,
            // Asked per painted row rather than looked up in a list of every
            // selected index, which would make painting quadratic.
            |index, value| match &self.selected_many {
                Some(selected_many) => selected_many(state, value),
                None => selected_row == Some(index),
            },
            |row| {
                if row.selected {
                    selected_items.push(row.index);
                }
                match &self.paint_item {
                    Some(paint_item) => paint_item(state, row),
                    None => default_item_line(&row, glyphs, &style),
                }
            },
        );
        ctx.widget(
            ListWidget::new(&items)
                .first_item(first_item)
                .focused_item(focused_item)
                .selected_items(&selected_items)
                .disabled_items(&disabled_items)
                .style(style)
                .focused(cursor_visible)
                .hovered(ctx.hovered())
                .disabled(self.disabled)
                .focus_symbol(&self.focus_symbol),
            area,
        );
    }

    fn handle_event(&mut self, event: &Event, state: &S, ctx: &mut EventCtx<'_>) -> EventResult<M> {
        if self.disabled || self.items.is_empty() {
            return EventResult::Ignored;
        }
        let area = ctx.area();
        match event {
            Event::Mouse(mouse) => match mouse.kind {
                // The wheel addresses the view rather than a row, so it is
                // answered before the row decision is asked for.
                MouseKind::Scroll(direction) => self.scroll_view(direction, state, area, ctx),
                // `row_at` already rejects anything outside the list, so no
                // paint-state gate is needed here; whether the cursor is *shown*
                // stays a paint decision.
                kind => {
                    let row = self
                        .viewport
                        .row_at(area, self.items.len(), mouse.column, mouse.row);
                    match list_core::row_intent(
                        kind,
                        &self.items,
                        row,
                        self.focused_index(state),
                        self.selected.is_some() || self.selected_many.is_some(),
                    ) {
                        RowIntent::BlockPress | RowIntent::Stay => EventResult::Consumed,
                        RowIntent::Focus(index) => self.move_focus(index, state, area),
                        RowIntent::Commit(index) => self.select(index),
                        RowIntent::Bubble => EventResult::Ignored,
                    }
                }
            },
            Event::Key(key) => self.handle_key(*key, state, area),
            _ => EventResult::Ignored,
        }
    }

    fn scope_options(&self) -> ScopeOptions {
        ScopeOptions::default().focusable(
            self.focused_item.is_some()
                && !self.disabled
                && linear_nav::has_enabled(self.items.len(), |i| self.disabled_at(i)),
        )
    }
}

fn default_item_line<T>(
    row: &ListItemState<'_, T>,
    glyphs: Option<selection_indicator::MarkerGlyphs<'_>>,
    style: &ListStyle,
) -> Text<'static> {
    let Some(glyphs) = glyphs else {
        return Text::from(row.label.to_string());
    };
    Text::from(selection_indicator::marker_line(
        row.label,
        row.selected,
        row.disabled,
        selection_indicator::MarkerColors {
            disabled: style.disabled_foreground,
            selected: style.selected_marker,
            unselected: style.unselected_marker,
        },
        glyphs,
    ))
}
