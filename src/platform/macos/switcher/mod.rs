use std::cell::RefCell;

use objc2::{rc::Retained, runtime::Sel, sel};
use objc2_app_kit::{NSApplication, NSEvent, NSEventModifierFlags, NSImage, NSTextField, NSWindow};
use objc2_foundation::{MainThreadMarker, NSString};
use winit::window::WindowId;

use crate::window::macos::tab_navigation::KeyCombo;

use self::appkit::{
    EditorSwitcherActionHandler, EditorSwitcherDocumentView, EditorSwitcherSearchDelegate,
};
pub use self::hotkey::editor_switcher_key_event_matches;
pub use self::row::EditorSwitcherRow;
use self::row::editor_switcher_row_matches;
use self::ui::rebuild_editor_switcher_rows;
pub use self::ui::show_editor_switcher_panel;
use super::request_activate_window;

mod appkit;
mod hotkey;
mod layout;
mod row;
mod ui;

const KEY_ESCAPE: u16 = 53;
const KEY_RETURN: u16 = 36;
const KEY_KEYPAD_ENTER: u16 = 76;
const KEY_DOWN: u16 = 125;
const KEY_UP: u16 = 126;
const KEY_P: u16 = 35;
const KEY_N: u16 = 45;
const KEY_TAB: u16 = 48;
const KEY_DELETE: u16 = 51;
const KEY_FORWARD_DELETE: u16 = 117;

thread_local! {
    static EDITOR_SWITCHER_STATE: RefCell<Option<EditorSwitcherState>> = const { RefCell::new(None) };
}

struct EditorSwitcherState {
    window: Retained<NSWindow>,
    search_field: Retained<NSTextField>,
    results_container: Retained<EditorSwitcherDocumentView>,
    row_action_handler: Retained<EditorSwitcherActionHandler>,
    _search_delegate: Retained<EditorSwitcherSearchDelegate>,
    rows: Vec<EditorSwitcherRow>,
    icon: Option<Retained<NSImage>>,
    filtered_indices: Vec<usize>,
    selected_index: usize,
    query: String,
    toggle_shortcut: Option<KeyCombo>,
}

struct EditorSwitcherStateParts {
    window: Retained<NSWindow>,
    search_field: Retained<NSTextField>,
    results_container: Retained<EditorSwitcherDocumentView>,
    row_action_handler: Retained<EditorSwitcherActionHandler>,
    search_delegate: Retained<EditorSwitcherSearchDelegate>,
    rows: Vec<EditorSwitcherRow>,
    icon: Option<Retained<NSImage>>,
    toggle_shortcut: Option<KeyCombo>,
}

impl EditorSwitcherState {
    fn new(parts: EditorSwitcherStateParts) -> Self {
        let EditorSwitcherStateParts {
            window,
            search_field,
            results_container,
            row_action_handler,
            search_delegate,
            rows,
            icon,
            toggle_shortcut,
        } = parts;

        let mut state = Self {
            window,
            search_field,
            results_container,
            row_action_handler,
            _search_delegate: search_delegate,
            rows,
            icon,
            filtered_indices: Vec::new(),
            selected_index: 0,
            query: String::new(),
            toggle_shortcut,
        };
        state.refresh_filter();
        state.selected_index = state
            .filtered_indices
            .iter()
            .position(|index| !state.rows[*index].is_current)
            .unwrap_or(0);
        state
    }

    fn refresh_filter(&mut self) {
        let query = self.query.trim().to_lowercase();
        self.filtered_indices = self
            .rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| editor_switcher_row_matches(row, &query).then_some(index))
            .collect();

        if self.filtered_indices.is_empty() {
            self.selected_index = 0;
        } else if self.selected_index >= self.filtered_indices.len() {
            self.selected_index = self.filtered_indices.len() - 1;
        }
    }

    fn push_query(&mut self, text: &str) {
        self.query.push_str(text);
        self.selected_index = 0;
        self.refresh_filter();
    }

    fn pop_query(&mut self) {
        self.query.pop();
        self.selected_index = 0;
        self.refresh_filter();
    }

    fn move_selection(&mut self, delta: isize) {
        let len = self.filtered_indices.len();
        if len == 0 {
            self.selected_index = 0;
            return;
        }

        self.selected_index =
            (self.selected_index as isize + delta).rem_euclid(len as isize) as usize;
    }

    fn selected_window(&self) -> Option<WindowId> {
        self.filtered_indices.get(self.selected_index).map(|index| self.rows[*index].window_id)
    }

    fn update_view(&self) {
        let query = NSString::from_str(&self.query);
        self.search_field.setStringValue(&query);

        self.update_results_view();
    }

    fn update_results_view(&self) {
        rebuild_editor_switcher_rows(self);
    }
}

enum EditorSwitcherKeyAction {
    None,
    Close,
    Activate(WindowId),
    Pass,
}

fn handle_editor_switcher_key(event: &NSEvent) {
    let key_code = event.keyCode();
    let flags = event.modifierFlags();
    let action = EDITOR_SWITCHER_STATE.with(|cell| {
        let mut state_ref = cell.borrow_mut();
        let Some(state) = state_ref.as_mut() else {
            return EditorSwitcherKeyAction::Pass;
        };

        let action = if state
            .toggle_shortcut
            .as_ref()
            .is_some_and(|shortcut| shortcut.matches_nsevent(event))
        {
            EditorSwitcherKeyAction::Close
        } else {
            match key_code {
                KEY_ESCAPE => EditorSwitcherKeyAction::Close,
                KEY_RETURN | KEY_KEYPAD_ENTER => state
                    .selected_window()
                    .map(EditorSwitcherKeyAction::Activate)
                    .unwrap_or(EditorSwitcherKeyAction::Close),
                KEY_DOWN => {
                    state.move_selection(1);
                    EditorSwitcherKeyAction::None
                }
                KEY_UP => {
                    state.move_selection(-1);
                    EditorSwitcherKeyAction::None
                }
                KEY_P if flags.contains(NSEventModifierFlags::Control) => {
                    state.move_selection(-1);
                    EditorSwitcherKeyAction::None
                }
                KEY_N if flags.contains(NSEventModifierFlags::Control) => {
                    state.move_selection(1);
                    EditorSwitcherKeyAction::None
                }
                KEY_TAB => {
                    let delta = if flags.contains(NSEventModifierFlags::Shift) { -1 } else { 1 };
                    state.move_selection(delta);
                    EditorSwitcherKeyAction::None
                }
                KEY_DELETE | KEY_FORWARD_DELETE => {
                    state.pop_query();
                    EditorSwitcherKeyAction::None
                }
                _ => {
                    if flags
                        .intersects(NSEventModifierFlags::Command | NSEventModifierFlags::Control)
                    {
                        EditorSwitcherKeyAction::None
                    } else if let Some(chars) = event.characters() {
                        let text = chars.to_string();
                        if text.chars().all(|character| !character.is_control()) {
                            state.push_query(&text);
                        }
                        EditorSwitcherKeyAction::None
                    } else {
                        EditorSwitcherKeyAction::None
                    }
                }
            }
        };

        if matches!(action, EditorSwitcherKeyAction::None) {
            state.update_view();
        }

        action
    });

    match action {
        EditorSwitcherKeyAction::None | EditorSwitcherKeyAction::Pass => {}
        EditorSwitcherKeyAction::Close => close_editor_switcher(),
        EditorSwitcherKeyAction::Activate(window_id) => {
            close_editor_switcher();
            request_activate_window(window_id);
        }
    }
}

fn handle_editor_switcher_toggle_key(event: &NSEvent) -> bool {
    let should_close = EDITOR_SWITCHER_STATE.with(|cell| {
        cell.borrow().as_ref().is_some_and(|state| {
            state.toggle_shortcut.as_ref().is_some_and(|shortcut| shortcut.matches_nsevent(event))
        })
    });

    if should_close {
        close_editor_switcher();
    }

    should_close
}

fn handle_editor_switcher_current_toggle_key() -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };

    NSApplication::sharedApplication(mtm)
        .currentEvent()
        .as_deref()
        .is_some_and(handle_editor_switcher_toggle_key)
}

fn update_editor_switcher_query_from_search_field() {
    EDITOR_SWITCHER_STATE.with(|cell| {
        let mut state_ref = cell.borrow_mut();
        let Some(state) = state_ref.as_mut() else {
            return;
        };

        state.query = state.search_field.stringValue().to_string();
        state.selected_index = 0;
        state.refresh_filter();
        state.update_results_view();
    });
}

fn editor_switcher_action_for_command_selector(
    command_selector: Sel,
    state: &mut EditorSwitcherState,
) -> EditorSwitcherKeyAction {
    if command_selector == sel!(cancelOperation:) {
        return EditorSwitcherKeyAction::Close;
    }

    if selector_is_newline(command_selector) {
        return state
            .selected_window()
            .map(EditorSwitcherKeyAction::Activate)
            .unwrap_or(EditorSwitcherKeyAction::Close);
    }

    if let Some(delta) = selector_selection_delta(command_selector) {
        state.move_selection(delta);
        state.update_results_view();
        return EditorSwitcherKeyAction::None;
    }

    EditorSwitcherKeyAction::Pass
}

fn selector_is_newline(command_selector: Sel) -> bool {
    command_selector == sel!(insertNewline:)
        || command_selector == sel!(insertNewlineIgnoringFieldEditor:)
}

fn selector_selection_delta(command_selector: Sel) -> Option<isize> {
    if command_selector == sel!(moveUp:) || command_selector == sel!(insertBacktab:) {
        Some(-1)
    } else if command_selector == sel!(moveDown:) || command_selector == sel!(insertTab:) {
        Some(1)
    } else {
        None
    }
}

fn handle_editor_switcher_command_selector(command_selector: Sel) -> bool {
    if handle_editor_switcher_current_toggle_key() {
        return true;
    }

    let action = EDITOR_SWITCHER_STATE.with(|cell| {
        let mut state_ref = cell.borrow_mut();
        let Some(state) = state_ref.as_mut() else {
            return EditorSwitcherKeyAction::Pass;
        };

        editor_switcher_action_for_command_selector(command_selector, state)
    });

    match action {
        EditorSwitcherKeyAction::None => true,
        EditorSwitcherKeyAction::Close => {
            close_editor_switcher();
            true
        }
        EditorSwitcherKeyAction::Activate(window_id) => {
            close_editor_switcher();
            request_activate_window(window_id);
            true
        }
        EditorSwitcherKeyAction::Pass => false,
    }
}

pub fn close_editor_switcher_if_open() -> bool {
    EDITOR_SWITCHER_STATE.with(|cell| {
        if let Some(state) = cell.borrow_mut().take() {
            state.window.orderOut(None);
            true
        } else {
            false
        }
    })
}

fn close_editor_switcher() {
    close_editor_switcher_if_open();
}

fn activate_editor_switcher_row(row: usize) {
    let selected_window = EDITOR_SWITCHER_STATE.with(|cell| {
        let mut state_ref = cell.borrow_mut();
        let state = state_ref.as_mut()?;
        state.selected_index = row.min(state.filtered_indices.len().saturating_sub(1));
        state.selected_window()
    });

    if let Some(window_id) = selected_window {
        close_editor_switcher();
        request_activate_window(window_id);
    }
}

fn activate_selected_editor_switcher_row() {
    let selected_window = EDITOR_SWITCHER_STATE
        .with(|cell| cell.borrow().as_ref().and_then(|state| state.selected_window()));

    if let Some(window_id) = selected_window {
        close_editor_switcher();
        request_activate_window(window_id);
    }
}
