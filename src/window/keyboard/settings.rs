use super::KeyboardLayout;
use crate::settings::FromValue;

#[derive(Clone, SettingGroup)]
#[setting_prefix = "keyboard"]
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
