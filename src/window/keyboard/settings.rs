use super::KeyboardLayout;
use crate::settings::FromValue;

#[derive(SettingGroup)]
#[setting_prefix = "keyboard"]
#[derive(Clone)]
pub struct KeyboardSettings {
    pub layout: KeyboardLayout,
}

impl Default for KeyboardSettings {
    fn default() -> Self {
        Self {
            layout: KeyboardLayout::Qwerty,
        }
    }
}
