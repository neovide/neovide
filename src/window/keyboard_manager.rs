use crate::{
    bridge::{SerialCommand, UiCommand},
    event_aggregator::EVENT_AGGREGATOR,
    settings::SETTINGS,
    window::KeyboardSettings,
};
use winit::{
    event::{ElementState, Event, KeyEvent, WindowEvent},
    keyboard::{Key, Key::Dead},
    platform::modifier_supplement::KeyEventExtModifierSupplement,
};

enum InputEvent {
    KeyEvent(KeyEvent),
    ImeInput(String),
}
pub struct KeyboardManager {
    shift: bool,
    ctrl: bool,
    alt: bool,
    prev_dead_key: Option<char>,
    logo: bool,
    ignore_input_this_frame: bool,
    queued_input_events: Vec<InputEvent>,
}

impl KeyboardManager {
    pub fn new() -> KeyboardManager {
        KeyboardManager {
            shift: false,
            ctrl: false,
            alt: false,
            prev_dead_key: None,
            logo: false,
            ignore_input_this_frame: false,
            queued_input_events: Vec::new(),
        }
    }

    pub fn handle_event(&mut self, event: &Event<()>) {
        match event {
            Event::WindowEvent {
                event: WindowEvent::Focused(_focused),
                ..
            } => {
                // When window is just focused or lost it's focus, ignore keyboard events
                // that were submitted this frame
                self.ignore_input_this_frame = true;
            }
            Event::WindowEvent {
                event:
                    WindowEvent::KeyboardInput {
                        event: key_event, ..
                    },
                ..
            } => {
                // Store the event so that we can ignore it properly if the window was just
                // focused.
                self.queued_input_events
                    .push(InputEvent::KeyEvent(key_event.clone()));
            }
            Event::WindowEvent {
                event: WindowEvent::ReceivedImeText(string),
                ..
            } => {
                self.queued_input_events
                    .push(InputEvent::ImeInput(string.to_string()));
            }
            Event::WindowEvent {
                event: WindowEvent::ModifiersChanged(modifiers),
                ..
            } => {
                // Record the modifier states so that we can properly add them to the keybinding
                // text
                self.shift = modifiers.shift_key();
                self.ctrl = modifiers.control_key();
                self.alt = modifiers.alt_key();
                self.logo = modifiers.super_key();
            }
            Event::MainEventsCleared => {
                // If the window wasn't just focused.
                if !self.should_ignore_input() {
                    // And we have a keyboard event this frame
                    for input_event in self.queued_input_events.iter() {
                        let mut next_dead_key = self.prev_dead_key;
                        match input_event {
                            InputEvent::KeyEvent(key_event) => {
                                // And a key was pressed
                                if key_event.state == ElementState::Pressed {
                                    if let Some(keybinding) = self.maybe_get_keybinding(key_event) {
                                        EVENT_AGGREGATOR.send(UiCommand::Serial(
                                            SerialCommand::Keyboard(keybinding),
                                        ));
                                    }
                                    next_dead_key = None;
                                } else if key_event.state == ElementState::Released {
                                    // dead key detect here
                                    if let Dead(dead_key) = key_event.logical_key {
                                        // should wait for the next input text_with_all_modifiers, and ignore the next ime.
                                        next_dead_key = dead_key;
                                    }
                                }
                            }
                            InputEvent::ImeInput(raw_input) => {
                                if self.prev_dead_key.is_none() {
                                    EVENT_AGGREGATOR.send(UiCommand::Serial(
                                        SerialCommand::Keyboard(raw_input.to_string()),
                                    ));
                                }
                            }
                        }
                        self.prev_dead_key = next_dead_key;
                    }
                }

                // Regardless of whether this was a valid keyboard input or not, rest ignoring and
                // whatever event was queued.
                self.ignore_input_this_frame = false;
                self.queued_input_events.clear();
            }
            _ => {}
        }
    }

    fn should_ignore_input(&self) -> bool {
        let settings = SETTINGS.get::<KeyboardSettings>();
        self.ignore_input_this_frame || (self.logo && !settings.use_logo)
    }

    fn maybe_get_keybinding(&self, key_event: &KeyEvent) -> Option<String> {
        // Determine if this key event represents a key which won't ever
        // present text.
        if let Some(key_text) = is_control_key(key_event.logical_key) {
            if self.prev_dead_key.is_some() {
                //recover dead key to normal character
                let real_char = String::from(self.prev_dead_key.unwrap());
                Some(real_char + &self.format_special_key(true, key_text))
            } else {
                Some(self.format_special_key(true, key_text))
            }
        } else {
            let key_text = if self.prev_dead_key.is_none() {
                key_event.text
            } else {
                key_event.text_with_all_modifiers()
            };
            if let Some(original_key_text) = key_text {
                let mut key_text = original_key_text;
                if self.alt {
                    if let Some(modify) = key_event_text(key_event) {
                        key_text = modify;
                    }
                }
                // This is not a control key, so we rely upon winit to determine if
                // this is a deadkey or not.
                let keybinding_string =
                    if let Some((escaped_text, use_shift)) = is_special(key_text) {
                        self.format_special_key(use_shift, escaped_text)
                    } else {
                        self.format_normal_key(key_text)
                    };

                Some(keybinding_string)
            } else {
                None
            }
        }
    }

    fn format_special_key(&self, use_shift: bool, text: &str) -> String {
        let modifiers = self.format_modifier_string(use_shift);

        format!("<{modifiers}{text}>")
    }

    fn format_normal_key(&self, text: &str) -> String {
        let has_modifier = self.ctrl || use_alt(self.alt) || self.logo;
        // use shift only if `text` is alphabetic
        let is_alphabetic = text.chars().all(char::is_alphabetic);

        if has_modifier {
            self.format_special_key(is_alphabetic, text)
        } else {
            text.to_string()
        }
    }

    pub fn format_modifier_string(&self, use_shift: bool) -> String {
        let shift = or_empty(self.shift && use_shift, "S-");
        let ctrl = or_empty(self.ctrl, "C-");
        let alt = or_empty(use_alt(self.alt), "M-");
        let logo = or_empty(self.logo, "D-");

        shift.to_owned() + ctrl + alt + logo
    }
}

#[cfg(not(target_os = "macos"))]
fn use_alt(alt: bool) -> bool {
    alt
}

// The option or alt key is used on Macos for character set changes
// and does not operate the same as other systems.
#[cfg(target_os = "macos")]
fn use_alt(alt: bool) -> bool {
    let settings = SETTINGS.get::<KeyboardSettings>();
    settings.macos_alt_is_meta && alt
}

#[cfg(not(target_os = "macos"))]
fn key_event_text(key_event: &KeyEvent) -> Option<&str> {
    key_event.key_without_modifiers().to_text()
}

#[cfg(target_os = "macos")]
fn key_event_text(key_event: &KeyEvent) -> Option<&str> {
    let settings = SETTINGS.get::<KeyboardSettings>();
    if settings.macos_alt_is_meta {
        key_event.text
    } else {
        key_event.text_with_all_modifiers()
    }
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
        Key::Tab => Some("Tab"),
        _ => None,
    }
}

// returns (`escaped_text`, `use_shift`)
fn is_special(text: &str) -> Option<(&str, bool)> {
    match text {
        " " => Some(("Space", true)),
        "<" => Some(("lt", false)),
        "\\" => Some(("Bslash", false)),
        "|" => Some(("Bar", false)),
        "\t" => Some(("Tab", true)),
        "\n" | "\r" => Some(("CR", true)),
        _ => None,
    }
}
