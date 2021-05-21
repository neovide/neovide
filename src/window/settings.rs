use crate::settings::*;

pub use super::keyboard::KeyboardSettings;

#[derive(Clone, SettingGroup)]
pub struct WindowSettings {
    pub refresh_rate: u64,
    pub no_idle: bool,
    pub transparency: f32,
    pub fullscreen: bool,
    pub iso_layout: bool,
    pub remember_dimension: bool,
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            transparency: 1.0,
            fullscreen: false,
            iso_layout: false,
            refresh_rate: 60,
            no_idle: SETTINGS
                .neovim_arguments
                .contains(&String::from("--noIdle")),
            remember_dimension: false,
        }
    }
}
