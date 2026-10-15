use std::{cell::RefCell, collections::HashMap, sync::Arc};

use objc2::rc::Retained;
use objc2_app_kit::NSScroller;

use super::{
    registry::{scrollbar_key, shared_state},
    state::{ScrollbarState, SharedScrollbarState},
};

thread_local! {
    static INTERACTIONS: RefCell<HashMap<usize, Interaction>> = RefCell::new(HashMap::new());
    static PROGRAMMATIC_UPDATES: RefCell<HashMap<usize, usize>> = RefCell::new(HashMap::new());
}

struct Interaction {
    target: SharedScrollbarState,
    dragging_knob: bool,
    canceled: bool,
    remove_on_finish: bool,
}

impl Interaction {
    fn new(target: SharedScrollbarState, dragging_knob: bool) -> Self {
        Self { target, dragging_knob, canceled: false, remove_on_finish: false }
    }

    fn target(&mut self, current: Option<&SharedScrollbarState>) -> Option<SharedScrollbarState> {
        if !current.is_some_and(|current| Arc::ptr_eq(current, &self.target)) {
            self.canceled = true;
        }

        (!self.canceled).then(|| self.target.clone())
    }

    fn is_dragging_knob(&self) -> bool {
        self.dragging_knob && !self.canceled
    }

    fn track_knob(&mut self) {
        if !self.canceled {
            self.dragging_knob = true;
        }
    }

    fn retire(&mut self) {
        self.canceled = true;
        self.remove_on_finish = true;
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
        target.lock().unwrap().as_ref()?;
        let key = scrollbar_key(scroller);
        INTERACTIONS.with(|cell| {
            let mut interactions = cell.borrow_mut();
            if interactions.contains_key(&key) {
                return None;
            }
            interactions.insert(key, Interaction::new(target, dragging_knob));
            Some(Self { scroller: Retained::from(scroller) })
        })
    }
}

impl Drop for ScrollbarTracking {
    fn drop(&mut self) {
        let interaction =
            INTERACTIONS.with(|cell| cell.borrow_mut().remove(&scrollbar_key(&self.scroller)));
        if interaction.is_some_and(|interaction| interaction.remove_on_finish) {
            self.scroller.removeFromSuperview();
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
    INTERACTIONS.with(|cell| {
        if let Some(interaction) = cell.borrow_mut().get_mut(&scrollbar_key(scroller)) {
            interaction.track_knob();
        }
    });
}

pub fn cancel_tracking(scroller: &NSScroller) {
    INTERACTIONS.with(|cell| {
        if let Some(interaction) = cell.borrow_mut().get_mut(&scrollbar_key(scroller)) {
            interaction.canceled = true;
        }
    });
}

/// Returns whether an active tracking guard will detach the retired view later.
pub fn retire_tracking(scroller: &NSScroller) -> bool {
    INTERACTIONS.with(|cell| {
        let mut interactions = cell.borrow_mut();
        let Some(interaction) = interactions.get_mut(&scrollbar_key(scroller)) else {
            return false;
        };
        interaction.retire();
        true
    })
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
    let target =
        INTERACTIONS.with(|cell| match cell.borrow_mut().get_mut(&scrollbar_key(scroller)) {
            Some(interaction) => interaction.target(current.as_ref()),
            None => current,
        })?;

    let state = target.lock().unwrap().clone();
    if state.is_none() {
        cancel_tracking(scroller);
    }

    state
}

/// Suppresses callbacks from setters, including nested AppKit setter calls.
pub fn with_programmatic_update<R>(scroller: &NSScroller, update: impl FnOnce() -> R) -> R {
    let _guard = ProgrammaticUpdate::begin(scrollbar_key(scroller));
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
