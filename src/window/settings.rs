use crate::{cmd_line::CmdLineSettings, settings::*};

#[derive(Clone, SettingGroup)]
pub struct WindowSettings {
    pub refresh_rate: u64,
    pub refresh_rate_idle: u64,
    pub no_idle: bool,
    pub transparency: f32,
    pub scale_factor: f32,
    pub fullscreen: bool,
    pub iso_layout: bool,
    pub remember_window_size: bool,
    pub remember_window_position: bool,
    pub hide_mouse_when_typing: bool,
    pub touch_deadzone: f32,
    pub touch_drag_timeout: f32,
    pub background_color: String,
    pub confirm_quit: bool,
    pub font_edging: String,
    pub font_hinting: String,
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            transparency: 1.0,
            scale_factor: 1.0,
            fullscreen: false,
            iso_layout: false,
            refresh_rate: 60,
            refresh_rate_idle: 5,
            no_idle: SETTINGS.get::<CmdLineSettings>().no_idle,
            remember_window_size: true,
            remember_window_position: true,
            hide_mouse_when_typing: false,
            touch_deadzone: 6.0,
            touch_drag_timeout: 0.17,
            background_color: "".to_string(),
            confirm_quit: true,
            font_edging: "antialias".to_string(),
            font_hinting: "full".to_string(),
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
