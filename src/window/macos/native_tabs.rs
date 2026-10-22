use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

use objc2_app_kit::NSWindow;

use crate::window::MacosWindowFeature;

pub(crate) static SHOW_NATIVE_TAB_BAR: AtomicBool = AtomicBool::new(false);
pub(crate) static ENABLE_NATIVE_TAB_BAR: AtomicBool = AtomicBool::new(false);

fn native_tabs_enabled() -> bool { ENABLE_NATIVE_TAB_BAR.load(Ordering::Relaxed) }

pub(crate) fn should_show_native_tab_bar() -> bool {
    native_tabs_enabled() && SHOW_NATIVE_TAB_BAR.load(Ordering::Relaxed)
}

pub(crate) fn native_tab_bar_enabled() -> bool { native_tabs_enabled() }

pub(crate) fn merge_all_windows_if_native_tabs(ns_window: &NSWindow) {
    if should_show_native_tab_bar() {
        ns_window.mergeAllWindows(None);
        MacosWindowFeature::apply_tab_bar_preference(ns_window);
    }
}
