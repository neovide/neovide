pub mod hotkey;
pub mod tab_navigation;

pub use crate::platform::macos::{
    get_last_host_window, get_ns_window, hide_application, is_focus_suppressed,
    is_tab_overview_active, native_tab_bar_enabled, register_file_handler, trigger_tab_overview,
    window_identifier, MacosWindowFeature, TouchpadStage,
};
