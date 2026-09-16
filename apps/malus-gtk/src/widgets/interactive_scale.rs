//! GtkScale interaction ownership. Programmatic synchronization is guarded;
//! pointer, keyboard and accessibility edits all use the same commit path.
use relm4::gtk::{self, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};

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

#[derive(Clone)]
pub struct InteractiveScale {
    pub widget: gtk::Scale,
    interaction: Rc<RefCell<Interaction>>,
    timer: Rc<RefCell<Option<gtk::glib::SourceId>>>,
    synchronizing: Rc<Cell<bool>>,
}

impl InteractiveScale {
    /// Volume coalesces edits every 40ms; seek commits once on pointer release
    /// (or after a short keyboard/accessibility edit burst).
    pub fn new(
        upper: f64,
        step: f64,
        volume: bool,
        preview: impl Fn(f64) + 'static,
        commit: impl Fn(f64) + 'static,
    ) -> Self {
        let widget = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, upper, step);
        if volume {
            widget.set_value(upper);
        }
        widget.set_draw_value(false);
        widget.set_hexpand(true);
        widget.set_width_request(40);
        widget.set_focusable(true);
        let interaction = Rc::new(RefCell::new(Interaction::default()));
        let timer: Rc<RefCell<Option<gtk::glib::SourceId>>> = Rc::new(RefCell::new(None));
        let commit: Rc<dyn Fn(f64)> = Rc::new(commit);
        let preview: Rc<dyn Fn(f64)> = Rc::new(preview);

        let state = interaction.clone();
        let pending_timer = timer.clone();
        let send = commit.clone();
        let synchronizing = Rc::new(Cell::new(true));
        let syncing = synchronizing.clone();
        widget.connect_value_changed(move |scale| {
            if syncing.get() {
                return;
            }
            let value = scale
                .value()
                .clamp(scale.adjustment().lower(), scale.adjustment().upper());
            {
                let mut state = state.borrow_mut();
                state.preview = Some(value);
                state.dirty = true;
            }
            preview(value);
            if volume || !state.borrow().pointer_down {
                if !volume && let Some(source) = pending_timer.borrow_mut().take() {
                    source.remove();
                }
                if pending_timer.borrow().is_none() {
                    let state = state.clone();
                    let timer = pending_timer.clone();
                    let send = send.clone();
                    *pending_timer.borrow_mut() = Some(gtk::glib::timeout_add_local_once(
                        Duration::from_millis(if volume { 40 } else { 160 }),
                        move || {
                            timer.borrow_mut().take();
                            let value = state.borrow_mut().commit();
                            if let Some(value) = value {
                                send(value);
                            }
                        },
                    ));
                }
            }
        });

        // Observe raw release in capture phase. GestureClick::released is not a
        // drag-finished signal: GTK's range drag recognizer can deny that gesture.
        let events = gtk::EventControllerLegacy::new();
        events.set_propagation_phase(gtk::PropagationPhase::Capture);
        let state = interaction.clone();
        let pending_timer = timer.clone();
        events.connect_event(move |_, event| {
            use gtk::gdk::EventType;
            match event.event_type() {
                EventType::ButtonPress | EventType::TouchBegin => {
                    state.borrow_mut().pointer_down = true
                }
                EventType::ButtonRelease | EventType::TouchEnd => {
                    if let Some(source) = pending_timer.borrow_mut().take() {
                        source.remove();
                    }
                    let value = {
                        let mut state = state.borrow_mut();
                        state.pointer_down = false;
                        let value = state.commit();
                        state.preview = None;
                        value
                    };
                    if let Some(value) = value {
                        commit(value);
                    }
                }
                EventType::TouchCancel => *state.borrow_mut() = Interaction::default(),
                _ => {}
            }
            gtk::glib::Propagation::Proceed
        });
        widget.add_controller(events);
        synchronizing.set(false);
        let result = Self {
            widget,
            interaction,
            timer,
            synchronizing,
        };
        let state = result.interaction.clone();
        let timer = result.timer.clone();
        result.widget.connect_unmap(move |_| {
            if let Some(source) = timer.borrow_mut().take() {
                source.remove();
            }
            *state.borrow_mut() = Interaction::default();
        });
        result
    }

    pub fn sync(&self, value: f64, upper: f64, tolerance: f64) -> f64 {
        let value = self
            .interaction
            .borrow_mut()
            .display(value, tolerance, Instant::now());
        self.synchronizing.set(true);
        self.widget.set_range(0.0, upper.max(1.0));
        // Do not touch even the displayed value while a pointer owns the scale.
        if !self.interaction.borrow().pointer_down {
            self.widget.set_value(value);
        }
        self.synchronizing.set(false);
        value
    }

    pub fn cancel(&self) {
        if let Some(source) = self.timer.borrow_mut().take() {
            source.remove();
        }
        *self.interaction.borrow_mut() = Interaction::default();
    }
}

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
