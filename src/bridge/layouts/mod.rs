mod keypress;
mod modifiers;
#[cfg_attr(feature = "sdl2", path = "sdl2.rs")]
#[cfg_attr(feature = "winit", path = "winit.rs")]
mod qwerty;

use keypress::Keypress;
use modifiers::Modifiers;

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
fn unsupported_key<R>(keycode: Keycode) -> Option<R> {
    trace!("Unsupported key: {:?}", keycode);
    None
}

/// The keyboard layout used for input.
#[derive(Clone)]
enum KeyboardLayout {
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

/// Sets up Neovim settings related to keyboard layout.
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
    let control = modifiers.contains(Mod::LCTRLMOD) || modifiers.contains(Mod::RCTRLMOD);
    let meta = modifiers.contains(Mod::LALTMOD) || modifiers.contains(Mod::RALTMOD);
    let logo = modifiers.contains(Mod::LGUIMOD) || modifiers.contains(Mod::RGUIMOD);
    let mods = Modifiers::new(shift, control, meta, logo);
    produce_neovim_keybinding_string_shared(keycode, keytext, mods)
}

#[cfg(feature = "winit")]
pub fn produce_neovim_keybinding_string(
    keycode: Option<Keycode>,
    keytext: Option<String>,
    modifiers: Option<ModifiersState>,
) -> Option<String> {
    let mods = if let Some(modifiers) = modifiers {
        Modifiers::new(
            modifiers.shift(),
            modifiers.ctrl(),
            modifiers.alt(),
            modifiers.logo(),
        )
    } else {
        Modifiers::new(false, false, false, false)
    };
    produce_neovim_keybinding_string_shared(keycode, keytext, mods)
}

fn produce_neovim_keybinding_string_shared(
    keycode: Option<Keycode>,
    keytext: Option<String>,
    mods: Modifiers,
) -> Option<String> {
    if let Some(text) = keytext {
        Some(
            if text == "<" {
                Keypress::new("lt", true, true)
            } else {
                Keypress::new(&text, false, false)
            }
            .as_token(mods),
        )
    } else if let Some(keycode) = keycode {
        match SETTINGS.get::<KeyboardSettings>().layout {
            KeyboardLayout::Qwerty => handle_qwerty_layout(keycode, mods.shift),
        }
        .map(|e| e.as_token(mods))
    } else {
        None
    }
}
