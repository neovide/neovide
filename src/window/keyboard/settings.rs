use super::KeyboardLayout;
use crate::{
    register_nvim_setting,
    settings::{FromValue, Value, SETTINGS},
};

#[derive(Clone)]
pub struct KeyboardSettings {
    pub layout: KeyboardLayout,
}

pub fn initialize_settings() {
    SETTINGS.set(&KeyboardSettings {
        layout: KeyboardLayout::Qwerty,
    });
    register_nvim_setting!("keyboard_layout", KeyboardSettings::layout);
}
