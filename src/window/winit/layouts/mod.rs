mod qwerty;

use log::trace;
use skulpin::winit::event::ModifiersState;
use skulpin::winit::event::VirtualKeyCode as Keycode;

use super::keyboard::*;
use crate::settings::*;
use qwerty::*;

pub fn unsupported_key<R>(keycode: Keycode) -> Option<R> {
    trace!("Unsupported key: {:?}", keycode);
    None
}

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
