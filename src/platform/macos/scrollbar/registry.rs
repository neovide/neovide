use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
};

use objc2_app_kit::NSScroller;

use super::appearance::ScrollbarColors;
use super::interaction::cancel_tracking;
use super::state::{ScrollbarState, SharedScrollbarState};

static SCROLLBAR_STATES: LazyLock<Mutex<HashMap<usize, SharedScrollbarState>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static SCROLLBAR_COLORS: LazyLock<Mutex<HashMap<usize, ScrollbarColors>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn scrollbar_key(scroller: &NSScroller) -> usize {
    scroller as *const NSScroller as usize
}

pub fn scrollbar_state(scroller: &NSScroller) -> Option<ScrollbarState> {
    shared_state(scroller)?.snapshot()
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
    if let Some(shared) = shared_state(scroller) {
        shared.update_pending_scroll(target_top);
    }
}
