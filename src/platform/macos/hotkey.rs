use std::collections::HashSet;
use std::{
    env::{self, VarError},
    fmt,
    mem::{self, MaybeUninit},
    os::raw::c_void,
    ptr,
};

use log::{info, warn};
use objc2_app_kit::NSEventModifierFlags;
use winit::{event_loop::EventLoopProxy, window::WindowId};

use crate::window::{EventPayload, MacShortcutCommand, UserEvent};

const PINNED_ENV_VAR: &str = "NEOVIDE_MACOS_PINNED_HOTKEY";
const SWITCHER_ENV_VAR: &str = "NEOVIDE_MACOS_SWITCHER_HOTKEY";
const LEGACY_ENV_VAR: &str = "NEOVIDE_MACOS_ACTIVATION_HOTKEY";

const PINNED_DEFAULT: &str = "cmd+ctrl+z";
const SWITCHER_DEFAULT: &str = "cmd+ctrl+n";

const HOTKEY_SIGNATURE: u32 = u32::from_be_bytes(*b"NEOV");
const EVENT_CLASS_KEYBOARD: u32 = u32::from_be_bytes(*b"keyb");
const EVENT_KIND_HOT_KEY_PRESSED: u32 = 6;
const EVENT_PARAM_DIRECT_OBJECT: u32 = u32::from_be_bytes(*b"----");
const TYPE_EVENT_HOT_KEY_ID: u32 = u32::from_be_bytes(*b"hkid");
const NO_ERR: OSStatus = 0;

const CMD_KEY: u32 = 1 << 8;
const SHIFT_KEY: u32 = 1 << 9;
const OPTION_KEY: u32 = 1 << 11;
const CONTROL_KEY: u32 = 1 << 12;

#[repr(C)]
struct EventHotKeyID {
    signature: u32,
    id: u32,
}

#[repr(C)]
struct EventTypeSpec {
    event_class: u32,
    event_kind: u32,
}

type OSStatus = i32;
type EventTargetRef = *mut c_void;
type EventHandlerCallRef = *mut c_void;
type EventHandlerRef = *mut c_void;
type EventHotKeyRef = *mut c_void;
type EventRef = *mut c_void;
type EventHandlerProcPtr =
    unsafe extern "C" fn(EventHandlerCallRef, EventRef, *mut c_void) -> OSStatus;

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    fn RegisterEventHotKey(
        keyCode: u32,
        modifiers: u32,
        hotKeyID: EventHotKeyID,
        target: EventTargetRef,
        options: u32,
        outHotKeyRef: *mut EventHotKeyRef,
    ) -> OSStatus;

    fn UnregisterEventHotKey(hotKeyRef: EventHotKeyRef) -> OSStatus;

    fn InstallEventHandler(
        target: EventTargetRef,
        handler: EventHandlerProcPtr,
        numTypes: u32,
        typeList: *const EventTypeSpec,
        userData: *mut c_void,
        handlerRef: *mut EventHandlerRef,
    ) -> OSStatus;

    fn RemoveEventHandler(handlerRef: EventHandlerRef) -> OSStatus;

    fn GetApplicationEventTarget() -> EventTargetRef;

    fn GetEventParameter(
        event: EventRef,
        name: u32,
        desiredType: u32,
        actualType: *mut u32,
        size: u32,
        actualSize: *mut u32,
        data: *mut c_void,
    ) -> OSStatus;
}

const HOTKEY_DEFINITIONS: &[HotkeyDefinition] = &[
    HotkeyDefinition {
        action: ShortcutAction::TogglePinnedWindow,
        env_vars: &[PINNED_ENV_VAR],
        default: PINNED_DEFAULT,
    },
    HotkeyDefinition {
        action: ShortcutAction::ShowEditorSwitcher,
        env_vars: &[SWITCHER_ENV_VAR, LEGACY_ENV_VAR],
        default: SWITCHER_DEFAULT,
    },
];

struct HotkeyDefinition {
    action: ShortcutAction,
    env_vars: &'static [&'static str],
    default: &'static str,
}

pub struct GlobalHotkeys {
    handler_ref: EventHandlerRef,
    hotkey_refs: Vec<EventHotKeyRef>,
    context: *mut HotkeyContext,
}

impl Drop for GlobalHotkeys {
    fn drop(&mut self) {
        unsafe {
            for hotkey in &self.hotkey_refs {
                if !hotkey.is_null() {
                    let _ = UnregisterEventHotKey(*hotkey);
                }
            }
            if !self.handler_ref.is_null() {
                let _ = RemoveEventHandler(self.handler_ref);
            }
            if !self.context.is_null() {
                drop(Box::from_raw(self.context));
            }
        }
    }
}

impl fmt::Debug for GlobalHotkeys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GlobalHotkeys").finish()
    }
}

impl GlobalHotkeys {
    pub fn register(proxy: EventLoopProxy<EventPayload>) -> Option<Self> {
        let mut prepared: Vec<PreparedShortcut> = Vec::new();

        for definition in HOTKEY_DEFINITIONS {
            let choice = shortcut_choice(definition.env_vars, definition.default);
            let Some(shortcut_definition) = choice else {
                continue;
            };

            let shortcut = match ActivationShortcut::parse(&shortcut_definition) {
                Ok(shortcut) => shortcut,
                Err(err) => {
                    warn!(
                        "Failed to parse macOS hotkey '{}': {}",
                        shortcut_definition, err
                    );
                    continue;
                }
            };

            let registration = match HotkeyRegistration::from_shortcut(&shortcut) {
                Some(registration) => registration,
                None => {
                    warn!(
                        "macOS activation shortcut '{}' is not supported by the system hotkey API",
                        shortcut.describe()
                    );
                    continue;
                }
            };

            prepared.push(PreparedShortcut {
                action: definition.action,
                description: shortcut.describe(),
                registration,
            });
        }

        if prepared.is_empty() {
            return None;
        }

        unsafe {
            let context = Box::new(HotkeyContext {
                proxy,
                entries: Vec::new(),
            });
            let context_ptr = Box::into_raw(context);

            let mut seen_combinations = HashSet::new();

            let hotkey_event_type = EventTypeSpec {
                event_class: EVENT_CLASS_KEYBOARD,
                event_kind: EVENT_KIND_HOT_KEY_PRESSED,
            };

            let mut handler_ref: EventHandlerRef = ptr::null_mut();
            let status = InstallEventHandler(
                GetApplicationEventTarget(),
                hotkey_handler,
                1,
                &hotkey_event_type,
                context_ptr.cast(),
                &mut handler_ref,
            );

            if status != NO_ERR {
                warn!("Failed to install macOS hotkey handler: {}", status);
                drop(Box::from_raw(context_ptr));
                return None;
            }

            let mut hotkey_refs = Vec::new();

            for shortcut in prepared {
                let combo = (
                    shortcut.registration.key_code,
                    shortcut.registration.modifiers,
                );
                if !seen_combinations.insert(combo) {
                    warn!(
                        "Skipping macOS shortcut '{}' because that key combination is already assigned",
                        shortcut.description
                    );
                    continue;
                }

                let mut hotkey_ref: EventHotKeyRef = ptr::null_mut();
                let status = RegisterEventHotKey(
                    shortcut.registration.key_code,
                    shortcut.registration.modifiers,
                    EventHotKeyID {
                        signature: HOTKEY_SIGNATURE,
                        id: shortcut.action.id(),
                    },
                    GetApplicationEventTarget(),
                    0,
                    &mut hotkey_ref,
                );

                if status != NO_ERR {
                    warn!(
                        "Failed to register macOS hotkey '{}': {}",
                        shortcut.description, status
                    );
                    continue;
                }

                (*context_ptr).entries.push(ShortcutEntry {
                    id: shortcut.action.id(),
                    description: shortcut.description.clone(),
                    action: shortcut.action,
                });

                info!(
                    "Registered macOS activation shortcut: {}",
                    shortcut.description
                );

                hotkey_refs.push(hotkey_ref);
            }

            if hotkey_refs.is_empty() {
                let _ = RemoveEventHandler(handler_ref);
                drop(Box::from_raw(context_ptr));
                return None;
            }

            Some(Self {
                handler_ref,
                hotkey_refs,
                context: context_ptr,
            })
        }
    }
}

struct PreparedShortcut {
    action: ShortcutAction,
    description: String,
    registration: HotkeyRegistration,
}

struct HotkeyContext {
    proxy: EventLoopProxy<EventPayload>,
    entries: Vec<ShortcutEntry>,
}

struct ShortcutEntry {
    id: u32,
    description: String,
    action: ShortcutAction,
}

#[derive(Clone, Copy)]
enum ShortcutAction {
    TogglePinnedWindow,
    ShowEditorSwitcher,
}

impl ShortcutAction {
    fn id(self) -> u32 {
        match self {
            ShortcutAction::TogglePinnedWindow => 1,
            ShortcutAction::ShowEditorSwitcher => 2,
        }
    }

    fn command(self) -> MacShortcutCommand {
        match self {
            ShortcutAction::TogglePinnedWindow => MacShortcutCommand::TogglePinnedWindow,
            ShortcutAction::ShowEditorSwitcher => MacShortcutCommand::ShowEditorSwitcher,
        }
    }
}

unsafe extern "C" fn hotkey_handler(
    _next: EventHandlerCallRef,
    event: EventRef,
    user_data: *mut c_void,
) -> OSStatus {
    if user_data.is_null() {
        return NO_ERR;
    }

    let mut hotkey_id = MaybeUninit::<EventHotKeyID>::uninit();
    let status = GetEventParameter(
        event,
        EVENT_PARAM_DIRECT_OBJECT,
        TYPE_EVENT_HOT_KEY_ID,
        ptr::null_mut(),
        mem::size_of::<EventHotKeyID>() as u32,
        ptr::null_mut(),
        hotkey_id.as_mut_ptr().cast(),
    );

    if status != NO_ERR {
        return status;
    }

    let hotkey_id = hotkey_id.assume_init();
    if hotkey_id.signature != HOTKEY_SIGNATURE {
        return NO_ERR;
    }

    let context = &*(user_data as *mut HotkeyContext);
    if let Some(entry) = context
        .entries
        .iter()
        .find(|entry| entry.id == hotkey_id.id)
    {
        info!(
            "macOS activation shortcut detected; requesting focus ({})",
            entry.description
        );
        let payload = EventPayload::new(
            UserEvent::MacShortcut(entry.action.command()),
            WindowId::from(0),
        );
        let _ = context.proxy.send_event(payload);
    }

    NO_ERR
}

fn shortcut_choice<'a>(env_vars: &'a [&str], default: &'a str) -> Option<String> {
    for var in env_vars {
        match env::var(var) {
            Ok(value) => {
                let trimmed = value.trim();
                if trimmed.is_empty() || is_disabled_keyword(trimmed) {
                    info!("macOS activation shortcut from {} disabled", var);
                    return None;
                }
                return Some(value);
            }
            Err(VarError::NotPresent) => continue,
            Err(err) => {
                warn!("Failed to read {}: {}", var, err);
                continue;
            }
        }
    }
    Some(default.to_string())
}

fn is_disabled_keyword(value: &str) -> bool {
    matches_ignore_case(value, "off")
        || matches_ignore_case(value, "none")
        || matches_ignore_case(value, "disable")
        || matches_ignore_case(value, "disabled")
        || matches_ignore_case(value, "false")
}

fn matches_ignore_case(value: &str, keyword: &str) -> bool {
    value.eq_ignore_ascii_case(keyword)
}

#[derive(Clone, Copy)]
struct ActivationShortcut {
    key: ActivationKey,
    modifiers: NSEventModifierFlags,
}

impl ActivationShortcut {
    fn parse(input: &str) -> Result<Self, ShortcutParseError> {
        let mut modifiers = NSEventModifierFlags::empty();
        let mut key: Option<ActivationKey> = None;

        for raw_token in input.split('+') {
            let token = raw_token.trim();
            if token.is_empty() {
                continue;
            }

            if let Some(flag) = modifier_from_token(token) {
                modifiers |= flag;
                continue;
            }

            if key.is_some() {
                return Err(ShortcutParseError::DuplicateKey);
            }

            key = Some(ActivationKey::from_token(token)?);
        }

        Ok(Self {
            key: key.ok_or(ShortcutParseError::MissingKey)?,
            modifiers,
        })
    }

    fn describe(&self) -> String {
        let mut parts: Vec<String> = Vec::new();

        if self.modifiers.contains(NSEventModifierFlags::Command) {
            parts.push("cmd".into());
        }
        if self.modifiers.contains(NSEventModifierFlags::Control) {
            parts.push("ctrl".into());
        }
        if self.modifiers.contains(NSEventModifierFlags::Option) {
            parts.push("alt".into());
        }
        if self.modifiers.contains(NSEventModifierFlags::Shift) {
            parts.push("shift".into());
        }

        parts.push(self.key.describe());

        parts.join("+")
    }
}

struct HotkeyRegistration {
    key_code: u32,
    modifiers: u32,
}

impl HotkeyRegistration {
    fn from_shortcut(shortcut: &ActivationShortcut) -> Option<Self> {
        let key_code = shortcut.key.key_code()?;
        let modifiers = modifiers_to_carbon(shortcut.modifiers)?;

        Some(Self {
            key_code,
            modifiers,
        })
    }
}

fn modifiers_to_carbon(flags: NSEventModifierFlags) -> Option<u32> {
    let mut result = 0;

    if flags.contains(NSEventModifierFlags::Command) {
        result |= CMD_KEY;
    }
    if flags.contains(NSEventModifierFlags::Control) {
        result |= CONTROL_KEY;
    }
    if flags.contains(NSEventModifierFlags::Option) {
        result |= OPTION_KEY;
    }
    if flags.contains(NSEventModifierFlags::Shift) {
        result |= SHIFT_KEY;
    }

    Some(result)
}

fn modifier_from_token(token: &str) -> Option<NSEventModifierFlags> {
    let normalized = token.to_ascii_lowercase();
    match normalized.as_str() {
        "cmd" | "command" | "⌘" => Some(NSEventModifierFlags::Command),
        "ctrl" | "control" | "⌃" => Some(NSEventModifierFlags::Control),
        "alt" | "option" | "⌥" => Some(NSEventModifierFlags::Option),
        "shift" | "⇧" => Some(NSEventModifierFlags::Shift),
        _ => None,
    }
}

#[derive(Clone, Copy)]
enum ActivationKey {
    Character(char),
}

impl ActivationKey {
    fn from_token(token: &str) -> Result<Self, ShortcutParseError> {
        let mut chars = token.chars();
        let ch = chars
            .next()
            .ok_or(ShortcutParseError::MissingKey)?
            .to_ascii_lowercase();

        if chars.next().is_some() {
            return Err(ShortcutParseError::UnsupportedToken(token.to_string()));
        }

        Ok(Self::Character(ch))
    }

    fn key_code(&self) -> Option<u32> {
        match self {
            ActivationKey::Character(c) => keycode_for_char(*c),
        }
    }

    fn describe(&self) -> String {
        match self {
            ActivationKey::Character(c) => c.to_string(),
        }
    }
}

fn keycode_for_char(c: char) -> Option<u32> {
    match c.to_ascii_lowercase() {
        'a' => Some(0x00),
        'b' => Some(0x0B),
        'c' => Some(0x08),
        'd' => Some(0x02),
        'e' => Some(0x0E),
        'f' => Some(0x03),
        'g' => Some(0x05),
        'h' => Some(0x04),
        'i' => Some(0x22),
        'j' => Some(0x26),
        'k' => Some(0x28),
        'l' => Some(0x25),
        'm' => Some(0x2E),
        'n' => Some(0x2D),
        'o' => Some(0x1F),
        'p' => Some(0x23),
        'q' => Some(0x0C),
        'r' => Some(0x0F),
        's' => Some(0x01),
        't' => Some(0x11),
        'u' => Some(0x20),
        'v' => Some(0x09),
        'w' => Some(0x0D),
        'x' => Some(0x07),
        'y' => Some(0x10),
        'z' => Some(0x06),
        '0' => Some(0x1D),
        '1' => Some(0x12),
        '2' => Some(0x13),
        '3' => Some(0x14),
        '4' => Some(0x15),
        '5' => Some(0x17),
        '6' => Some(0x16),
        '7' => Some(0x1A),
        '8' => Some(0x1C),
        '9' => Some(0x19),
        _ => None,
    }
}

#[derive(Debug)]
enum ShortcutParseError {
    MissingKey,
    DuplicateKey,
    UnsupportedToken(String),
}

impl fmt::Display for ShortcutParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShortcutParseError::MissingKey => write!(f, "Shortcut is missing a key"),
            ShortcutParseError::DuplicateKey => {
                write!(f, "Shortcut already defines a key; only one is supported")
            }
            ShortcutParseError::UnsupportedToken(token) => {
                write!(f, "Unsupported token '{}'", token)
            }
        }
    }
}
