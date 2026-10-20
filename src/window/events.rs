use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

use winit::event::DeviceId;
use winit::window::Theme;

use crate::bridge::RestartDetails;
use crate::renderer::DrawCommand;
use crate::settings::HotReloadConfigs;
use crate::settings::SettingsChanged;

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub struct Pressure {
    device_id: DeviceId,
    pressure: f32,
    stage: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProgressBarUpdate {
    pub id: Option<String>,
    pub source: Option<String>,
    pub status: Option<String>,
    pub percent: Option<f32>,
}

impl ProgressBarUpdate {
    /// neovim stable and nightly encode the same save event differently.
    /// for example stable uses id = "bufwrite" with percent = 0 while nightly uses
    /// id = "nvim.bufwrite [file]" without a percent field.
    /// A determinate percentage on either shape is not treated as built-in save progress.
    pub fn is_neovim_bufwrite(&self) -> bool {
        self.source.as_deref() == Some("nvim")
            && match self.id.as_deref() {
                Some("bufwrite") => self.percent == Some(0.0), // stable
                Some(id) if id.starts_with("nvim.bufwrite ") => self.percent.is_none(), // nightly
                _ => false,
            }
    }
}

#[cfg(target_os = "macos")]
#[derive(Clone, Debug, PartialEq)]
pub enum ForceClickKind {
    Text,
    Url,
    File,
}

#[cfg(target_os = "macos")]
impl From<&str> for ForceClickKind {
    fn from(value: &str) -> Self {
        match value {
            "url" => ForceClickKind::Url,
            "file" => ForceClickKind::File,
            _ => ForceClickKind::Text,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum WindowCommand {
    TitleChanged(String),
    SetMouseEnabled(bool),
    ListAvailableFonts,
    FocusWindow,
    #[cfg(target_os = "macos")]
    TouchpadPressure {
        col: i64,
        row: i64,
        entity: String,
        guifont: String,
        kind: ForceClickKind,
    },
    #[cfg(target_os = "macos")]
    HighlightMatchingPair {
        grid: u64,
        row: u64,
        column: u64,
        text: Option<String>,
    },
    #[cfg(target_os = "macos")]
    DocumentStateChanged {
        path: String,
        modified: bool,
    },
    Minimize,
    ThemeChanged(Option<Theme>),
    #[cfg(windows)]
    RegisterRightClick,
    #[cfg(windows)]
    UnregisterRightClick,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Debug, PartialEq)]
pub enum MacShortcutCommand {
    TogglePinnedWindow,
    ShowEditorSwitcher,
}

#[derive(Clone, Debug, PartialEq)]
pub enum UserEvent {
    DrawCommandBatch(Vec<DrawCommand>),
    #[cfg(target_os = "macos")]
    OpenFiles {
        files: Vec<String>,
        cwd: Option<String>,
        caller_cwd: Option<String>,
        tabs: bool,
        new_window: bool,
        neovim_bin: Option<String>,
        neovim_args: Option<Vec<String>>,
    },
    WindowCommand(WindowCommand),
    SettingsChanged(SettingsChanged),
    ConfigsChanged(Box<HotReloadConfigs>),
    #[allow(dead_code)]
    RedrawRequested,
    NeovimExited,
    NeovimLaunchError {
        message: String,
    },
    NeovimRestart(RestartDetails),
    ShowProgressBar {
        update: ProgressBarUpdate,
    },
    #[cfg(target_os = "macos")]
    CreateWindow,
    #[cfg(target_os = "macos")]
    ActivateWindow(winit::window::WindowId),
    #[cfg(target_os = "macos")]
    MacShortcut(MacShortcutCommand),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventTarget {
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Window(winit::window::WindowId),
    Route(RouteId),
    Focused,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RouteId(u64);

impl RouteId {
    pub fn next() -> Self {
        static NEXT_ROUTE_ID: AtomicU64 = AtomicU64::new(1);
        Self(NEXT_ROUTE_ID.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Debug, Clone)]
pub struct EventPayload {
    pub payload: UserEvent,
    pub target: EventTarget,
}

impl EventPayload {
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub fn for_window(payload: UserEvent, window_id: winit::window::WindowId) -> Self {
        Self { payload, target: EventTarget::Window(window_id) }
    }

    pub fn for_route(payload: UserEvent, route_id: RouteId) -> Self {
        Self { payload, target: EventTarget::Route(route_id) }
    }

    pub fn all(payload: UserEvent) -> Self { Self { payload, target: EventTarget::All } }
}

impl From<Vec<DrawCommand>> for UserEvent {
    fn from(value: Vec<DrawCommand>) -> Self { UserEvent::DrawCommandBatch(value) }
}

impl From<WindowCommand> for UserEvent {
    fn from(value: WindowCommand) -> Self { UserEvent::WindowCommand(value) }
}

#[cfg(target_os = "macos")]
impl From<MacShortcutCommand> for EventPayload {
    fn from(value: MacShortcutCommand) -> Self { EventPayload::all(UserEvent::MacShortcut(value)) }
}

impl From<SettingsChanged> for UserEvent {
    fn from(value: SettingsChanged) -> Self { UserEvent::SettingsChanged(value) }
}

impl From<HotReloadConfigs> for UserEvent {
    fn from(value: HotReloadConfigs) -> Self { UserEvent::ConfigsChanged(Box::new(value)) }
}
