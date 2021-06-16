use crate::{cmd_line::CmdLineSettings, settings::*};

pub use super::keyboard::KeyboardSettings;

#[derive(Clone, SettingGroup)]
pub struct WindowSettings {
    pub refresh_rate: u64,
    pub no_idle: bool,
    pub transparency: f32,
    pub fullscreen: bool,
    pub iso_layout: bool,
    pub scroll_dead_zone: f32,
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            transparency: 1.0,
            fullscreen: false,
            iso_layout: false,
            refresh_rate: 60,
            no_idle: SETTINGS
                .get::<CmdLineSettings>()
                .neovim_args
                .contains(&String::from("--noIdle")),
            scroll_dead_zone: 0.0,
        }
    }
}
