use winit::event::ElementState;
use winit::event::KeyEvent;
use winit::event::Modifiers;

use crate::CmdLineSettings;
use crate::settings::Settings;
use crate::window::macos::shortcut::KeyCombo;

#[derive(Clone, Copy)]
pub enum TabNavigationAction {
    Next,
    Previous,
}

#[derive(Clone)]
pub struct TabNavigationHotkeys {
    next: Option<KeyCombo>,
    prev: Option<KeyCombo>,
}

impl TabNavigationHotkeys {
    pub fn new(settings: &Settings) -> Self {
        let cmdline = settings.get::<CmdLineSettings>();
        Self {
            next: KeyCombo::parse(&cmdline.system_tab_next_hotkey),
            prev: KeyCombo::parse(&cmdline.system_tab_prev_hotkey),
        }
    }

    pub fn action_for(
        &self,
        event: &KeyEvent,
        modifiers: &Modifiers,
    ) -> Option<TabNavigationAction> {
        match event.state {
            ElementState::Pressed => [
                (TabNavigationAction::Next, self.next.as_ref()),
                (TabNavigationAction::Previous, self.prev.as_ref()),
            ]
            .into_iter()
            .find(|(_, combo)| combo.is_some_and(|combo| combo.matches_key_event(event, modifiers)))
            .map(|(action, _)| action),
            ElementState::Released => None,
        }
    }
}
