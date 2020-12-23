use crate::settings::*;
use super::keyboard::initialize_settings as keyboard_initialize_settings;

#[derive(Clone)]
pub struct WindowSettings {
    pub refresh_rate: u64,
    pub transparency: f32,
    pub no_idle: bool,
    pub fullscreen: bool,
}

pub fn initialize_settings() {
    let no_idle = SETTINGS
        .neovim_arguments
        .contains(&String::from("--noIdle"));

    SETTINGS.set(&WindowSettings {
        refresh_rate: 60,
        transparency: 1.0,
        no_idle,
        fullscreen: false,
    });

    register_nvim_setting!("refresh_rate", WindowSettings::refresh_rate);
    register_nvim_setting!("transparency", WindowSettings::transparency);
    register_nvim_setting!("no_idle", WindowSettings::no_idle);
    register_nvim_setting!("fullscreen", WindowSettings::fullscreen);

    keyboard_initialize_settings();
}
