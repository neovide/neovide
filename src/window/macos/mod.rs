pub mod hotkey;
pub mod tab_navigation;

pub use crate::platform::macos::EditorSwitcherRow;
pub use crate::platform::macos::close_editor_switcher_if_open;
pub use crate::platform::macos::editor_switcher_key_event_matches;
pub use crate::platform::macos::hide_application;
pub use crate::platform::macos::native_tab_bar_enabled;
pub use crate::platform::macos::register_file_handler;
pub use crate::platform::macos::show_editor_switcher_panel;
