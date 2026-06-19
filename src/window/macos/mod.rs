pub mod hotkey;
pub mod tab_navigation;

pub use crate::platform::macos::{
    EditorSwitcherRow, MacosWindowFeature, TouchpadStage, close_editor_switcher_if_open,
    editor_switcher_key_event_matches, hide_application, native_tab_bar_enabled,
    register_file_handler, show_editor_switcher_panel,
};
