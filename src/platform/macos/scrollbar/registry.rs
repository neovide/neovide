use std::{
    cell::RefCell,
    collections::HashMap,
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

use objc2_app_kit::NSScroller;

use super::appearance::ScrollbarColors;
use super::interaction::cancel_tracking;
use super::state::{ScrollbarState, SharedScrollbarState};

thread_local! {
    static SCROLLBAR_USER_INPUT: RefCell<Option<HashMap<usize, Instant>>> =
        const { RefCell::new(None) };
}

static SCROLLBAR_STATES: LazyLock<Mutex<HashMap<usize, SharedScrollbarState>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static SCROLLBAR_COLORS: LazyLock<Mutex<HashMap<usize, ScrollbarColors>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn scrollbar_key(scroller: &NSScroller) -> usize {
    scroller as *const NSScroller as usize
}

pub fn mark_user_input(scroller: &NSScroller, duration: Duration) {
    let key = scrollbar_key(scroller);
    let deadline = Instant::now() + duration;
    SCROLLBAR_USER_INPUT.with(|cell| {
        let mut guard = cell.borrow_mut();
        let map = guard.get_or_insert_with(HashMap::new);
        map.insert(key, deadline);
    });
}

pub fn scrollbar_state(scroller: &NSScroller) -> Option<ScrollbarState> {
    let shared = shared_state(scroller)?;
    shared.lock().unwrap().clone()
}

pub fn shared_state(scroller: &NSScroller) -> Option<SharedScrollbarState> {
    SCROLLBAR_STATES.lock().unwrap().get(&scrollbar_key(scroller)).cloned()
}

pub fn is_control(scroller: &NSScroller) -> bool {
    SCROLLBAR_STATES.lock().unwrap().contains_key(&scrollbar_key(scroller))
}

pub fn register(scroller: &NSScroller, state: SharedScrollbarState) {
    SCROLLBAR_STATES.lock().unwrap().insert(scrollbar_key(scroller), state);
}

pub fn unregister(scroller: &NSScroller) {
    cancel_tracking(scroller);
    remove_entry(scrollbar_key(scroller));
}

fn remove_entry(key: usize) {
    SCROLLBAR_STATES.lock().unwrap().remove(&key);
    SCROLLBAR_COLORS.lock().unwrap().remove(&key);
    SCROLLBAR_USER_INPUT.with(|cell| {
        if let Some(map) = cell.borrow_mut().as_mut() {
            map.remove(&key);
        }
    });
}

pub fn scrollbar_colors(scroller: &NSScroller) -> Option<ScrollbarColors> {
    let colors = SCROLLBAR_COLORS.lock().unwrap();
    colors.get(&scrollbar_key(scroller)).copied()
}

pub fn set_scrollbar_colors(scroller: &NSScroller, colors: Option<ScrollbarColors>) {
    let mut stored_colors = SCROLLBAR_COLORS.lock().unwrap();
    let key = scrollbar_key(scroller);
    if let Some(colors) = colors {
        stored_colors.insert(key, colors);
    } else {
        stored_colors.remove(&key);
    }
}

pub fn update_pending_scroll(scroller: &NSScroller, target_top: f64) {
    let Some(shared) = shared_state(scroller) else {
        return;
    };

    let mut guard = shared.lock().unwrap();
    let Some(state) = guard.as_mut() else {
        return;
    };

    let max_top = (state.line_count - state.visible_lines).max(0.0);
    let target_top = target_top.clamp(0.0, max_top);
    let previous_top = state.top_line.clamp(0.0, max_top);
    let delta = target_top - previous_top;

    state.top_line = target_top;

    let max_current_line = (state.line_count - 1.0).max(0.0);
    state.current_line = (state.current_line + delta).clamp(0.0, max_current_line);
}
