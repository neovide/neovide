use crate::{cmd_line::CmdLineSettings, settings::*};

#[derive(Clone, SettingGroup)]
pub struct WindowSettings {
    pub background_color: String,
    pub confirm_quit: bool,
    pub fullscreen: bool,
    pub hide_mouse_when_typing: bool,
    pub idle: bool,
    pub iso_layout: bool,
    pub os_blur: bool,
    pub padding_bottom: u32,
    pub padding_left: u32,
    pub padding_right: u32,
    pub padding_top: u32,
    pub refresh_rate: u64,
    pub refresh_rate_idle: u64,
    pub remember_window_position: bool,
    pub remember_window_size: bool,
    pub scale_factor: f32,
    pub theme: String,
    pub touch_deadzone: f32,
    pub touch_drag_timeout: f32,
    pub transparency: f32,
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            background_color: "".to_string(),
            confirm_quit: true,
            fullscreen: false,
            hide_mouse_when_typing: false,
            idle: SETTINGS.get::<CmdLineSettings>().idle,
            iso_layout: false,
            os_blur: false,
            padding_bottom: 0,
            padding_left: 0,
            padding_right: 0,
            padding_top: 0,
            refresh_rate: 60,
            refresh_rate_idle: 5,
            remember_window_position: true,
            remember_window_size: true,
            scale_factor: 1.0,
            theme: "".to_string(),
            touch_deadzone: 6.0,
            touch_drag_timeout: 0.17,
            transparency: 1.0,
        }
    }
}

#[derive(Clone, SettingGroup)]
#[setting_prefix = "input"]
pub struct KeyboardSettings {
    pub macos_alt_is_meta: bool,
    pub ime: bool,
}

#[allow(clippy::derivable_impls)]
impl Default for KeyboardSettings {
    fn default() -> Self {
        Self {
            macos_alt_is_meta: false,
            ime: true,
        }
    }
}
