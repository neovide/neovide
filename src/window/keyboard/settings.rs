use super::KeyboardLayout;
use crate::settings::FromValue;

#[setting_prefix = "keyboard"]
#[derive(Clone, SettingGroup)]
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
