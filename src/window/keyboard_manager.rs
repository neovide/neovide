use crate::{
    bridge::{SerialCommand, UiCommand},
    event_aggregator::EVENT_AGGREGATOR,
};
#[cfg(target_os = "macos")]
use crate::{settings::SETTINGS, window::KeyboardSettings};
#[allow(unused_imports)]
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
use winit::{
    event::{ElementState, Event, Ime, KeyEvent, Modifiers, WindowEvent},
    keyboard::Key,
};

pub struct KeyboardManager {
    modifiers: Modifiers,
    ime_preedit: (String, Option<(usize, usize)>),
}

impl KeyboardManager {
    pub fn new() -> KeyboardManager {
        KeyboardManager {
            modifiers: Modifiers::default(),
            ime_preedit: ("".to_string(), None),
        }
    }

    pub fn handle_event(&mut self, event: &Event<()>) {
        match event {
            Event::WindowEvent {
                event:
                    WindowEvent::KeyboardInput {
                        event: key_event,
                        is_synthetic,
                        ..
                    },
                ..
            } if key_event.state == ElementState::Pressed
                && self.ime_preedit.0.is_empty()
                && !is_synthetic =>
            {
                if let Some(text) = self.format_key(key_event) {
                    log::trace!("Key pressed {} {:?}", text, self.modifiers.state());
                    EVENT_AGGREGATOR.send(UiCommand::Serial(SerialCommand::Keyboard(text)));
                }
            }
            Event::WindowEvent {
                event: WindowEvent::Ime(Ime::Commit(text)),
                ..
            } => {
                log::trace!("Ime commit {text}");
                EVENT_AGGREGATOR.send(UiCommand::Serial(SerialCommand::Keyboard(text.to_string())));
            }
            Event::WindowEvent {
                event: WindowEvent::Ime(Ime::Preedit(text, cursor_offset)),
                ..
            } => self.ime_preedit = (text.to_string(), *cursor_offset),
            Event::WindowEvent {
                event: WindowEvent::ModifiersChanged(modifiers),
                ..
            } => {
                // Record the modifier states so that we can properly add them to the keybinding
                // text
                self.modifiers = *modifiers;
            }
            _ => {}
        }
    }

    fn format_key(&self, key_event: &KeyEvent) -> Option<String> {
        if let Some(text) = get_special_key(&key_event.logical_key) {
            Some(self.format_key_text(text, true))
        } else {
            self.format_normal_key(key_event)
        }
    }

    fn format_normal_key(&self, key_event: &KeyEvent) -> Option<String> {
        // On macOs, when alt is held and alt_is_meta is set to true, then send the base key plus
        // the whole modifier state. Otherwise send the resulting character with "S-" and "M-"
        // removed.
        #[cfg(target_os = "macos")]
        if self.modifiers.state().alt_key() && use_alt() {
            return key_event
                .key_without_modifiers()
                .to_text()
                .map(|text| self.format_key_text(text, true));
        }

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
        let modifiers = self.format_modifier_string(is_special);
        // < needs to be formatted as a special character, but note that it's not threated as a
        // special key for the modifier formatting, so S- and -M are still potentially stripped
        let (text, is_special) = if text == "<" {
            ("lt", true)
        } else {
            (text, is_special)
        };
        if modifiers.is_empty() {
            if is_special {
                format!("<{text}>")
            } else {
                text.to_string()
            }
        } else {
            format!("<{modifiers}{text}>")
        }
    }

    pub fn format_modifier_string(&self, is_special: bool) -> String {
        // Is special is used for special keys so that all modifiers are always included
        // It's also true with alt_is_meta is set to true.
        // When the key is not special, shift is removed, since the base character is already
        // shifted. Furthermore on macOS, meta is additionally removed when alt_is_meta is set to false
        let shift = or_empty(self.modifiers.state().shift_key() && is_special, "S-");
        let ctrl = or_empty(self.modifiers.state().control_key(), "C-");
        let alt = or_empty(
            self.modifiers.state().alt_key() && (use_alt() || is_special),
            "M-",
        );
        let logo = or_empty(self.modifiers.state().super_key(), "D-");

        shift.to_owned() + ctrl + alt + logo
    }
}

fn or_empty(condition: bool, text: &str) -> &str {
    if condition {
        text
    } else {
        ""
    }
}

#[cfg(not(target_os = "macos"))]
fn use_alt() -> bool {
    true
}

// The option or alt key is used on Macos for character set changes
// and does not operate the same as other systems.
#[cfg(target_os = "macos")]
fn use_alt() -> bool {
    let settings = SETTINGS.get::<KeyboardSettings>();
    settings.macos_alt_is_meta
}

fn get_special_key(key: &Key) -> Option<&str> {
    match key {
        Key::Backspace => Some("BS"),
        Key::Space => Some("Space"),
        Key::Escape => Some("Esc"),
        Key::Delete => Some("Del"),
        Key::ArrowUp => Some("Up"),
        Key::ArrowDown => Some("Down"),
        Key::ArrowLeft => Some("Left"),
        Key::ArrowRight => Some("Right"),
        Key::F1 => Some("F1"),
        Key::F2 => Some("F2"),
        Key::F3 => Some("F3"),
        Key::F4 => Some("F4"),
        Key::F5 => Some("F5"),
        Key::F6 => Some("F6"),
        Key::F7 => Some("F7"),
        Key::F8 => Some("F8"),
        Key::F9 => Some("F9"),
        Key::F10 => Some("F10"),
        Key::F11 => Some("F11"),
        Key::F12 => Some("F12"),
        Key::F13 => Some("F13"),
        Key::F14 => Some("F14"),
        Key::F15 => Some("F15"),
        Key::F16 => Some("F16"),
        Key::F17 => Some("F17"),
        Key::F18 => Some("F18"),
        Key::F19 => Some("F19"),
        Key::F20 => Some("F20"),
        Key::F21 => Some("F21"),
        Key::F22 => Some("F22"),
        Key::F23 => Some("F23"),
        Key::F24 => Some("F24"),
        Key::F25 => Some("F25"),
        Key::F26 => Some("F26"),
        Key::F27 => Some("F27"),
        Key::F28 => Some("F28"),
        Key::F29 => Some("F29"),
        Key::F30 => Some("F30"),
        Key::F31 => Some("F31"),
        Key::F32 => Some("F32"),
        Key::F33 => Some("F33"),
        Key::F34 => Some("F34"),
        Key::F35 => Some("F35"),
        Key::Insert => Some("Insert"),
        Key::Home => Some("Home"),
        Key::End => Some("End"),
        Key::PageUp => Some("PageUp"),
        Key::PageDown => Some("PageDown"),
        Key::Tab => Some("Tab"),
        _ => None,
    }
}
