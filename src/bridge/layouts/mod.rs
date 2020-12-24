#[cfg_attr(feature = "sdl2", path = "sdl2.rs")]
#[cfg_attr(feature = "winit", path = "winit.rs")]
mod qwerty;
mod keypress;

use keypress::Keypress;

use log::{error, trace};

#[cfg(feature = "sdl2")]
use skulpin::sdl2::keyboard::{Keycode, Mod};

#[cfg(feature = "winit")]
use skulpin::winit::event::ModifiersState;
#[cfg(feature = "winit")]
use skulpin::winit::event::VirtualKeyCode as Keycode;

use crate::settings::{FromValue, Value, SETTINGS};

use qwerty::*;

/// Handler for noop keyboard events.
pub fn unsupported_key<R>(keycode: Keycode) -> Option<R> {
    trace!("Unsupported key: {:?}", keycode);
    None
}

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
struct KeyboardSettings {
    layout: KeyboardLayout,
}

pub fn initialize_settings() {
    SETTINGS.set(&KeyboardSettings {
        layout: KeyboardLayout::Qwerty,
    });

    register_nvim_setting!("keyboard_layout", KeyboardSettings::layout);
}

#[cfg(feature = "sdl2")]
pub fn produce_neovim_keybinding_string(
    keycode: Option<Keycode>,
    keytext: Option<String>,
    modifiers: Mod,
) -> Option<String> {
    let shift = modifiers.contains(Mod::LSHIFTMOD) || modifiers.contains(Mod::RSHIFTMOD);
    let ctrl = modifiers.contains(Mod::LCTRLMOD) || modifiers.contains(Mod::RCTRLMOD);
    let alt = modifiers.contains(Mod::LALTMOD) || modifiers.contains(Mod::RALTMOD);
    let gui = modifiers.contains(Mod::LGUIMOD) || modifiers.contains(Mod::RGUIMOD);
    if let Some(text) = keytext {
        Some(Keypress::new(&text, false, false, ctrl, alt).as_token(gui))
    } else if let Some(keycode) = keycode {
        (match SETTINGS.get::<KeyboardSettings>().layout {
            KeyboardLayout::Qwerty => handle_qwerty_layout(keycode, shift, ctrl, alt),
        })
        .map(|e| {
            e.as_token(gui)
        })
    } else {
        None
    }
}

#[cfg(feature = "winit")]
pub fn produce_neovim_keybinding_string(
    keycode: Option<Keycode>,
    keytext: Option<String>,
    modifiers: Option<ModifiersState>,
) -> Option<String> {
    let mut shift = false;
    let mut ctrl = false;
    let mut alt = false;
    let mut gui = false;
    if let Some(modifiers) = modifiers {
        shift = modifiers.shift();
        ctrl = modifiers.ctrl();
        alt = modifiers.alt();
        gui = modifiers.logo();
    }

    if let Some(text) = keytext {
        Some(append_modifiers(&text, false, false, ctrl, alt, gui))
    } else if let Some(keycode) = keycode {
        (match SETTINGS.get::<KeyboardSettings>().layout {
            KeyboardLayout::Qwerty => handle_qwerty_layout(keycode, shift, ctrl, alt),
        })
        .map(|(transformed_text, special, shift, ctrl, alt)| {
            append_modifiers(transformed_text, special, shift, ctrl, alt, gui)
        })
    } else {
        None
    }
}
