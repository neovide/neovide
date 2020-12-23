mod qwerty;

use log::trace;
use skulpin::sdl2::keyboard::{Keycode, Mod};

use qwerty::*;
use super::keyboard::*;
use crate::settings::*;

pub fn unsupported_key<R>(keycode: Keycode) -> Option<R> {
    trace!("Unsupported key: {:?}", keycode);
    None
}

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
