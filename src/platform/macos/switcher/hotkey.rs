use winit::event::{ElementState, KeyEvent, Modifiers};

use crate::window::macos::tab_navigation::KeyCombo;

const SWITCHER_ENV_VAR: &str = "NEOVIDE_SYSTEM_SWITCHER_HOTKEY";
const DEFAULT_SWITCHER_HOTKEY: &str = "cmd+ctrl+n";

pub fn editor_switcher_key_event_matches(event: &KeyEvent, modifiers: &Modifiers) -> bool {
    event.state == ElementState::Pressed
        && editor_switcher_toggle_shortcut()
            .is_some_and(|shortcut| shortcut.matches_key_event(event, modifiers))
}

pub fn editor_switcher_toggle_shortcut() -> Option<KeyCombo> {
    let shortcut =
        std::env::var(SWITCHER_ENV_VAR).ok().unwrap_or_else(|| DEFAULT_SWITCHER_HOTKEY.to_string());

    KeyCombo::parse(&shortcut)
}
