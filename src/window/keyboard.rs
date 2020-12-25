use log::error;

use crate::settings::*;

#[derive(Clone)]
pub enum KeyboardLayout {
    Qwerty,
}

impl FromValue for KeyboardLayout {
    fn from_value(&mut self, value: Value) {
        match value.as_str() {
            Some("qwerty") => *self = KeyboardLayout::Qwerty,
            _ => error!(
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

pub fn append_modifiers(
    keycode_text: &str,
    special: bool,
    shift: bool,
    ctrl: bool,
    alt: bool,
    gui: bool,
) -> String {
    let mut result = keycode_text.to_string();
    let mut special = if result == "<" {
        result = "lt".to_string();
        true
    } else {
        special
    };

    if shift {
        special = true;
        result = format!("S-{}", result);
    }
    if ctrl {
        special = true;
        result = format!("C-{}", result);
    }
    if alt {
        special = true;
        result = format!("M-{}", result);
    }
    if cfg!(not(target_os = "windows")) && gui {
        special = true;
        result = format!("D-{}", result);
    }

    if special {
        result = format!("<{}>", result);
    }

    result
}
