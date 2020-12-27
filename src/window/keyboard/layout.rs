use crate::settings::{FromValue, Value};

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

impl From<KeyboardLayout> for Value {
    fn from(layout: KeyboardLayout) -> Self {
        match layout {
            KeyboardLayout::Qwerty => "qwerty".into(),
        }
    }
}
