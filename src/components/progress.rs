// Copied from ratcn 0.0.3: src/components/progress.rs

// A slim horizontal bar showing how far a task has come.
//
// ```text
// Uploading notes.md                    56%
// █████▋
// ```
//
// This is a themed, opinionated take on ratatui's [`Gauge`]: the gauge keeps
// the drawing — including the fractional block that lets the fill move in
// steps finer than one cell — and this widget adds what an application
// progress bar wants that a raw gauge does not carry:
//
// - theme colors, from a [`Theme`] via [`themed`](Self::themed) or an
//   explicit [`ProgressStyle`] — the theme's primary as the fill on the
//   inset well the other control surfaces use, the way the bar chart paints
// - an optional [`label`](ProgressWidget::label) above the bar, left-aligned
// - an optional [`show_value`](ProgressWidget::show_value) percentage,
//   right-aligned on that same row, the way shadcn/ui composes
//   `ProgressLabel` and `ProgressValue`
//
// The ratio is clamped into `0.0..=1.0` on the way in, so a denominator that
// briefly misbehaves cannot smear the bar off its track.
//
// A progress bar takes no focus and handles no events, so there is no
// interactive half.
//
// **Usable in any ratatui app.** Nothing here depends on
// [`Ratcn`](ratcn::runtime::Ratcn) or the component layer — it is an ordinary
// [`Widget`], so `frame.render_widget(...)` is all it needs.
//
// # Examples
//
// ```
// use ratcn::ProgressWidget;
//
// // Just the bar.
// # let _ =
// ProgressWidget::new(0.33);
//
// // Label and percentage above the bar.
// # let _ =
// ProgressWidget::new(0.56)
//     .label("Uploading notes.md")
//     .show_value(true);
// ```

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::Span,
    widgets::{Gauge, Widget},
};

use ratcn::{Theme, text_width};

/// Colors for a [`ProgressWidget`].
///
/// A bar has no interaction states, so unlike the interactive components'
/// style structs there is one color per role rather than one per role per
/// state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgressStyle {
    /// The fill painted from the left — the part of the task done so far.
    pub fill: Color,
    /// The well the fill moves through — the whole extent of the task.
    pub track: Color,
    /// The [`label`](ProgressWidget::label) above the bar.
    pub label: Color,
    /// The [`show_value`](ProgressWidget::show_value) percentage.
    pub value: Color,
}

impl ProgressStyle {
    /// A neutral style using plain ANSI colors, for painting without a
    /// [`Theme`]. Prefer [`from_theme`](Self::from_theme) when one is
    /// available.
    #[must_use]
    pub const fn fallback() -> Self {
        Self {
            fill: Color::Cyan,
            track: Color::DarkGray,
            label: Color::Gray,
            value: Color::Gray,
        }
    }

    /// Derive bar colors from `theme`: the primary as the fill, on the inset
    /// well the chart backdrop uses, with the label read as secondary text
    /// and the percentage as ordinary text.
    #[must_use]
    pub const fn from_theme(theme: &Theme) -> Self {
        Self {
            fill: theme.primary,
            track: theme.field,
            label: theme.muted_foreground,
            value: theme.foreground,
        }
    }
}

/// A progress bar — the fill's share of the track is the work done.
///
/// One instantiation is one bar. The ratio arrives through
/// [`new`](Self::new); [`label`](Self::label) and
/// [`show_value`](Self::show_value) compose the header above the track; and
/// [`themed`](Self::themed) or [`style`](Self::style) choose the colors.
#[derive(Debug, Clone, PartialEq)]
pub struct ProgressWidget<'a> {
    /// The fraction done, already clamped into `0.0..=1.0`.
    ratio: f64,
    label: Option<&'a str>,
    show_value: bool,
    style: ProgressStyle,
}

impl<'a> ProgressWidget<'a> {
    /// A bare bar filled by `ratio`, where `0.0` is empty and `1.0` is full.
    ///
    /// Values outside `0.0..=1.0` are clamped: infinities pin to the nearer
    /// end, and a NaN that slipped out of a division counts as empty rather
    /// than printing itself as a percentage.
    #[must_use]
    pub fn new(ratio: f64) -> Self {
        Self {
            ratio: clamp_ratio(ratio),
            label: None,
            show_value: false,
            style: ProgressStyle::fallback(),
        }
    }

    /// Label the bar — the task's name, above the fill, left-aligned. Pair it
    /// with [`show_value`](Self::show_value) to complete the composition:
    /// name on the left, percentage on the right, bar underneath.
    #[must_use]
    pub const fn label(mut self, label: &'a str) -> Self {
        self.label = Some(label);
        self
    }

    /// Print the percentage right-aligned above the bar. Rounded to the
    /// nearest whole percent, `0%` through `100%`. The percentage and the
    /// fill round independently — ratatui's gauge rounds its last cell in
    /// eighths — so at a width's rounding edge the number can briefly sit one
    /// eighth of a cell away from the bar it describes. On a row too narrow
    /// for the number, what fits of it prints, digits first.
    #[must_use]
    pub const fn show_value(mut self, show_value: bool) -> Self {
        self.show_value = show_value;
        self
    }

    /// Take colors from `theme`.
    #[must_use]
    pub const fn themed(mut self, theme: &Theme) -> Self {
        self.style = ProgressStyle::from_theme(theme);
        self
    }

    /// Use these exact colors, ignoring any theme.
    #[must_use]
    pub const fn style(mut self, style: ProgressStyle) -> Self {
        self.style = style;
        self
    }

    /// Rows the bar asks a layout for: two when a label or the percentage
    /// shows, one otherwise — an empty label still shows, since you asked
    /// for the row. Given less, the bar keeps the rows there are and
    /// the header is the first thing dropped; given more, the extra rows are
    /// left alone — a progress bar is one row of track, not a block meter.
    #[must_use]
    pub const fn height(&self) -> u16 {
        if self.has_header() { 2 } else { 1 }
    }

    const fn has_header(&self) -> bool {
        self.label.is_some() || self.show_value
    }
}

/// Any ratio becomes one a gauge can draw: finite values clamp into
/// `0.0..=1.0`, and infinities run to the nearer end while NaN lands empty.
fn clamp_ratio(ratio: f64) -> f64 {
    if ratio.is_nan() {
        0.0
    } else {
        ratio.clamp(0.0, 1.0)
    }
}

impl Widget for ProgressWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }

        // The header sits directly above the track, and only when there are
        // rows for both; squeezed to one row, the bar keeps the space.
        let (header_area, bar_area) = if self.has_header() && area.height >= 2 {
            (
                Some(Rect { height: 1, ..area }),
                Rect {
                    y: area.y + 1,
                    height: area.height - 1,
                    ..area
                },
            )
        } else {
            (None, area)
        };

        // The bar is exactly one row tall, wherever inside the area that row
        // was promised to be.
        let bar_area = ratcn::geometry::fixed_height(bar_area, 1);

        if let Some(header_area) = header_area {
            self.render_header(header_area, buf);
        }
        if !bar_area.is_empty() {
            Gauge::default()
                .ratio(self.ratio)
                .label("")
                .use_unicode(true)
                .gauge_style(Style::default().fg(self.style.fill).bg(self.style.track))
                .render(bar_area, buf);
            self.restore_label_slot(bar_area, buf);
        }
    }
}

impl ProgressWidget<'_> {
    /// Ratatui reserves the cell under its own centered label even when the
    /// label is empty: whenever the fill crosses the gauge's middle column,
    /// that one cell comes back blanked with the fill and track colors
    /// swapped. The percentage lives up in the header here, so the cell is
    /// restored to the full block its neighbors carry. The exact slot and
    /// fill math are pinned by the tests — an upstream change breaks loudly
    /// here rather than punching silent holes in every app's bars.
    ///
    /// Fixed upstream in ratatui#2579 (issue #2547), unreleased as of
    /// ratatui 0.30.2. Once a release carrying it is pinned, this restore
    /// overwrites the cell with the identical content — delete it then.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the ratio is clamped into 0.0..=1.0, so the fill never exceeds the area's width and always fits a u16"
    )]
    fn restore_label_slot(&self, bar_area: Rect, buf: &mut Buffer) {
        let filled_end = bar_area.x + (f64::from(bar_area.width) * self.ratio).floor() as u16;
        let label_slot = bar_area.x + bar_area.width / 2;
        if filled_end > label_slot {
            buf[(label_slot, bar_area.y)]
                .set_symbol(ratatui::symbols::block::FULL)
                .set_fg(self.style.fill)
                .set_bg(self.style.track);
        }
    }
    /// The header row: label flush left, percentage flush right, each in its
    /// own color. On a row too narrow for both, the percentage holds its
    /// place and the label takes what remains.
    fn render_header(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "the ratio is clamped into 0.0..=1.0, so the percentage rounds into 0..=100 and always fits a u16"
        )]
        let value_text = format!("{}%", (self.ratio * 100.0).round() as u16);
        // Only a shown percentage may hold room at the right edge; a
        // label-only header gets the whole row.
        let value_width = if self.show_value {
            text_width::display_width_u16(&value_text).min(area.width)
        } else {
            0
        };
        let value_x = area.right() - value_width;

        if self.show_value {
            Span::raw(value_text.as_str())
                .style(Style::default().fg(self.style.value))
                .render(Rect::new(value_x, area.y, value_width, 1), buf);
        }

        if let Some(label) = self.label {
            // With a percentage showing, the label stops one cell short of it,
            // so a truncated label cannot read as one string with the number.
            let gap = u16::from(self.show_value);
            let room = (value_x - area.x).saturating_sub(gap);
            if room > 0 {
                Span::raw(text_width::truncate_to_width(label, usize::from(room)))
                    .style(Style::default().fg(self.style.label))
                    .render(Rect::new(area.x, area.y, room, 1), buf);
            }
        }
    }
}
