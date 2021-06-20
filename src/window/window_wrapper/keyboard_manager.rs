use glutin::event::{ElementState, Event, WindowEvent};
use glutin::keyboard::Key;

use crate::bridge::UiCommand;
use crate::channel_utils::LoggingTx;

pub struct KeyboardManager {
    command_sender: LoggingTx<UiCommand>,
    shift: bool,
    ctrl: bool,
    alt: bool,
    logo: bool,
}

#[cfg(not(target_os = "windows"))]
fn use_logo(logo: bool) -> bool {
    logo
}

// The Windows key is used for OS-level shortcuts,
// so we want to ignore the logo key on this platform.
#[cfg(target_os = "windows")]
fn use_logo(_: bool) -> bool {
    false
}

fn or_empty(condition: bool, text: &str) -> &str {
    if condition {
        text
    } else {
        ""
    }
}

fn get_key_text(key: Key<'static>) -> Option<(&str, bool)> {
    match key {
        Key::Character(character_text) => match character_text {
            " " => Some(("Space", true)),
            "<" => Some(("lt", true)),
            "\\" => Some(("Bslash", true)),
            "|" => Some(("Bar", true)),
            "   " => Some(("Tab", true)),
            "\n" => Some(("CR", true)),
            _ => Some((character_text, false)),
        },
        Key::Backspace => Some(("BS", true)),
        Key::Tab => Some(("Tab", true)),
        Key::Enter => Some(("CR", true)),
        Key::Escape => Some(("Esc", true)),
        Key::Space => Some(("Space", true)),
        Key::Delete => Some(("Del", true)),
        Key::ArrowUp => Some(("Up", true)),
        Key::ArrowDown => Some(("Down", true)),
        Key::ArrowLeft => Some(("Left", true)),
        Key::ArrowRight => Some(("Right", true)),
        Key::F1 => Some(("F1", true)),
        Key::F2 => Some(("F2", true)),
        Key::F3 => Some(("F3", true)),
        Key::F4 => Some(("F4", true)),
        Key::F5 => Some(("F5", true)),
        Key::F6 => Some(("F6", true)),
        Key::F7 => Some(("F7", true)),
        Key::F8 => Some(("F8", true)),
        Key::F9 => Some(("F9", true)),
        Key::F10 => Some(("F10", true)),
        Key::F11 => Some(("F11", true)),
        Key::F12 => Some(("F12", true)),
        Key::Insert => Some(("Insert", true)),
        Key::Home => Some(("Home", true)),
        Key::End => Some(("End", true)),
        Key::PageUp => Some(("PageUp", true)),
        Key::PageDown => Some(("PageDown", true)),
        _ => None,
    }
}

impl KeyboardManager {
    pub fn new(command_sender: LoggingTx<UiCommand>) -> KeyboardManager {
        KeyboardManager {
            command_sender,
            shift: false,
            ctrl: false,
            alt: false,
            logo: false,
        }
    }

    fn format_keybinding_string(&self, special: bool, text: &str) -> String {
        let special = special || self.shift || self.ctrl || self.alt || self.logo;

        let open = or_empty(special, "<");
        let shift = or_empty(self.shift, "S-");
        let ctrl = or_empty(self.ctrl, "C-");
        let alt = or_empty(self.alt, "M-");
        let logo = or_empty(use_logo(self.logo), "D-");
        let close = or_empty(special, ">");

        format!("{}{}{}{}{}{}{}", open, shift, ctrl, alt, logo, text, close)
    }

    pub fn handle_event(&mut self, event: &Event<()>) {
        if let Event::WindowEvent {
            event: WindowEvent::KeyboardInput {
                event: key_event, ..
            },
            ..
        } = event
        {
            let key_pressed = match key_event.state {
                ElementState::Pressed => true,
                ElementState::Released => false,
            };

            match key_event.logical_key {
                Key::Shift => self.shift = key_pressed,
                Key::Control => self.ctrl = key_pressed,
                Key::Alt => self.alt = key_pressed,
                Key::Super => self.logo = key_pressed,
                _ => {}
            };

            if key_event.state == ElementState::Pressed {
                if let Some((key_text, special)) = get_key_text(key_event.logical_key) {
                    let keybinding_string = self.format_keybinding_string(special, key_text);

                    self.command_sender
                        .send(UiCommand::Keyboard(keybinding_string))
                        .expect("Could not send keyboard ui command");
                }
            }
        }
    }
}
