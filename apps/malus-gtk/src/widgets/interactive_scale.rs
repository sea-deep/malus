//! Custom slider widget — draws its own trough, fill, and knob via Cairo.
//!
//! The knob centre is **always positioned exactly at `fill_fraction × widget_width`**,
//! providing pixel-perfect alignment that GTK's native `GtkScale` CSS layout cannot
//! guarantee (GTK's internal gizmo layout places the slider based on trough margins).
//!
//! # Theming
//! The CSS `color` property drives the fill/highlight colour:
//! ```css
//! .interactive-slider.seek-bar      { color: @accent_color; }
//! .interactive-slider.volume-slider { color: alpha(@window_fg_color, 0.75); }
//! ```
//! Trough (14 % alpha) and knob face (white handle matching Adwaita/macOS) are
//! derived dynamically — no hardcoded colour values.
use crate::design::tokens::{SEEK_THUMB_SIZE, SEEK_TROUGH_THICKNESS, SEEK_TROUGH_THICKNESS_ACTIVE};
use relm4::gtk::{self, prelude::*};
use std::{
    cell::{Cell, RefCell},
    f64::consts::TAU,
    rc::Rc,
    time::{Duration, Instant},
};

// ── Drawing constants from canonical tokens ──────────────────────────────────
const TROUGH_H_RESTING: f64 = SEEK_TROUGH_THICKNESS as f64;
const TROUGH_H_ACTIVE: f64 = SEEK_TROUGH_THICKNESS_ACTIVE as f64;
const KNOB_RADIUS: f64 = (SEEK_THUMB_SIZE as f64) / 2.0;

// Horizontal padding so the knob circle never gets clipped by the widget edge.
const KNOB_PAD: f64 = KNOB_RADIUS + 1.0;

// ── Interaction state ─────────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct Interaction {
    pointer_down: bool,
    dirty: bool,
    preview: Option<f64>,
    pending: Option<(f64, Instant)>,
}

impl Interaction {
    fn display(&mut self, actual: f64, tolerance: f64, now: Instant) -> f64 {
        if let Some(preview) = self.preview {
            return preview;
        }
        if let Some((target, sent)) = self.pending {
            if (actual - target).abs() <= tolerance
                || now.duration_since(sent) > Duration::from_secs(3)
            {
                self.pending = None;
            } else {
                return target;
            }
        }
        actual
    }

    fn commit(&mut self) -> Option<f64> {
        if !self.dirty {
            return None;
        }
        self.dirty = false;
        let value = self.preview?;
        self.pending = Some((value, Instant::now()));
        if !self.pointer_down {
            self.preview = None;
        }
        Some(value)
    }
}

// ── Draw state ────────────────────────────────────────────────────────────────

#[derive(Default)]
struct DrawState {
    fraction: f64, // 0.0 ..= 1.0
    hovered: bool,
    dragging: bool,
}

// ── Public widget ─────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct InteractiveScale {
    pub widget: gtk::DrawingArea,
    upper: Rc<Cell<f64>>,
    step: Rc<Cell<f64>>,
    volume: bool,
    preview_cb: Rc<dyn Fn(f64)>,
    commit_cb: Rc<dyn Fn(f64)>,
    draw: Rc<RefCell<DrawState>>,
    interaction: Rc<RefCell<Interaction>>,
    timer: Rc<RefCell<Option<gtk::glib::SourceId>>>,
}

impl InteractiveScale {
    /// * `upper`  — max value (track ms, or 1.0 for volume).
    /// * `step`   — keyboard / scroll step (same units as `upper`).
    /// * `volume` — 40 ms coalesced commit; `false` (seek) = commit on release.
    pub fn new(
        upper: f64,
        step: f64,
        volume: bool,
        preview: impl Fn(f64) + 'static,
        commit: impl Fn(f64) + 'static,
    ) -> Self {
        let widget = gtk::DrawingArea::new();
        widget.add_css_class("interactive-slider");
        widget.set_hexpand(true);
        widget.set_size_request(40, 20);
        widget.set_focusable(true);
        widget.set_can_focus(true);
        widget.set_accessible_role(gtk::AccessibleRole::Slider);

        let upper = Rc::new(Cell::new(upper.max(1.0)));
        let step = Rc::new(Cell::new(step.max(0.0001)));
        let draw: Rc<RefCell<DrawState>> = Rc::new(RefCell::new(DrawState {
            fraction: if volume { 1.0 } else { 0.0 },
            ..Default::default()
        }));
        let interaction: Rc<RefCell<Interaction>> = Rc::new(RefCell::new(Interaction::default()));
        let timer: Rc<RefCell<Option<gtk::glib::SourceId>>> = Rc::new(RefCell::new(None));
        let commit_cb: Rc<dyn Fn(f64)> = Rc::new(commit);
        let preview_cb: Rc<dyn Fn(f64)> = Rc::new(preview);

        // ── Draw function ─────────────────────────────────────────────────────
        {
            let ds = draw.clone();
            widget.set_draw_func(move |w, cr, width, height| {
                let ds = ds.borrow();
                let wf = width as f64;
                let hf = height as f64;

                // Drawable trough span: inset by KNOB_PAD on each side so the
                // knob circle never clips against the widget boundary.
                let track_x0 = KNOB_PAD;
                let track_w = (wf - 2.0 * KNOB_PAD).max(0.0);
                let cx = track_x0 + ds.fraction * track_w;
                let cy = hf / 2.0;

                let is_sensitive = w.is_sensitive();
                let is_active = (ds.hovered || ds.dragging || w.has_focus()) && is_sensitive;

                let th = if is_active {
                    TROUGH_H_ACTIVE
                } else {
                    TROUGH_H_RESTING
                };
                let ty = cy - th / 2.0;

                // Fill colour from CSS `color` property — zero hardcoded colours.
                let c = w.color();
                let alpha_mult = if is_sensitive { 1.0 } else { 0.45 };
                let (r, g, b, a) = (
                    f64::from(c.red()),
                    f64::from(c.green()),
                    f64::from(c.blue()),
                    f64::from(c.alpha()) * alpha_mult,
                );

                // Trough background (fill colour at 14 % opacity).
                cr.set_source_rgba(r, g, b, a * 0.14);
                pill(cr, track_x0, ty, track_w, th);
                let _ = cr.fill();

                // Highlight / progress.
                let fill_w = ds.fraction * track_w;
                if fill_w > 0.5 {
                    cr.set_source_rgba(r, g, b, a);
                    pill(cr, track_x0, ty, fill_w, th);
                    let _ = cr.fill();
                }

                // Knob centred exactly at the fill edge (only visible when active/hovered).
                if is_active {
                    // Shadow disc.
                    cr.set_source_rgba(0.0, 0.0, 0.0, 0.22 * alpha_mult);
                    cr.arc(cx, cy, KNOB_RADIUS + 0.75, 0.0, TAU);
                    let _ = cr.fill();
                    // White knob face — semantic handle colour.
                    cr.set_source_rgba(1.0, 1.0, 1.0, a);
                    cr.arc(cx, cy, KNOB_RADIUS, 0.0, TAU);
                    let _ = cr.fill();
                }
            });
        }

        // ── Hover detection (EventControllerMotion) ───────────────────────────
        {
            let ds = draw.clone();
            let wq = widget.clone();
            let motion = gtk::EventControllerMotion::new();
            {
                let ds = ds.clone();
                let wq = wq.clone();
                motion.connect_enter(move |ctrl, _x, _y| {
                    if ctrl.widget().is_some_and(|w| w.is_sensitive()) {
                        ds.borrow_mut().hovered = true;
                        wq.queue_draw();
                    }
                });
            }
            {
                let ds = ds.clone();
                let wq = wq.clone();
                motion.connect_motion(move |ctrl, _x, _y| {
                    if ctrl.widget().is_some_and(|w| w.is_sensitive()) && !ds.borrow().hovered {
                        ds.borrow_mut().hovered = true;
                        wq.queue_draw();
                    }
                });
            }
            {
                let ds = ds.clone();
                let wq = wq.clone();
                motion.connect_leave(move |_| {
                    if !ds.borrow().dragging {
                        ds.borrow_mut().hovered = false;
                        wq.queue_draw();
                    }
                });
            }
            widget.add_controller(motion);
        }

        // ── Focus outline / keyboard tracking ─────────────────────────────────
        {
            let wq = widget.clone();
            let focus = gtk::EventControllerFocus::new();
            {
                let wq = wq.clone();
                focus.connect_enter(move |_| {
                    wq.queue_draw();
                });
            }
            {
                let wq = wq.clone();
                focus.connect_leave(move |_| {
                    wq.queue_draw();
                });
            }
            widget.add_controller(focus);
        }

        // ── Pointer drag (GestureDrag) ────────────────────────────────────────
        {
            let gesture = gtk::GestureDrag::new();
            gesture.set_button(gtk::gdk::BUTTON_PRIMARY);
            gesture.set_propagation_phase(gtk::PropagationPhase::Bubble);

            fn x_to_frac(x: f64, widget_w: i32) -> f64 {
                let track_w = (widget_w as f64 - 2.0 * KNOB_PAD).max(1.0);
                ((x - KNOB_PAD) / track_w).clamp(0.0, 1.0)
            }

            // drag_begin — immediately claim sequence to prevent ancestor window handle drag
            {
                let ds = draw.clone();
                let upper_c = upper.clone();
                let iact = interaction.clone();
                let timer_c = timer.clone();
                let preview_c = preview_cb.clone();
                let commit_c = commit_cb.clone();
                let wq = widget.clone();
                gesture.connect_drag_begin(move |g, x, _y| {
                    if !g.widget().is_some_and(|w| w.is_sensitive()) {
                        return;
                    }
                    // Claim sequence immediately so WindowHandle does not trigger a window move.
                    g.set_state(gtk::EventSequenceState::Claimed);
                    if let Some(w) = g.widget() {
                        w.grab_focus();
                    }

                    let widget_w = g.widget().map_or(1, |w| w.width());
                    let frac = x_to_frac(x, widget_w);
                    let value = frac * upper_c.get();
                    {
                        let mut st = iact.borrow_mut();
                        st.pointer_down = true;
                        st.preview = Some(value);
                        st.dirty = true;
                    }
                    {
                        let mut d = ds.borrow_mut();
                        d.fraction = frac;
                        d.dragging = true;
                    }
                    wq.queue_draw();
                    preview_c(value);
                    if volume {
                        start_timer(&timer_c, &iact, &commit_c, Duration::from_millis(40));
                    }
                });
            }

            // drag_update — continuous drag feedback
            {
                let ds = draw.clone();
                let upper_c = upper.clone();
                let iact = interaction.clone();
                let timer_c = timer.clone();
                let preview_c = preview_cb.clone();
                let commit_c = commit_cb.clone();
                let wq = widget.clone();
                gesture.connect_drag_update(move |g, offset_x, _offset_y| {
                    if !iact.borrow().pointer_down {
                        return;
                    }
                    let start_x = match g.start_point() {
                        Some((sx, _)) => sx,
                        None => return,
                    };
                    let widget_w = g.widget().map_or(1, |w| w.width());
                    let frac = x_to_frac(start_x + offset_x, widget_w);
                    let value = frac * upper_c.get();
                    {
                        let mut st = iact.borrow_mut();
                        st.preview = Some(value);
                        st.dirty = true;
                    }
                    ds.borrow_mut().fraction = frac;
                    wq.queue_draw();
                    preview_c(value);
                    if volume {
                        start_timer(&timer_c, &iact, &commit_c, Duration::from_millis(40));
                    }
                });
            }

            // drag_end — commit on release
            {
                let ds = draw.clone();
                let iact = interaction.clone();
                let timer_c = timer.clone();
                let commit_c = commit_cb.clone();
                let wq = widget.clone();
                gesture.connect_drag_end(move |g, offset_x, offset_y| {
                    if let Some(src) = timer_c.borrow_mut().take() {
                        src.remove();
                    }
                    let value = {
                        let mut st = iact.borrow_mut();
                        st.pointer_down = false;
                        let v = st.commit();
                        st.preview = None;
                        v
                    };
                    let is_inside = match g.start_point() {
                        Some((sx, sy)) => {
                            let ex = sx + offset_x;
                            let ey = sy + offset_y;
                            let w = g.widget().map_or(0, |w| w.width()) as f64;
                            let h = g.widget().map_or(0, |w| w.height()) as f64;
                            ex >= 0.0 && ex <= w && ey >= 0.0 && ey <= h
                        }
                        None => false,
                    };
                    {
                        let mut d = ds.borrow_mut();
                        d.dragging = false;
                        d.hovered = is_inside;
                    }
                    wq.queue_draw();
                    if let Some(v) = value {
                        commit_c(v);
                    }
                });
            }

            widget.add_controller(gesture);
        }

        // ── Scroll controller (mouse wheel on slider itself) ──────────────────
        {
            let ds = draw.clone();
            let upper_c = upper.clone();
            let step_c = step.clone();
            let iact = interaction.clone();
            let timer_c = timer.clone();
            let preview_c = preview_cb.clone();
            let commit_c = commit_cb.clone();
            let wq = widget.clone();

            let scroll = gtk::EventControllerScroll::new(
                gtk::EventControllerScrollFlags::VERTICAL
                    | gtk::EventControllerScrollFlags::HORIZONTAL,
            );
            scroll.connect_scroll(move |ctrl, dx, dy| {
                if !ctrl.widget().is_some_and(|w| w.is_sensitive()) {
                    return gtk::glib::Propagation::Proceed;
                }
                let delta = if dy != 0.0 { -dy } else { dx };
                if delta == 0.0 {
                    return gtk::glib::Propagation::Proceed;
                }
                let step_val = step_c.get();
                let change = if volume {
                    if delta > 0.0 {
                        (delta * 0.04).max(0.02)
                    } else {
                        (delta * 0.04).min(-0.02)
                    }
                } else {
                    delta * step_val.max(1000.0)
                };
                let upper_v = upper_c.get();
                let current = ds.borrow().fraction * upper_v;
                let new_value = (current + change).clamp(0.0, upper_v);
                let frac = if upper_v > 0.0 {
                    new_value / upper_v
                } else {
                    0.0
                };
                {
                    let mut st = iact.borrow_mut();
                    st.preview = Some(new_value);
                    st.dirty = true;
                }
                ds.borrow_mut().fraction = frac;
                wq.queue_draw();
                preview_c(new_value);

                let delay = Duration::from_millis(if volume { 40 } else { 160 });
                if !volume && let Some(src) = timer_c.borrow_mut().take() {
                    src.remove();
                }
                start_timer(&timer_c, &iact, &commit_c, delay);
                gtk::glib::Propagation::Stop
            });
            widget.add_controller(scroll);
        }

        // ── Keyboard (arrow keys, PageUp/Down, Home/End) ───────────────────────
        {
            let ds = draw.clone();
            let upper_c = upper.clone();
            let step_c = step.clone();
            let iact = interaction.clone();
            let timer_c = timer.clone();
            let preview_c = preview_cb.clone();
            let commit_c = commit_cb.clone();
            let wq = widget.clone();

            let keys = gtk::EventControllerKey::new();
            keys.connect_key_pressed(move |ctrl, key, _, _| {
                if !ctrl.widget().is_some_and(|w| w.is_sensitive()) {
                    return gtk::glib::Propagation::Proceed;
                }
                use gtk::gdk::Key;
                let step_val = step_c.get();
                let delta = match key {
                    Key::Left | Key::KP_Left | Key::Down | Key::KP_Down => -step_val,
                    Key::Right | Key::KP_Right | Key::Up | Key::KP_Up => step_val,
                    Key::Page_Down => -step_val * 5.0,
                    Key::Page_Up => step_val * 5.0,
                    Key::Home => -upper_c.get(),
                    Key::End => upper_c.get(),
                    _ => return gtk::glib::Propagation::Proceed,
                };
                let upper_v = upper_c.get();
                let current = ds.borrow().fraction * upper_v;
                let value = (current + delta).clamp(0.0, upper_v);
                let frac = if upper_v > 0.0 { value / upper_v } else { 0.0 };
                {
                    let mut st = iact.borrow_mut();
                    st.preview = Some(value);
                    st.dirty = true;
                }
                ds.borrow_mut().fraction = frac;
                wq.queue_draw();
                preview_c(value);

                if !volume && let Some(src) = timer_c.borrow_mut().take() {
                    src.remove();
                }
                let delay = Duration::from_millis(if volume { 40 } else { 160 });
                start_timer(&timer_c, &iact, &commit_c, delay);
                gtk::glib::Propagation::Stop
            });
            widget.add_controller(keys);
        }

        // ── Cleanup on unmap ──────────────────────────────────────────────────
        {
            let timer_u = timer.clone();
            let iact_u = interaction.clone();
            let draw_u = draw.clone();
            widget.connect_unmap(move |_| {
                if let Some(src) = timer_u.borrow_mut().take() {
                    src.remove();
                }
                *iact_u.borrow_mut() = Interaction::default();
                *draw_u.borrow_mut() = DrawState::default();
            });
        }

        Self {
            widget,
            upper,
            step,
            volume,
            preview_cb,
            commit_cb,
            draw,
            interaction,
            timer,
        }
    }

    // ── Public API ────────────────────────────────────────────────────────────

    /// Current raw value (fraction × upper).
    pub fn value(&self) -> f64 {
        self.draw.borrow().fraction * self.upper.get()
    }

    /// Returns the current step increment.
    pub fn step(&self) -> f64 {
        self.step.get()
    }

    /// Sets the step increment.
    pub fn set_step(&self, step: f64) {
        self.step.set(step.max(0.0001));
    }

    /// Returns the upper limit of the slider range.
    pub fn upper(&self) -> f64 {
        self.upper.get()
    }

    /// Sets the upper limit of the slider range.
    pub fn set_upper(&self, upper: f64) {
        self.upper.set(upper.max(1.0));
        self.widget.queue_draw();
    }

    /// Programmatically update the value, triggering preview and scheduling debounced commit.
    pub fn set_value(&self, value: f64) {
        let upper = self.upper.get();
        let clamped = value.clamp(0.0, upper);
        let frac = if upper > 0.0 { clamped / upper } else { 0.0 };
        {
            let mut st = self.interaction.borrow_mut();
            st.preview = Some(clamped);
            st.dirty = true;
        }
        self.draw.borrow_mut().fraction = frac;
        self.widget.queue_draw();
        (self.preview_cb)(clamped);

        let delay = Duration::from_millis(if self.volume { 40 } else { 160 });
        if !self.volume
            && let Some(src) = self.timer.borrow_mut().take()
        {
            src.remove();
        }
        start_timer(&self.timer, &self.interaction, &self.commit_cb, delay);
    }

    /// Legacy helper for callers passing an explicit commit closure.
    pub fn set_value_interactive(&self, value: f64, commit_fn: impl Fn(f64)) {
        self.set_value(value);
        commit_fn(self.value());
    }

    /// Called on each playback-state update. Prevents snap-back during user interaction.
    pub fn sync(&self, value: f64, upper: f64, tolerance: f64) -> f64 {
        let displayed = self
            .interaction
            .borrow_mut()
            .display(value, tolerance, Instant::now());
        let upper_v = upper.max(1.0);
        self.upper.set(upper_v);
        if !self.interaction.borrow().pointer_down {
            self.draw.borrow_mut().fraction = (displayed / upper_v).clamp(0.0, 1.0);
            self.widget.queue_draw();
        }
        displayed
    }

    /// Cancel any in-flight interaction / pending timers (e.g. on track change).
    pub fn cancel(&self) {
        if let Some(src) = self.timer.borrow_mut().take() {
            src.remove();
        }
        *self.interaction.borrow_mut() = Interaction::default();
    }

    /// Update sensitivity state. Clears hover/drag states if disabled.
    pub fn set_sensitive(&self, sensitive: bool) {
        self.widget.set_sensitive(sensitive);
        if !sensitive {
            self.cancel();
            let mut d = self.draw.borrow_mut();
            d.hovered = false;
            d.dragging = false;
            self.widget.queue_draw();
        }
    }

    /// Returns whether the slider is currently sensitive.
    pub fn is_sensitive(&self) -> bool {
        self.widget.is_sensitive()
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Pill (fully-rounded rectangle) Cairo path.
fn pill(cr: &gtk::cairo::Context, x: f64, y: f64, w: f64, h: f64) {
    use std::f64::consts::PI;
    let r = (h / 2.0).min(w / 2.0);
    if r <= 0.0 {
        return;
    }
    cr.move_to(x + r, y);
    cr.line_to(x + w - r, y);
    cr.arc(x + w - r, y + r, r, -PI / 2.0, 0.0);
    cr.line_to(x + w, y + h - r);
    cr.arc(x + w - r, y + h - r, r, 0.0, PI / 2.0);
    cr.line_to(x + r, y + h);
    cr.arc(x + r, y + h - r, r, PI / 2.0, PI);
    cr.line_to(x, y + r);
    cr.arc(x + r, y + r, r, PI, 3.0 * PI / 2.0);
    cr.close_path();
}

/// Start a coalescing commit timer.
fn start_timer(
    timer: &Rc<RefCell<Option<gtk::glib::SourceId>>>,
    interaction: &Rc<RefCell<Interaction>>,
    commit: &Rc<dyn Fn(f64)>,
    delay: Duration,
) {
    if timer.borrow().is_some() {
        return;
    }
    let timer2 = timer.clone();
    let iact2 = interaction.clone();
    let commit2 = commit.clone();
    *timer.borrow_mut() = Some(gtk::glib::timeout_add_local_once(delay, move || {
        timer2.borrow_mut().take();
        if let Some(v) = iact2.borrow_mut().commit() {
            commit2(v);
        }
    }));
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incoming_status_cannot_move_a_scrub_and_only_one_seek_is_committed() {
        let now = Instant::now();
        let mut state = Interaction {
            pointer_down: true,
            dirty: true,
            preview: Some(75_000.0),
            pending: None,
        };
        assert_eq!(state.display(12_000.0, 1500.0, now), 75_000.0);
        state.pointer_down = false;
        assert_eq!(state.commit(), Some(75_000.0));
        assert_eq!(state.commit(), None);
        assert_eq!(state.display(12_500.0, 1500.0, now), 75_000.0);
        assert_eq!(state.display(75_200.0, 1500.0, Instant::now()), 75_200.0);
    }

    #[test]
    fn zero_volume_is_a_valid_target_and_failed_commands_eventually_resync() {
        let mut state = Interaction {
            dirty: true,
            preview: Some(0.0),
            ..Default::default()
        };
        assert_eq!(state.commit(), Some(0.0));
        assert_eq!(state.display(1.0, 0.01, Instant::now()), 0.0);
        assert_eq!(
            state.display(1.0, 0.01, Instant::now() + Duration::from_secs(4)),
            1.0
        );
    }
}
