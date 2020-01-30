use std::sync::atomic::{AtomicBool, Ordering};

use skulpin::winit::event::{KeyboardInput, ElementState, ModifiersState, VirtualKeyCode};

use super::{BRIDGE, UiCommand};

lazy_static! {
    pub static ref KEYBOARD_MANAGER: KeyboardManager = KeyboardManager::new();
}

pub struct KeyboardManager {
    shift: AtomicBool,
    ctrl: AtomicBool,
    alt: AtomicBool,
    logo: AtomicBool
}

impl KeyboardManager {
    pub fn new() -> KeyboardManager {
        KeyboardManager {
            shift: AtomicBool::new(false),
            ctrl: AtomicBool::new(false),
            alt: AtomicBool::new(false),
            logo: AtomicBool::new(false)
        }
    }

    fn apply_modifiers(&self, text: String, escaped: bool) -> String {
        let mut escaped = escaped;
        let mut result = text;

        if self.shift.load(Ordering::Relaxed) {
            result = format!("S-{}", result);
            escaped = true;
        }

        if self.ctrl.load(Ordering::Relaxed) {
            result = format!("C-{}", result);
            escaped = true;
        }

        if self.alt.load(Ordering::Relaxed) {
            result = format!("M-{}", result);
            escaped = true;
        }

        if self.logo.load(Ordering::Relaxed) {
            result = format!("D-{}", result);
            escaped = true;
        }

        if escaped {
            format!("<{}>", result)
        } else {
            result
        }
    }

    pub fn handle_keyboard_input(&self, input: KeyboardInput) {
        let keycode = match input {
            KeyboardInput {
                state: ElementState::Pressed,
                virtual_keycode: keycode,
                ..
            } => keycode,
            _ => None
        };

        keycode.and_then(|keycode| {
            match keycode {
                VirtualKeyCode::Escape => Some("ESC"),
                VirtualKeyCode::F1 => Some("F1"),
                VirtualKeyCode::F2 => Some("F2"),
                VirtualKeyCode::F3 => Some("F3"),
                VirtualKeyCode::F4 => Some("F4"),
                VirtualKeyCode::F5 => Some("F5"),
                VirtualKeyCode::F6 => Some("F6"),
                VirtualKeyCode::F7 => Some("F7"),
                VirtualKeyCode::F8 => Some("F8"),
                VirtualKeyCode::F9 => Some("F9"),
                VirtualKeyCode::F10 => Some("F10"),
                VirtualKeyCode::F11 => Some("F11"),
                VirtualKeyCode::F12 => Some("F12"),
                VirtualKeyCode::F13 => Some("F13"),
                VirtualKeyCode::F14 => Some("F14"),
                VirtualKeyCode::F15 => Some("F15"),
                VirtualKeyCode::F16 => Some("F16"),
                VirtualKeyCode::F17 => Some("F17"),
                VirtualKeyCode::F18 => Some("F18"),
                VirtualKeyCode::F19 => Some("F19"),
                VirtualKeyCode::F20 => Some("F20"),
                VirtualKeyCode::F21 => Some("F21"),
                VirtualKeyCode::F22 => Some("F22"),
                VirtualKeyCode::F23 => Some("F23"),
                VirtualKeyCode::F24 => Some("F24"),
                VirtualKeyCode::Insert => Some("Insert"),
                VirtualKeyCode::Home => Some("Home"),
                VirtualKeyCode::Delete => Some("Delete"),
                VirtualKeyCode::End => Some("End"),
                VirtualKeyCode::PageDown => Some("PageDown"),
                VirtualKeyCode::PageUp => Some("PageUp"),
                VirtualKeyCode::Left => Some("Left"),
                VirtualKeyCode::Up => Some("Up"),
                VirtualKeyCode::Right => Some("Right"),
                VirtualKeyCode::Down => Some("Down"),
                VirtualKeyCode::Back => Some("BS"),
                VirtualKeyCode::Return => Some("Enter"),
                VirtualKeyCode::Backslash => Some("Bslash"),
                VirtualKeyCode::Tab => Some("Tab"),
                _ => None
            }
        }).map(|keyboard_input| {
            // let keyboard_input = self.apply_modifiers(keyboard_input.to_string(), true);
            // BRIDGE.queue_command(UiCommand::Keyboard(keyboard_input));
        });
    }

    pub fn handle_received_character(&self, character: char) {
        let keyboard_input = character.escape_unicode().to_string();
        BRIDGE.queue_command(UiCommand::Keyboard(keyboard_input));
    }

    pub fn handle_modifiers(&self, modifiers: ModifiersState) {
        self.shift.store(modifiers.shift(), Ordering::Relaxed);
        self.ctrl.store(modifiers.ctrl(), Ordering::Relaxed);
        self.alt.store(modifiers.alt(), Ordering::Relaxed);
        self.logo.store(modifiers.logo(), Ordering::Relaxed);
    }
}
