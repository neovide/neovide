//! TODO: We should move this shared state to window/wrapper/macos/tabs later.

use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

pub static SHOW_SYSTEM_TAB_BAR: AtomicBool = AtomicBool::new(false);
pub static ENABLE_SYSTEM_TAB_BAR: AtomicBool = AtomicBool::new(false);

fn system_tabs_enabled() -> bool { ENABLE_SYSTEM_TAB_BAR.load(Ordering::Relaxed) }

pub fn should_show_system_tab_bar() -> bool {
    system_tabs_enabled() && SHOW_SYSTEM_TAB_BAR.load(Ordering::Relaxed)
}

pub fn system_tab_bar_enabled() -> bool { system_tabs_enabled() }
