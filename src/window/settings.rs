use crate::{cmd_line::CmdLineSettings, settings::*};

#[derive(Clone, SettingGroup)]
pub struct WindowSettings {
    pub refresh_rate: u64,
    pub no_idle: bool,
    pub transparency: f32,
    pub fullscreen: bool,
    pub iso_layout: bool,
    pub remember_window_size: bool,
    pub remember_window_position: bool,
    pub hide_mouse_when_typing: bool,
    pub touch_deadzone: f32,
    pub touch_drag_timeout: f32,
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            transparency: 1.0,
            fullscreen: false,
            iso_layout: false,
            refresh_rate: 60,
            no_idle: SETTINGS.get::<CmdLineSettings>().no_idle,
            remember_window_size: true,
            remember_window_position: true,
            hide_mouse_when_typing: false,
            touch_deadzone: 6.0,
            touch_drag_timeout: 0.17,
        }
    }
}

#[derive(Clone, SettingGroup)]
#[setting_prefix = "input"]
pub struct KeyboardSettings {
    pub use_logo: bool,
    pub macos_alt_is_meta: bool,
}

impl Default for KeyboardSettings {
    fn default() -> Self {
        Self {
            use_logo: cfg!(target_os = "macos"),
            macos_alt_is_meta: false,
        }
    }
}
