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
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            transparency: 1.0,
            fullscreen: false,
            iso_layout: false,
            refresh_rate: 60,
            no_idle: SETTINGS.get::<CmdLineSettings>().no_idle,
            remember_window_size: false,
            remember_window_position: false,
            hide_mouse_when_typing: false,
        }
    }
}

#[derive(Clone, Default, SettingGroup)]
#[setting_prefix = "input"]
pub struct KeyboardSettings {
    pub use_logo: bool,
}
