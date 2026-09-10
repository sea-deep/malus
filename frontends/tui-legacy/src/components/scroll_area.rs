// Copied from ratcn 0.0.3: src/components/scroll_area.rs

// A vertical viewport for arbitrary interactive ratcn descendants.
//
// Descendants are declared against their full logical content allocations.
// The runtime translates and clips ordinary paint and pointer input without
// changing those allocations, while keeping offscreen descendants in focus
// traversal. Popup, hint, modal, and deferred paint escape the ordinary clip.

use ratatui::{
    layout::Rect,
    style::{Color, Style},
    widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState},
};

use ratcn::Theme;
use ratcn::runtime::{
    Component, DeclareCtx, Event, EventCtx, EventResult, KeyCode, MouseKind, ScopeOptions,
    ScrollDirection,
};
use ratcn::theme::resolve_style;

const WHEEL_ROWS: u16 = 3;

/// Every color a [`ScrollArea`] scrollbar paints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollAreaStyle {
    /// Scrollbar thumb color.
    pub thumb: Color,
    /// Scrollbar track color.
    pub track: Color,
}

impl ScrollAreaStyle {
    /// Derive the thumb and track from `theme`.
    #[must_use]
    pub const fn from_theme(theme: &Theme) -> Self {
        Self {
            thumb: theme.primary,
            track: theme.border,
        }
    }
}

/// Where the wheel, a key, or a reveal left the view.
///
/// Event handling writes it and the next declaration reads it, which is what
/// lets an unbound area scroll at all and what carries a reveal into the frame
/// that follows a focus change.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum ScrollHold {
    /// Nothing is holding the view: the bound offset decides where it sits.
    #[default]
    Released,
    /// The view is held at `offset`. `base` is the bound offset the hold was
    /// taken against, and it is what makes releasing permanent: the
    /// declaration drops the hold for good the moment a bound offset moves
    /// away from `base`, so an app that scrolls its own area keeps it, and
    /// returning to the offset the hold was taken at cannot revive it.
    Held { offset: u16, base: Option<u16> },
}

type ReadOffsetFn<S> = Box<dyn Fn(&S) -> u16>;
type OnChangeFn<M> = Box<dyn Fn(u16) -> M>;
type ContentFn<S, M> = Box<dyn FnOnce(&mut DeclareCtx<'_, S, M>)>;
type StyleFn = Box<dyn Fn(&Theme) -> ScrollAreaStyle>;

/// A vertical viewport that hosts arbitrary interactive ratcn descendants.
///
/// One column is reserved at the right for the scrollbar. The
/// [`content`](Self::content) closure receives the remaining width and exactly
/// `content_height` logical rows, however many of them are visible. The
/// scrollbar is an indicator of where the view sits.
///
/// The offset is the area's own until [`scroll`](Self::scroll) binds it. Wheel,
/// Page Up, Page Down, Home, and End reach descendants first; what none of them
/// handles scrolls the area. An event that leaves the offset where it is — every
/// one of these keys at an edge, and a horizontal wheel — bubbles on to the app,
/// which keeps app hotkeys on those keys alive. The area is a fallback focus
/// stop while it holds no focusable descendant, so keyboard scrolling works for
/// paint-only content.
///
/// Focus moving to a descendant the viewport clips scrolls that descendant
/// into view.
///
/// # Panics
///
/// A `ScrollArea` inside another `ScrollArea` panics, as does content larger
/// than 262,144 cells, as does a single paint inside one covering more than
/// that.
///
/// ```
/// # use ratatui::layout::Rect;
/// # use ratcn::runtime::DeclareCtx;
/// # use ratcn::{Button, ScrollArea};
/// # struct State;
/// # enum Msg { Saved }
/// # fn declare(ctx: &mut DeclareCtx<'_, State, Msg>) {
/// ctx.component(
///     "settings",
///     ScrollArea::new(40).content(|ctx| {
///         ctx.component(
///             "save",
///             Button::new("Save").on_press(|| Msg::Saved),
///             Rect::new(0, 0, 10, 3),
///         );
///     }),
///     Rect::new(0, 0, 30, 10),
/// );
/// # }
/// ```
pub struct ScrollArea<S, M> {
    content_height: u16,
    read_offset: Option<ReadOffsetFn<S>>,
    on_change: Option<OnChangeFn<M>>,
    content: Option<ContentFn<S, M>>,
    style: Option<StyleFn>,
    hover_focus: bool,
}

impl<S, M> std::fmt::Debug for ScrollArea<S, M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScrollArea")
            .field("content_height", &self.content_height)
            .field("content", &self.content.is_some())
            .field("hover_focus", &self.hover_focus)
            .finish_non_exhaustive()
    }
}

impl<S, M> ScrollArea<S, M> {
    /// A viewport over `content_height` logical rows.
    #[must_use]
    pub fn new(content_height: u16) -> Self {
        Self {
            content_height,
            read_offset: None,
            on_change: None,
            content: None,
            style: None,
            hover_focus: false,
        }
    }

    /// Bind the first visible content row to app state.
    ///
    /// `read` is consulted for every event, so repeated wheel or page events
    /// compose before a redraw, and `on_change` carries each new offset to the
    /// app's update. Left unbound, the area keeps the offset itself and emits
    /// nothing.
    ///
    /// A reveal moves the view on its own, without a message; the next offset
    /// the area emits starts from where the reveal left it.
    #[must_use]
    pub fn scroll(
        mut self,
        read: impl Fn(&S) -> u16 + 'static,
        on_change: impl Fn(u16) -> M + 'static,
    ) -> Self {
        self.read_offset = Some(Box::new(read));
        self.on_change = Some(Box::new(on_change));
        self
    }

    /// Declare the viewport's descendants in their full logical content area.
    #[must_use]
    pub fn content(mut self, content: impl FnOnce(&mut DeclareCtx<'_, S, M>) + 'static) -> Self {
        self.content = Some(Box::new(content));
        self
    }

    /// Replace the theme-derived scrollbar style.
    #[must_use]
    pub fn style(mut self, style: impl Fn(&Theme) -> ScrollAreaStyle + 'static) -> Self {
        self.style = Some(Box::new(style));
        self
    }

    /// Make pointer motion choose between the content's direct child scopes.
    ///
    /// This is opt-in so ordinary scrollable forms leave keyboard focus alone
    /// when the pointer drifts. It suits a scrollable pane or tile grid whose
    /// direct children are the regions focus should follow.
    #[must_use]
    pub const fn hover_focus(mut self) -> Self {
        self.hover_focus = true;
        self
    }

    /// The visible content rectangle inside `area`: everything but the
    /// scrollbar gutter.
    fn viewport(area: Rect) -> Rect {
        Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height)
    }

    fn max_offset(&self, area: Rect) -> u16 {
        self.content_height
            .saturating_sub(Self::viewport(area).height)
    }

    fn bound_offset(&self, state: &S) -> Option<u16> {
        self.read_offset.as_ref().map(|read| read(state))
    }

    /// The offset in force: a standing hold, or the bound value.
    fn resolve(&self, area: Rect, bound: Option<u16>, hold: ScrollHold) -> u16 {
        let offset = match hold {
            ScrollHold::Held { offset, base } if base == bound => offset,
            _ => bound.unwrap_or(0),
        };
        offset.min(self.max_offset(area))
    }

    /// Release a hold the app has scrolled out from under, and answer with the
    /// offset this declaration lays out from.
    ///
    /// Releasing happens here, once per frame, because the declaration is
    /// where a bound offset is read; and it is permanent, so an app that
    /// returns to the offset a hold was taken at does not revive it.
    fn settle(&self, ctx: &mut DeclareCtx<'_, S, M>, area: Rect, bound: Option<u16>) -> u16 {
        let mut unheld = ScrollHold::Released;
        let hold = ctx.transient_mut::<ScrollHold>().unwrap_or(&mut unheld);
        if matches!(*hold, ScrollHold::Held { base, .. } if base != bound) {
            *hold = ScrollHold::Released;
        }
        self.resolve(area, bound, *hold)
    }

    /// The offset in force at event time.
    fn current(&self, state: &S, ctx: &mut EventCtx<'_>) -> u16 {
        let hold = *ctx.transient::<ScrollHold>();
        self.resolve(ctx.area(), self.bound_offset(state), hold)
    }

    /// Hold the view at `offset` for the coming declaration, and report the
    /// offset taken. `None` when the view is already there.
    fn hold(&self, offset: u16, state: &S, ctx: &mut EventCtx<'_>) -> Option<u16> {
        let area = ctx.area();
        let current = self.current(state, ctx);
        let offset = offset.min(self.max_offset(area));
        if offset == current {
            return None;
        }
        *ctx.transient::<ScrollHold>() = ScrollHold::Held {
            offset,
            base: self.bound_offset(state),
        };
        Some(offset)
    }

    /// Scroll to `offset`.
    ///
    /// [`EventResult::Ignored`] when the view is already there, which leaves an
    /// app hotkey on Home, End, or a page key working while focus rests in an
    /// area with nothing to scroll.
    fn scroll_to(&self, offset: u16, state: &S, ctx: &mut EventCtx<'_>) -> EventResult<M> {
        let Some(offset) = self.hold(offset, state, ctx) else {
            return EventResult::Ignored;
        };
        match &self.on_change {
            Some(on_change) => EventResult::Emit(on_change(offset)),
            None => EventResult::Consumed,
        }
    }

    /// The smallest offset change that puts `target` on screen.
    fn reveal_offset(&self, target: Rect, area: Rect, current: u16) -> u16 {
        let viewport = Self::viewport(area);
        let visible_top = viewport.y.saturating_add(current);
        let visible_bottom = visible_top.saturating_add(viewport.height);
        let requested = if target.height > viewport.height || target.y < visible_top {
            target.y.saturating_sub(viewport.y)
        } else if target.bottom() > visible_bottom {
            target
                .bottom()
                .saturating_sub(viewport.height)
                .saturating_sub(viewport.y)
        } else {
            current
        };
        requested.min(self.max_offset(area))
    }
}

impl<S: 'static, M: 'static> Component<S, M> for ScrollArea<S, M> {
    fn declare(&mut self, ctx: &mut DeclareCtx<'_, S, M>) {
        let area = ctx.area();
        let viewport = Self::viewport(area);
        let bound = self.bound_offset(ctx.state());
        let offset = self.settle(ctx, area, bound);
        let content = self.content.take();
        ctx.viewport(viewport, self.content_height, offset, |ctx| {
            if let Some(content) = content {
                content(ctx);
            }
        });

        if self.content_height <= viewport.height || area.width == 0 || area.height == 0 {
            return;
        }
        let gutter = Rect::new(area.right().saturating_sub(1), area.y, 1, area.height);
        let style = resolve_style(
            self.style.as_deref(),
            ctx.theme,
            ScrollAreaStyle::from_theme,
        );
        let viewport_height = viewport.height;
        // Scrollbar positions are row offsets, so the count is the inclusive
        // offset range, not the total row count.
        let position_count = self
            .content_height
            .saturating_sub(viewport_height)
            .saturating_add(1);
        ctx.paint(move |ctx| {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .thumb_style(Style::new().fg(style.thumb))
                .track_style(Style::new().fg(style.track));
            let mut state = ScrollbarState::new(usize::from(position_count))
                .position(usize::from(offset))
                .viewport_content_length(usize::from(viewport_height));
            ctx.stateful_widget(scrollbar, gutter, &mut state);
        });
    }

    fn handle_event(&mut self, event: &Event, state: &S, ctx: &mut EventCtx<'_>) -> EventResult<M> {
        let area = ctx.area();
        let current = self.current(state, ctx);
        let page = Self::viewport(area).height;
        let target = match event {
            Event::Mouse(mouse) => match mouse.kind {
                MouseKind::Scroll(ScrollDirection::Up) => current.saturating_sub(WHEEL_ROWS),
                MouseKind::Scroll(ScrollDirection::Down) => current.saturating_add(WHEEL_ROWS),
                _ => return EventResult::Ignored,
            },
            Event::Key(key) if !key.modifiers.any() => match key.code {
                KeyCode::PageUp => current.saturating_sub(page),
                KeyCode::PageDown => current.saturating_add(page),
                KeyCode::Home => 0,
                KeyCode::End => self.max_offset(area),
                _ => return EventResult::Ignored,
            },
            _ => return EventResult::Ignored,
        };
        self.scroll_to(target, state, ctx)
    }

    fn reveal_in_viewport(&mut self, target: Rect, state: &S, ctx: &mut EventCtx<'_>) {
        let area = ctx.area();
        let current = self.current(state, ctx);
        self.hold(self.reveal_offset(target, area, current), state, ctx);
    }

    fn scope_options(&self) -> ScopeOptions {
        let options = ScopeOptions::default().focusable(true);
        if self.hover_focus {
            options.hover_focus()
        } else {
            options
        }
    }
}
