use crate::settings::*;

#[derive(Clone)]
pub enum KeyboardLayout {
    Qwerty,
}

impl FromValue for KeyboardLayout {
    fn from_value(&mut self, value: Value) {
        match value.as_str() {
            Some("qwerty") => *self = KeyboardLayout::Qwerty,
            _ => log::error!(
                "keyboard_layout setting expected a known keyboard layout name, but received: {}",
                value
            ),
        }
    }
}

impl Into<Value> for KeyboardLayout {
    fn into(self) -> Value {
        match self {
            KeyboardLayout::Qwerty => "qwerty".into(),
        }
    }
}

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
