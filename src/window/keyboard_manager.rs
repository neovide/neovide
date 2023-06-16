use crate::{
    bridge::{SerialCommand, UiCommand},
    event_aggregator::EVENT_AGGREGATOR,
};
use winit::{
    event::{ElementState, Event, Ime, Modifiers, WindowEvent},
    keyboard::Key,
    platform::modifier_supplement::KeyEventExtModifierSupplement,
};

pub struct KeyboardManager {
    modifiers: Modifiers,
}

impl KeyboardManager {
    pub fn new() -> KeyboardManager {
        KeyboardManager {
            modifiers: Modifiers::default(),
        }
    }

    pub fn handle_event(&mut self, event: &Event<()>) {
        match event {
            Event::WindowEvent {
                event:
                    WindowEvent::KeyboardInput {
                        event: key_event, ..
                    },
                ..
            } => {
                if key_event.state == ElementState::Pressed {
                    if let Some(text) = get_control_key(&key_event.logical_key).or(key_event
                        .text_with_all_modifiers()
                        .map(|text| text.to_string()))
                    {
                        log::trace!("Key pressed {} {:?}", text, self.modifiers.state());

                        EVENT_AGGREGATOR.send(UiCommand::Serial(SerialCommand::Keyboard(
                            self.format_special_key(&text),
                        )));
                    }
                }
            }
            Event::WindowEvent {
                event: WindowEvent::Ime(Ime::Commit(_string)),
                ..
            } => {}
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

    fn format_special_key(&self, text: &str) -> String {
        let text = if text == "<" { "<lt>" } else { text };
        let modifiers = self.format_modifier_string(false);
        if modifiers.is_empty() {
            text.to_string()
        } else {
            format!("<{modifiers}{text}>")
        }
    }

    pub fn format_modifier_string(&self, use_shift: bool) -> String {
        let shift = or_empty(self.modifiers.state().shift_key() && use_shift, "S-");
        let ctrl = or_empty(self.modifiers.state().control_key(), "C-");
        let alt = or_empty(self.modifiers.state().alt_key(), "M-");
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

fn get_control_key(key: &Key) -> Option<String> {
    match key {
        Key::Backspace => Some("BS"),
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
    .map(|text| format!("<{}>", text))
}
