use winit::event::WindowEvent;
use winit::window::WindowId;

use super::ShortcutHandling;
use super::WindowSettingsChanged;
use super::WinitWindowWrapper;

impl WinitWindowWrapper {
    pub(super) fn handle_platform_window_settings_changed(
        &mut self,
        _window_ids: &[WindowId],
        _changed_setting: WindowSettingsChanged,
    ) {
    }

    pub(super) fn handle_platform_shortcuts(
        &mut self,
        _window_id: WindowId,
        _event: &WindowEvent,
    ) -> ShortcutHandling {
        ShortcutHandling::default()
    }
}
