use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

pub static SHOW_NATIVE_TAB_BAR: AtomicBool = AtomicBool::new(false);
pub static ENABLE_NATIVE_TAB_BAR: AtomicBool = AtomicBool::new(false);

fn native_tabs_enabled() -> bool { ENABLE_NATIVE_TAB_BAR.load(Ordering::Relaxed) }

pub fn should_show_native_tab_bar() -> bool {
    native_tabs_enabled() && SHOW_NATIVE_TAB_BAR.load(Ordering::Relaxed)
}

pub fn native_tab_bar_enabled() -> bool { native_tabs_enabled() }
