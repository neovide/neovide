use std::{cell::RefCell, collections::HashMap};

use objc2::rc::Retained;
use objc2_app_kit::NSScroller;

use super::{
    appkit::request_display,
    registry::{scrollbar_key, shared_state},
    state::{ScrollbarState, SharedScrollbarState},
};

thread_local! {
    /// AppKit callbacks run on the main thread and can reenter Rust during tracking,
    /// so this state is shared within that thread. Each entry pins the mouse-down
    /// target until ScrollbarTracking is dropped, even after cancellation.
    static INTERACTIONS: RefCell<HashMap<usize, Interaction>> = RefCell::new(HashMap::new());

    /// A nonzero count suppresses input callbacks for that control only. Counting
    /// guards rather than storing a flag keeps an inner setter from re-enabling
    /// callbacks while an outer guard is still alive. The last guard removes the entry.
    static PROGRAMMATIC_UPDATES: RefCell<HashMap<usize, usize>> = RefCell::new(HashMap::new());
}

/// The lifecycle of one mouse-down interaction, ending when AppKit tracking returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InteractionState {
    /// Tracking a press that may still become a knob drag.
    Pressed,
    /// Tracking the knob, including its final position on mouse-up.
    Dragging,
    /// Input is permanently canceled for this interaction, but the view is retained.
    Canceled,
    /// Input is canceled and the view must be detached when tracking returns.
    Retired,
}

struct Interaction {
    target: SharedScrollbarState,
    state: InteractionState,
}

impl Interaction {
    fn new(target: SharedScrollbarState, dragging_knob: bool) -> Self {
        let state =
            if dragging_knob { InteractionState::Dragging } else { InteractionState::Pressed };
        Self { target, state }
    }

    fn target(&mut self, current: Option<&SharedScrollbarState>) -> Option<SharedScrollbarState> {
        if !current.is_some_and(|current| current.same_instance(&self.target)) {
            self.cancel();
        }

        match self.state {
            InteractionState::Pressed | InteractionState::Dragging => Some(self.target.clone()),
            InteractionState::Canceled | InteractionState::Retired => None,
        }
    }

    fn is_dragging_knob(&self) -> bool {
        self.state == InteractionState::Dragging
    }

    fn track_knob(&mut self) {
        if self.state == InteractionState::Pressed {
            self.state = InteractionState::Dragging;
        }
    }

    fn cancel(&mut self) {
        self.state = match self.state {
            InteractionState::Pressed | InteractionState::Dragging => InteractionState::Canceled,
            InteractionState::Canceled | InteractionState::Retired => self.state,
        };
    }

    fn retire(&mut self) {
        self.state = InteractionState::Retired;
    }
}

/// Keeps the mouse-down target and view alive through appkit nested tracking loop.
///
/// A buffer replacement gets a different callback handle so hiding or retiring the
/// control cancels this interaction permanently, even if a later redraw shows it
/// again before the mouse-up. A retired view is detached only after tracking returns.
pub struct ScrollbarTracking {
    scroller: Retained<NSScroller>,
}

impl ScrollbarTracking {
    pub fn begin(scroller: &NSScroller, dragging_knob: bool) -> Option<Self> {
        if scroller.isHidden() || !scroller.isEnabled() {
            return None;
        }

        let target = shared_state(scroller)?;
        target.snapshot()?;
        let key = scrollbar_key(scroller);
        let tracking = INTERACTIONS.with(|cell| {
            let mut interactions = cell.borrow_mut();
            if interactions.contains_key(&key) {
                return None;
            }
            interactions.insert(key, Interaction::new(target, dragging_knob));
            Some(Self { scroller: Retained::from(scroller) })
        })?;

        if dragging_knob {
            request_display(scroller);
        }

        Some(tracking)
    }
}

impl Drop for ScrollbarTracking {
    fn drop(&mut self) {
        let interaction =
            INTERACTIONS.with(|cell| cell.borrow_mut().remove(&scrollbar_key(&self.scroller)));

        if let Some(interaction) = interaction {
            match interaction.state {
                InteractionState::Retired => self.scroller.removeFromSuperview(),
                InteractionState::Dragging => request_display(&self.scroller),
                InteractionState::Pressed | InteractionState::Canceled => {}
            }
        }
    }
}

pub fn is_dragging_knob(scroller: &NSScroller) -> bool {
    INTERACTIONS.with(|cell| {
        cell.borrow().get(&scrollbar_key(scroller)).is_some_and(Interaction::is_dragging_knob)
    })
}

/// Returns appkit normalized value while the knob is being dragged.
///
/// The knob is inset from the slot, so reconstructing this value from their
/// rectangles moves the viewport away from the endpoints on mouse-up.
pub fn drag_value(scroller: &NSScroller) -> Option<f64> {
    is_dragging_knob(scroller).then(|| scroller.doubleValue())
}

/// AppKit can turn a slot click into knob tracking after mouse-down.
pub fn track_knob(scroller: &NSScroller) {
    update_interaction(scroller, Interaction::track_knob);
}

pub fn cancel_tracking(scroller: &NSScroller) {
    update_interaction(scroller, Interaction::cancel);
}

/// Returns whether an active tracking guard will detach the retired view later.
pub fn retire_tracking(scroller: &NSScroller) -> bool {
    update_interaction(scroller, Interaction::retire).is_some()
}

/// Reads live viewport data without allowing a tracking session to change targets.
pub fn input_state(scroller: &NSScroller) -> Option<ScrollbarState> {
    if callbacks_suppressed(scrollbar_key(scroller)) {
        return None;
    }

    if scroller.isHidden() || !scroller.isEnabled() {
        cancel_tracking(scroller);
        return None;
    }

    let current = shared_state(scroller);
    let target = update_interaction(scroller, |interaction| interaction.target(current.as_ref()))
        .unwrap_or(current)?;

    let state = target.snapshot();
    if state.is_none() {
        cancel_tracking(scroller);
    }

    state
}

fn update_interaction<R>(
    scroller: &NSScroller,
    update: impl FnOnce(&mut Interaction) -> R,
) -> Option<R> {
    let (result, changed) = INTERACTIONS.with(|cell| {
        let mut interactions = cell.borrow_mut();
        let interaction = interactions.get_mut(&scrollbar_key(scroller))?;
        let was_dragging = interaction.is_dragging_knob();
        let result = update(interaction);
        Some((result, was_dragging != interaction.is_dragging_knob()))
    })?;

    if changed {
        request_display(scroller);
    }

    Some(result)
}

/// Suppresses callbacks from setters, including nested AppKit setter calls.
pub fn with_programmatic_update<R>(scroller: &NSScroller, update: impl FnOnce() -> R) -> R {
    let _ = ProgrammaticUpdate::begin(scrollbar_key(scroller));
    update()
}

fn callbacks_suppressed(key: usize) -> bool {
    PROGRAMMATIC_UPDATES.with(|cell| cell.borrow().contains_key(&key))
}

struct ProgrammaticUpdate(usize);
impl ProgrammaticUpdate {
    fn begin(key: usize) -> Self {
        PROGRAMMATIC_UPDATES.with(|cell| *cell.borrow_mut().entry(key).or_default() += 1);
        Self(key)
    }
}

impl Drop for ProgrammaticUpdate {
    fn drop(&mut self) {
        PROGRAMMATIC_UPDATES.with(|cell| {
            let mut updates = cell.borrow_mut();
            if let Some(depth) = updates.get_mut(&self.0) {
                *depth -= 1;
                if *depth == 0 {
                    updates.remove(&self.0);
                }
            }
        });
    }
}
