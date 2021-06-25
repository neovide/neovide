use glutin::event::{ElementState, Event, WindowEvent};
use glutin::keyboard::Key;

use crate::bridge::UiCommand;
use crate::channel_utils::LoggingTx;

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

fn is_control_key(key: Key<'static>) -> Option<&str> {
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
        Key::Insert => Some("Insert"),
        Key::Home => Some("Home"),
        Key::End => Some("End"),
        Key::PageUp => Some("PageUp"),
        Key::PageDown => Some("PageDown"),
        _ => None,
    }
}

fn is_special(text: &str) -> Option<&str> {
    match text {
        " " => Some("Space"),
        "<" => Some("lt"),
        "\\" => Some("Bslash"),
        "|" => Some("Bar"),
        "\t" => Some("Tab"),
        "\n" => Some("CR"),
        _ => None,
    }
}

pub struct KeyboardManager {
    command_sender: LoggingTx<UiCommand>,
    ctrl: bool,
    alt: bool,
    logo: bool,
}

impl KeyboardManager {
    pub fn new(command_sender: LoggingTx<UiCommand>) -> KeyboardManager {
        KeyboardManager {
            command_sender,
            ctrl: false,
            alt: false,
            logo: false,
        }
    }

    fn format_keybinding_string(&self, special: bool, text: &str) -> String {
        let special = special || self.ctrl || self.alt || self.logo;

        let open = or_empty(special, "<");
        let ctrl = or_empty(self.ctrl, "C-");
        let alt = or_empty(self.alt, "M-");
        let logo = or_empty(use_logo(self.logo), "D-");
        let close = or_empty(special, ">");

        format!("{}{}{}{}{}{}", open, ctrl, alt, logo, text, close)
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
                    if let Some(key_text) = is_control_key(key_event.logical_key) {
                        let keybinding_string = self.format_keybinding_string(true, key_text);

                        self.command_sender
                            .send(UiCommand::Keyboard(keybinding_string))
                            .expect("Could not send keyboard ui command");
                    } else if let Some(key_text) = key_event.text {
                        let keybinding_string = if let Some(escaped_text) = is_special(key_text) {
                            self.format_keybinding_string(true, escaped_text)
                        } else {
                            self.format_keybinding_string(false, key_text)
                        };

                        self.command_sender
                            .send(UiCommand::Keyboard(keybinding_string))
                            .expect("Could not send keyboard ui command");
                    }
                }
            }
            Event::WindowEvent {
                event: WindowEvent::ModifiersChanged(modifiers),
                ..
            } => {
                self.ctrl = modifiers.control_key();
                self.alt = modifiers.alt_key();
                self.logo = modifiers.super_key();
            }
            _ => {}
        }
    }
}
