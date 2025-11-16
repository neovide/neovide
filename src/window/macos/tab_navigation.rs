use log::warn;
use winit::{
    event::{ElementState, KeyEvent, Modifiers},
    keyboard::{Key, NamedKey},
};

use crate::{settings::Settings, CmdLineSettings};

#[derive(Clone, Copy)]
pub(crate) enum TabNavigationAction {
    Next,
    Previous,
}

#[derive(Clone)]
pub(crate) struct TabNavigationHotkeys {
    next: Option<KeyCombo>,
    prev: Option<KeyCombo>,
}

impl TabNavigationHotkeys {
    pub(crate) fn new(settings: &Settings) -> Self {
        let cmdline = settings.get::<CmdLineSettings>();
        Self {
            next: KeyCombo::parse(&cmdline.macos_tab_next_hotkey),
            prev: KeyCombo::parse(&cmdline.macos_tab_prev_hotkey),
        }
    }

    pub(crate) fn action_for(
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
            .find(|(_, combo)| combo.is_some_and(|combo| combo.matches(event, modifiers)))
            .map(|(action, _)| action),
            ElementState::Released => None,
        }
    }
}

#[derive(Clone, Copy)]
struct KeyCombo {
    command: bool,
    control: bool,
    option: bool,
    shift: bool,
    key: KeyMatch,
}

impl KeyCombo {
    fn parse(raw: &str) -> Option<Self> {
        let trimmed = raw.trim();
        if trimmed.is_empty() || is_disabled_keyword(trimmed) {
            return None;
        }

        trimmed
            .split('+')
            .map(|part| part.trim())
            .filter(|t| !t.is_empty())
            .try_fold(ParseState::default(), |mut state, token| {
                state.apply_token(parse_token(token, raw)?, raw)?;
                Some(state)
            })?
            .build(raw)
    }

    fn matches(&self, event: &KeyEvent, modifiers: &Modifiers) -> bool {
        if !self.modifiers_match(modifiers) {
            return false;
        }

        let pressed_key = pressed_character(event);
        let logical_key = event.logical_key.as_ref();
        match self.key {
            KeyMatch::Char(expected) => pressed_key.is_some_and(|c| c == expected),
            KeyMatch::Named(expected) => {
                matches!(logical_key, Key::Named(named) if named == expected)
            }
        }
    }

    fn modifiers_match(&self, modifiers: &Modifiers) -> bool {
        let state = modifiers.state();
        (self.command, self.control, self.option, self.shift)
            == (
                state.super_key(),
                state.control_key(),
                state.alt_key(),
                state.shift_key(),
            )
    }
}

fn pressed_character(event: &KeyEvent) -> Option<char> {
    event
        .text
        .as_ref()
        .and_then(|text| text.chars().next())
        .or_else(|| match event.logical_key.as_ref() {
            Key::Character(text) if !text.is_empty() => text.chars().next(),
            _ => None,
        })
}

fn parse_token(value: &str, raw: &str) -> Option<ParsedToken> {
    let keyword = value.to_ascii_lowercase();
    Some(match keyword.as_str() {
        "cmd" => ParsedToken::Modifier(ModifierToken::Command),
        "ctrl" => ParsedToken::Modifier(ModifierToken::Control),
        "alt" => ParsedToken::Modifier(ModifierToken::Option),
        "shift" => ParsedToken::Modifier(ModifierToken::Shift),
        "left" => ParsedToken::Key(KeyMatch::Named(NamedKey::ArrowLeft)),
        "right" => ParsedToken::Key(KeyMatch::Named(NamedKey::ArrowRight)),
        "up" => ParsedToken::Key(KeyMatch::Named(NamedKey::ArrowUp)),
        "down" => ParsedToken::Key(KeyMatch::Named(NamedKey::ArrowDown)),
        _ => ParsedToken::Key(KeyMatch::Char(parse_character_key(value, raw)?)),
    })
}

fn parse_character_key(value: &str, raw: &str) -> Option<char> {
    let mut chars = value.chars();
    let Some(ch) = chars.next() else {
        warn!(
            "macOS tab navigation shortcut '{}' has no key; ignoring",
            raw
        );
        return None;
    };

    if chars.next().is_some() {
        warn!(
            "macOS tab navigation shortcut '{}' must end with a single character key; ignoring",
            raw
        );
        return None;
    }

    Some(ch)
}

#[derive(Default)]
struct ParseState {
    command: bool,
    control: bool,
    option: bool,
    shift: bool,
    key: Option<KeyMatch>,
}

impl ParseState {
    fn apply_token(&mut self, token: ParsedToken, raw: &str) -> Option<()> {
        match token {
            ParsedToken::Modifier(modifier) => self.apply_modifier(modifier),
            ParsedToken::Key(value) => self.set_key(value, raw)?,
        }
        Some(())
    }

    fn apply_modifier(&mut self, modifier: ModifierToken) {
        match modifier {
            ModifierToken::Command => self.command = true,
            ModifierToken::Control => self.control = true,
            ModifierToken::Option => self.option = true,
            ModifierToken::Shift => self.shift = true,
        }
    }

    fn set_key(&mut self, value: KeyMatch, raw: &str) -> Option<()> {
        match self.key {
            Some(_) => {
                warn!(
                    "macOS tab navigation shortcut '{}' has multiple keys; ignoring",
                    raw
                );
                None
            }
            None => {
                self.key = Some(value);
                Some(())
            }
        }
    }

    fn build(self, raw: &str) -> Option<KeyCombo> {
        Some(KeyCombo {
            command: self.command,
            control: self.control,
            option: self.option,
            shift: self.shift,
            key: self.key.or_else(|| {
                warn!(
                    "macOS tab navigation shortcut '{}' is missing a key component; ignoring",
                    raw
                );
                None
            })?,
        })
    }
}

fn is_disabled_keyword(value: &str) -> bool {
    value.trim().eq_ignore_ascii_case("false")
}

#[derive(Clone, Copy)]
enum KeyMatch {
    Char(char),
    Named(NamedKey),
}

enum ParsedToken {
    Modifier(ModifierToken),
    Key(KeyMatch),
}

#[derive(Clone, Copy)]
enum ModifierToken {
    Command,
    Control,
    Option,
    Shift,
}
