use std::sync::Arc;

use crate::{
    bridge::{send_ui, SerialCommand},
    settings::Settings,
};

#[allow(unused_imports)]
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
use winit::{
    event::{ElementState, Ime, KeyEvent, Modifiers, WindowEvent},
    keyboard::{Key, KeyCode, KeyLocation, NamedKey, PhysicalKey, ModifiersKeyState},
};
#[cfg(target_os = "macos")]
use crate::{
    platform::macos::keyboard,
    window::settings::{OptionAsMeta, WindowSettings},
};
use crate::profiling::tracy_named_frame;

fn is_ascii_alphabetic_char(text: &str) -> bool {
    text.len() == 1 && text.chars().next().unwrap().is_ascii_alphabetic()
}



pub struct KeyboardManager {
    modifiers: Modifiers,
    ime_preedit: (String, Option<(usize, usize)>),
    meta_is_pressed: bool, // see note on 'meta' below
    #[allow(dead_code)]
    settings: Arc<Settings>,
}

impl KeyboardManager {
    pub fn new(settings: Arc<Settings>) -> Self {
        KeyboardManager {
            modifiers: Modifiers::default(),
            ime_preedit: ("".to_string(), None),
            meta_is_pressed: false,
            settings,
        }
    }

    pub fn handle_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::KeyboardInput {
                event: key_event,
                is_synthetic: false,
                ..
            } if self.ime_preedit.0.is_empty() => {
                log::trace!("{key_event:#?}");
                if key_event.state == ElementState::Pressed {
                    if let Some(text) = self.format_key(key_event) {
                        log::trace!("Key pressed {} {:?}", text, self.modifiers.state());
                        tracy_named_frame!("keyboard input");
                        send_ui(SerialCommand::Keyboard(text));
                    }
                }
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                log::trace!("Ime commit {text}");
                send_ui(SerialCommand::Keyboard(self.format_key_text(text, false)));
            }
            WindowEvent::Ime(Ime::Preedit(text, cursor_offset)) => {
                self.ime_preedit = (text.to_string(), *cursor_offset)
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                // Record the modifier states so that we can properly add them to the keybinding text
                log::trace!("{:?}", *modifiers);
                self.modifiers = *modifiers;

                #[cfg(target_os = "macos")]
                {
                    let ws = self.settings.get::<WindowSettings>();
                    self.meta_is_pressed = match ws.input_macos_option_key_is_meta {
                        OptionAsMeta::Both => self.modifiers.state().alt_key(),
                        OptionAsMeta::OnlyLeft => {
                            self.modifiers.lalt_state() == ModifiersKeyState::Pressed
                        }
                        OptionAsMeta::OnlyRight => {
                            self.modifiers.ralt_state() == ModifiersKeyState::Pressed
                        }
                        OptionAsMeta::None => false,
                    };
                }

                #[cfg(not(target_os = "macos"))]
                {
                    self.meta_is_pressed = self.modifiers.state().alt_key();
                }
            }
            _ => {}
        }
    }

    fn handle_numpad_numkey<'a>(
        is_numlock_enabled: bool,
        numlock_str: &'a str,
        non_numlock_str: &'a str,
    ) -> Option<&'a str> {
        if is_numlock_enabled {
            return Some(numlock_str);
        }
        Some(non_numlock_str)
    }

    fn handle_numpad_key(key_event: &KeyEvent) -> Option<&str> {
        let PhysicalKey::Code(physical_key_code) = key_event.physical_key else {
            return None;
        };
        let is_numlock_key = key_event.text.is_some();
        match physical_key_code {
            KeyCode::NumpadDivide => Some("kDivide"),
            KeyCode::NumpadStar => Some("kMultiply"),
            KeyCode::NumpadSubtract => Some("kMinus"),
            KeyCode::NumpadAdd => Some("kPlus"),
            KeyCode::NumpadEnter => Some("kEnter"),
            KeyCode::NumpadEqual => Some("kEqual"),
            KeyCode::NumpadComma => match key_event.logical_key.as_ref() {
                Key::Character(",") => Some("kComma"),
                Key::Character(".") => Some("kPoint"),
                _ => None,
            },
            KeyCode::NumpadDecimal => {
                if is_numlock_key {
                    match key_event.logical_key.as_ref() {
                        Key::Character(",") => Some("kComma"),
                        Key::Character(".") => Some("kPoint"),
                        _ => None,
                    }
                } else {
                    Some("kDel")
                }
            }
            KeyCode::Numpad9 => {
                KeyboardManager::handle_numpad_numkey(is_numlock_key, "k9", "kPageUp")
            }
            KeyCode::Numpad8 => KeyboardManager::handle_numpad_numkey(is_numlock_key, "k8", "kUp"),
            KeyCode::Numpad7 => {
                KeyboardManager::handle_numpad_numkey(is_numlock_key, "k7", "kHome")
            }
            KeyCode::Numpad6 => {
                KeyboardManager::handle_numpad_numkey(is_numlock_key, "k6", "kRight")
            }
            KeyCode::Numpad5 => {
                KeyboardManager::handle_numpad_numkey(is_numlock_key, "k5", "kOrigin")
            }
            KeyCode::Numpad4 => {
                KeyboardManager::handle_numpad_numkey(is_numlock_key, "k4", "kLeft")
            }
            KeyCode::Numpad3 => {
                KeyboardManager::handle_numpad_numkey(is_numlock_key, "k3", "kPageDown")
            }
            KeyCode::Numpad2 => {
                KeyboardManager::handle_numpad_numkey(is_numlock_key, "k2", "kDown")
            }
            KeyCode::Numpad1 => KeyboardManager::handle_numpad_numkey(is_numlock_key, "k1", "kEnd"),
            KeyCode::Numpad0 => {
                KeyboardManager::handle_numpad_numkey(is_numlock_key, "k0", "Insert")
            }
            _ => None,
        }
    }

    fn format_key(&self, key_event: &KeyEvent) -> Option<String> {
        if let Some(text) = get_special_key(key_event) {
            Some(self.format_key_text(text, true))
        } else {
            self.format_normal_key(key_event)
        }
    }

    fn format_normal_key(&self, key_event: &KeyEvent) -> Option<String> {
        key_event
            .text
            .as_ref()
            .or(match &key_event.logical_key {
                Key::Character(text) => Some(text),
                _ => None,
            })
            .map(|text| self.format_key_text(text.as_str(), false))
    }

    fn format_key_text(&self, text: &str, is_special: bool) -> String {
        // Neovim always converts shifted ascii alpha characters to uppercase, so do it here already
        // This fixes some bugs where winit does not report the uppercase text as it should
        let text = if self.modifiers.state().shift_key() && is_ascii_alphabetic_char(text) {
            text.to_uppercase()
        } else {
            text.to_string()
        };

        let modifiers = self.format_modifier_string(&text, is_special);
        // < needs to be formatted as a special character, but note that it's not treated as a
        // special key for the modifier formatting, so S- and -M are still potentially stripped
        let (text, is_special) = if text == "<" {
            ("lt".to_string(), true)
        } else {
            (text, is_special)
        };
        if modifiers.is_empty() {
            if is_special {
                format!("<{text}>")
            } else {
                text
            }
        } else {
            format!("<{modifiers}{text}>")
        }
    }

    pub fn format_modifier_string(&self, text: &str, is_special: bool) -> String {
        #[cfg(target_os = "macos")]
        return keyboard::format_modifier_string(
            &self.modifiers,
            self.meta_is_pressed,
            text,
            is_special,
        );

        #[cfg(not(target_os = "macos"))]
        {
            let state = self.modifiers.state();
            let include_shift =
                is_special || (state.control_key() && is_ascii_alphabetic_char(text));
            let have_meta = self.meta_is_pressed;

            let mut ret = String::new();
            (state.shift_key() && include_shift).then(|| ret += "S-");
            state.control_key().then(|| ret += "C-");
            (have_meta).then(|| ret += "M-");
            state.super_key().then(|| ret += "D-");
            ret
        }
    }
}

fn get_special_key(key_event: &KeyEvent) -> Option<&str> {
    if key_event.location == KeyLocation::Numpad {
        return KeyboardManager::handle_numpad_key(key_event);
    }
    let Key::Named(key) = &key_event.logical_key else {
        return None;
    };
    match key {
        NamedKey::ArrowDown => Some("Down"),
        NamedKey::ArrowLeft => Some("Left"),
        NamedKey::ArrowRight => Some("Right"),
        NamedKey::ArrowUp => Some("Up"),
        NamedKey::Backspace => Some("BS"),
        NamedKey::Delete => Some("Del"),
        NamedKey::End => Some("End"),
        NamedKey::Enter => Some("Enter"),
        NamedKey::Escape => Some("Esc"),
        NamedKey::F1 => Some("F1"),
        NamedKey::F2 => Some("F2"),
        NamedKey::F3 => Some("F3"),
        NamedKey::F4 => Some("F4"),
        NamedKey::F5 => Some("F5"),
        NamedKey::F6 => Some("F6"),
        NamedKey::F7 => Some("F7"),
        NamedKey::F8 => Some("F8"),
        NamedKey::F9 => Some("F9"),
        NamedKey::F10 => Some("F10"),
        NamedKey::F11 => Some("F11"),
        NamedKey::F12 => Some("F12"),
        NamedKey::F13 => Some("F13"),
        NamedKey::F14 => Some("F14"),
        NamedKey::F15 => Some("F15"),
        NamedKey::F16 => Some("F16"),
        NamedKey::F17 => Some("F17"),
        NamedKey::F18 => Some("F18"),
        NamedKey::F19 => Some("F19"),
        NamedKey::F20 => Some("F20"),
        NamedKey::F21 => Some("F21"),
        NamedKey::F22 => Some("F22"),
        NamedKey::F23 => Some("F23"),
        NamedKey::F24 => Some("F24"),
        NamedKey::F25 => Some("F25"),
        NamedKey::F26 => Some("F26"),
        NamedKey::F27 => Some("F27"),
        NamedKey::F28 => Some("F28"),
        NamedKey::F29 => Some("F29"),
        NamedKey::F30 => Some("F30"),
        NamedKey::F31 => Some("F31"),
        NamedKey::F32 => Some("F32"),
        NamedKey::F33 => Some("F33"),
        NamedKey::F34 => Some("F34"),
        NamedKey::F35 => Some("F35"),
        NamedKey::Home => Some("Home"),
        NamedKey::Insert => Some("Insert"),
        NamedKey::PageDown => Some("PageDown"),
        NamedKey::PageUp => Some("PageUp"),
        NamedKey::Space => {
            // Space can finish a dead key sequence, so treat space as a special key only when
            // that doesn't happen.
            if key_event.text == Some(" ".into()) || key_event.text.is_none() {
                Some("Space")
            } else {
                None
            }
        }
        NamedKey::Tab => Some("Tab"),
        _ => None,
    }
}


//

//

//

