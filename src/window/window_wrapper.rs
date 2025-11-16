#[cfg(target_os = "macos")]
use std::collections::VecDeque;
use std::{cell::RefCell, fmt, rc::Rc, sync::Arc};

use log::trace;
#[cfg(target_os = "macos")]
use log::warn;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use rustc_hash::FxHashMap;
use winit::{
    dpi,
    event::{Ime, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoopProxy},
    window::{Fullscreen, Theme, Window, WindowId},
};
#[cfg(target_os = "macos")]
use winit::{
    event::{ElementState, KeyEvent, Modifiers},
    keyboard::{Key, NamedKey},
};

use super::{
    EventPayload, EventTarget, KeyboardManager, MouseManager, UserEvent, WindowCommand,
    WindowSettings, WindowSettingsChanged, WindowSize,
};

#[cfg(target_os = "macos")]
use {
    crate::units::{GridPos, Pixel},
    crate::{error_msg, window::settings},
    glamour::Point2,
    winit::platform::macos::{self, WindowExtMacOS},
};

#[cfg(target_os = "macos")]
use super::MacShortcutCommand;

use crate::{
    bridge::{
        send_ui, NeovimHandler, NeovimRuntime, ParallelCommand, RestartDetails, SerialCommand,
    },
    clipboard::ClipboardHandle,
    profiling::{tracy_frame, tracy_gpu_collect, tracy_gpu_zone, tracy_plot, tracy_zone},
    renderer::{
        create_skia_renderer, DrawCommand, Renderer, RendererSettingsChanged, SkiaRenderer, VSync,
    },
    running_tracker::RunningTracker,
    settings::{
        clamped_grid_size, font::FontSettings, load_last_window_settings, Config, HotReloadConfigs,
        Settings, SettingsChanged, DEFAULT_GRID_SIZE, MIN_GRID_SIZE,
    },
    units::{GridRect, GridScale, GridSize, PixelPos, PixelSize},
    window::{
        create_window, determine_grid_size, determine_window_size, PhysicalSize, ShouldRender,
        ThemeSettings,
    },
    CmdLineSettings,
};

#[cfg(windows)]
use {
    crate::windows_utils::{register_right_click, unregister_right_click},
    winit::platform::windows::{BackdropType, Color, WindowExtWindows},
};

#[cfg(target_os = "macos")]
use super::macos::{
    hide_application, is_focus_suppressed, is_tab_overview_active, native_tab_bar_enabled,
    trigger_tab_overview, MacosWindowFeature, TouchpadStage,
};

const GRID_TOLERANCE: f32 = 1e-3;

fn round_or_op<Op: FnOnce(f32) -> f32>(v: f32, op: Op) -> f32 {
    let rounded = v.round();
    if v.abs_diff_eq(&rounded, GRID_TOLERANCE) {
        rounded
    } else {
        op(v)
    }
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
enum TabNavigationAction {
    Next,
    Previous,
}

#[cfg(target_os = "macos")]
#[derive(Clone)]
struct TabNavigationHotkeys {
    next: Option<KeyCombo>,
    prev: Option<KeyCombo>,
}

#[cfg(target_os = "macos")]
impl TabNavigationHotkeys {
    fn new(settings: &Settings) -> Self {
        let cmdline = settings.get::<CmdLineSettings>();
        Self {
            next: KeyCombo::parse(&cmdline.macos_tab_next_hotkey),
            prev: KeyCombo::parse(&cmdline.macos_tab_prev_hotkey),
        }
    }

    fn action_for(&self, event: &KeyEvent, modifiers: &Modifiers) -> Option<TabNavigationAction> {
        if event.state != ElementState::Pressed {
            return None;
        }

        if let Some(combo) = &self.next {
            if combo.matches(event, modifiers) {
                return Some(TabNavigationAction::Next);
            }
        }

        if let Some(combo) = &self.prev {
            if combo.matches(event, modifiers) {
                return Some(TabNavigationAction::Previous);
            }
        }

        None
    }
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
struct KeyCombo {
    command: bool,
    control: bool,
    option: bool,
    shift: bool,
    key: KeyMatch,
}

#[cfg(target_os = "macos")]
impl KeyCombo {
    fn parse(raw: &str) -> Option<Self> {
        let trimmed = raw.trim();
        if trimmed.is_empty() || is_disabled_keyword(trimmed) {
            return None;
        }

        let mut command = false;
        let mut control = false;
        let mut option = false;
        let mut shift = false;
        let mut key: Option<KeyMatch> = None;

        for token in trimmed
            .split('+')
            .map(|part| part.trim())
            .filter(|t| !t.is_empty())
        {
            let normalized = token.to_ascii_lowercase();
            match normalized.as_str() {
                "cmd" | "command" | "⌘" => command = true,
                "ctrl" | "control" | "⌃" => control = true,
                "alt" | "option" | "⌥" => option = true,
                "shift" | "⇧" => shift = true,
                "left" | "←" => {
                    if key.is_some() {
                        warn!(
                            "macOS tab navigation shortcut '{}' has multiple keys; ignoring",
                            raw
                        );
                        return None;
                    }
                    key = Some(KeyMatch::Named(NamedKey::ArrowLeft));
                }
                "right" | "→" => {
                    if key.is_some() {
                        warn!(
                            "macOS tab navigation shortcut '{}' has multiple keys; ignoring",
                            raw
                        );
                        return None;
                    }
                    key = Some(KeyMatch::Named(NamedKey::ArrowRight));
                }
                "up" | "↑" => {
                    if key.is_some() {
                        warn!(
                            "macOS tab navigation shortcut '{}' has multiple keys; ignoring",
                            raw
                        );
                        return None;
                    }
                    key = Some(KeyMatch::Named(NamedKey::ArrowUp));
                }
                "down" | "↓" => {
                    if key.is_some() {
                        warn!(
                            "macOS tab navigation shortcut '{}' has multiple keys; ignoring",
                            raw
                        );
                        return None;
                    }
                    key = Some(KeyMatch::Named(NamedKey::ArrowDown));
                }
                value => {
                    if key.is_some() {
                        warn!(
                            "macOS tab navigation shortcut '{}' has multiple keys; ignoring",
                            raw
                        );
                        return None;
                    }
                    if value.chars().count() != 1 {
                        warn!(
                            "macOS tab navigation shortcut '{}' must end with a single character key; ignoring",
                            raw
                        );
                        return None;
                    }
                    let ch = match value.chars().next().map(|c| c.to_ascii_lowercase()) {
                        Some(ch) => ch,
                        None => {
                            warn!(
                                "macOS tab navigation shortcut '{}' has no key; ignoring",
                                raw
                            );
                            return None;
                        }
                    };
                    key = Some(KeyMatch::Char(ch));
                }
            }
        }

        let Some(key) = key else {
            warn!(
                "macOS tab navigation shortcut '{}' is missing a key component; ignoring",
                raw
            );
            return None;
        };

        Some(Self {
            command,
            control,
            option,
            shift,
            key,
        })
    }

    fn matches(&self, event: &KeyEvent, modifiers: &Modifiers) -> bool {
        let state = modifiers.state();
        if self.command != state.super_key()
            || self.control != state.control_key()
            || self.option != state.alt_key()
            || self.shift != state.shift_key()
        {
            return false;
        }

        match self.key {
            KeyMatch::Char(expected) => {
                let pressed_key = event
                    .text
                    .as_ref()
                    .and_then(|text| text.chars().next())
                    .or_else(|| match event.logical_key.as_ref() {
                        Key::Character(text) if !text.is_empty() => text.chars().next(),
                        _ => None,
                    });

                pressed_key
                    .map(|c| c.to_ascii_lowercase() == expected)
                    .unwrap_or(false)
            }
            KeyMatch::Named(expected) => matches!(
                event.logical_key.as_ref(),
                Key::Named(named) if named == expected
            ),
        }
    }
}

#[cfg(target_os = "macos")]
fn is_disabled_keyword(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "off" | "none" | "disable" | "disabled" | "false" | ""
    )
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
enum KeyMatch {
    Char(char),
    Named(NamedKey),
}

use approx::AbsDiffEq;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct WindowPadding {
    pub top: u32,
    pub left: u32,
    pub right: u32,
    pub bottom: u32,
}

#[derive(PartialEq, PartialOrd)]
enum UIState {
    Initing, // Running init.vim/lua
    WaitingForWindowCreate,
    FirstFrame,
    Showing, // No pending resizes
}

pub struct RouteWindow {
    pub skia_renderer: Rc<RefCell<Box<dyn SkiaRenderer>>>,
    pub winit_window: Rc<Window>,
    pub neovim_handler: NeovimHandler,
    pub mouse_manager: Rc<RefCell<Box<MouseManager>>>,
    pub renderer: Rc<RefCell<Box<Renderer>>>,
    #[cfg(target_os = "macos")]
    pub macos_feature: Option<Rc<RefCell<Box<MacosWindowFeature>>>>,
    pub title: String,
    pub last_applied_window_size: dpi::PhysicalSize<u32>,
    pub last_synced_grid_size: Option<GridSize<u32>>,
}

impl fmt::Debug for RouteWindow {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("RouteWindow")
            .field("skia_renderer", &"...") // Custom debug output since Box<dyn SkiaRenderer> is a trait object
            .field("winit_window", &self.winit_window)
            .field("neovim_handler", &self.neovim_handler)
            .finish()
    }
}

pub struct Route {
    pub window: RouteWindow,
    pub pending_initial_window_size: Option<WindowSize>,
    state: RouteState,
}

impl fmt::Debug for Route {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Route")
            .field("window", &self.window)
            .field("state", &"...")
            .finish()
    }
}

struct RouteState {
    font_changed_last_frame: bool,
    saved_inner_size: dpi::PhysicalSize<u32>,
    saved_grid_size: Option<GridSize<u32>>,
    requested_columns: Option<u32>,
    requested_lines: Option<u32>,
    window_padding: WindowPadding,
    is_minimized: bool,
    ime_enabled: bool,
    ime_area: (dpi::PhysicalPosition<u32>, dpi::PhysicalSize<u32>),
    inferred_theme: Option<Theme>,
    vsync: Option<VSync>,
}

impl RouteState {
    fn new() -> Self {
        Self {
            font_changed_last_frame: false,
            saved_inner_size: Default::default(),
            saved_grid_size: None,
            requested_columns: None,
            requested_lines: None,
            window_padding: WindowPadding {
                left: 0,
                right: 0,
                top: 0,
                bottom: 0,
            },
            is_minimized: false,
            ime_enabled: false,
            ime_area: Default::default(),
            inferred_theme: None,
            vsync: None,
        }
    }
}

#[derive(Clone)]
struct RestartRequest {
    details: RestartDetails,
    grid_size: GridSize<u32>,
}

pub struct WinitWindowWrapper {
    // Don't rearrange this, unless you have a good reason to do so
    // The destruction order has to be correct
    pub routes: FxHashMap<WindowId, Route>,
    pub runtime: Option<NeovimRuntime>,
    pub runtime_tracker: RunningTracker,
    pending_restart: FxHashMap<WindowId, RestartRequest>,
    keyboard_manager: KeyboardManager,
    ui_state: UIState,

    settings: Arc<Settings>,
    colorscheme_stream: Option<mundy::PreferencesStream>,

    #[cfg(target_os = "macos")]
    window_mru: VecDeque<WindowId>,
    #[cfg(target_os = "macos")]
    focus_return_target: Option<WindowId>,
    #[cfg(target_os = "macos")]
    ignore_next_focus_gain: bool,
    #[cfg(target_os = "macos")]
    tab_navigation_hotkeys: TabNavigationHotkeys,
}

impl WinitWindowWrapper {
    pub fn new(
        _initial_font_settings: Option<FontSettings>,
        settings: Arc<Settings>,
        runtime_tracker: RunningTracker,
        colorscheme_stream: mundy::PreferencesStream,
        clipboard_handle: ClipboardHandle,
    ) -> Self {
        let runtime =
            NeovimRuntime::new(clipboard_handle).expect("Failed to create neovim runtime");

        Self {
            routes: Default::default(),
            runtime: Some(runtime),
            runtime_tracker,
            pending_restart: FxHashMap::default(),
            keyboard_manager: KeyboardManager::new(settings.clone()),
            ui_state: UIState::Initing,
            settings: settings.clone(),
            colorscheme_stream: Some(colorscheme_stream),
            #[cfg(target_os = "macos")]
            window_mru: VecDeque::new(),
            #[cfg(target_os = "macos")]
            focus_return_target: None,
            #[cfg(target_os = "macos")]
            ignore_next_focus_gain: false,
            #[cfg(target_os = "macos")]
            tab_navigation_hotkeys: TabNavigationHotkeys::new(settings.as_ref()),
        }
    }

    pub fn request_window_creation(&mut self) {
        if self.routes.is_empty() && self.ui_state == UIState::Initing {
            self.ui_state = UIState::WaitingForWindowCreate;
        }
    }

    fn take_colorscheme_stream(&mut self) -> mundy::PreferencesStream {
        self.colorscheme_stream
            .take()
            .unwrap_or_else(|| mundy::Preferences::stream(mundy::Interest::ColorScheme))
    }

    pub fn exit(&mut self) {
        for route in self.routes.values_mut() {
            route.state.vsync = None;
        }
    }

    pub fn set_fullscreen(&mut self, fullscreen: bool) {
        let Some(route) = self.focused_route() else {
            return;
        };
        let window = route.window.winit_window.clone();
        if fullscreen {
            let handle = window.current_monitor();
            window.set_fullscreen(Some(Fullscreen::Borderless(handle)));
        } else {
            window.set_fullscreen(None);
        }
    }

    #[cfg(target_os = "macos")]
    pub fn set_macos_option_as_meta(&mut self, option: settings::OptionAsMeta) {
        let winit_option = match option {
            settings::OptionAsMeta::OnlyLeft => macos::OptionAsAlt::OnlyLeft,
            settings::OptionAsMeta::OnlyRight => macos::OptionAsAlt::OnlyRight,
            settings::OptionAsMeta::Both => macos::OptionAsAlt::Both,
            settings::OptionAsMeta::None => macos::OptionAsAlt::None,
        };

        let Some(route) = self.focused_route() else {
            return;
        };
        let window = route.window.winit_window.clone();
        if winit_option != window.option_as_alt() {
            window.set_option_as_alt(winit_option);
        }
    }

    #[cfg(target_os = "macos")]
    pub fn set_simple_fullscreen(&mut self, fullscreen: bool) {
        let Some(route) = self.focused_route() else {
            return;
        };
        if let Some(feature) = &route.window.macos_feature {
            if fullscreen {
                feature.borrow_mut().set_simple_fullscreen_mode(true);
                route.window.winit_window.set_simple_fullscreen(true);
            } else {
                route.window.winit_window.set_simple_fullscreen(false);
                feature.borrow_mut().set_simple_fullscreen_mode(false);
            }
        } else {
            route.window.winit_window.set_simple_fullscreen(fullscreen);
        }
    }

    pub fn minimize_window(&mut self) {
        let Some(route) = self.focused_route() else {
            return;
        };
        let window = route.window.winit_window.clone();
        window.set_minimized(true);
    }

    pub fn set_ime(&mut self, ime_enabled: bool) {
        let Some(route) = self.focused_route() else {
            return;
        };
        let window = route.window.winit_window.clone();
        window.set_ime_allowed(ime_enabled);
    }

    pub fn handle_window_command(&mut self, target: EventTarget, command: WindowCommand) {
        tracy_zone!("handle_window_commands", 0);
        let Some(target_window_id) = self.resolve_target_window_id(target) else {
            return;
        };

        match command {
            WindowCommand::TitleChanged(new_title) => {
                self.handle_title_changed(target_window_id, new_title)
            }
            WindowCommand::SetMouseEnabled(mouse_enabled) => {
                if let Some(route) = self.routes.get(&target_window_id) {
                    let mut mouse_manager = route.window.mouse_manager.borrow_mut();
                    mouse_manager.enabled = mouse_enabled;
                }
            }
            WindowCommand::ListAvailableFonts => self.send_font_names(target_window_id),
            WindowCommand::FocusWindow => {
                if let Some(route) = &self.routes.get(&target_window_id) {
                    let window = route.window.winit_window.clone();
                    window.focus_window();
                    #[cfg(target_os = "macos")]
                    if let Some(feature) = &route.window.macos_feature {
                        feature.borrow().activate_application();
                    }
                }
            }
            #[cfg(target_os = "macos")]
            WindowCommand::TouchpadPressure {
                col,
                row,
                entity,
                guifont,
                kind,
            } => {
                let Some(macos_feature) = self
                    .routes
                    .get(&target_window_id)
                    .and_then(|route| route.window.macos_feature.clone())
                else {
                    log::warn!("Touchpad pressure received before macOS feature initialization");
                    return;
                };

                let titlebar_height = macos_feature.borrow().system_titlebar_height as f32;
                let window_padding = self.calculate_window_padding(target_window_id);
                let pixel_position = self.grid_to_pixel_position(target_window_id, col, row);
                let Some(grid_scale_height) = self.routes.get(&target_window_id).map(|route| {
                    let renderer = route.window.renderer.borrow();
                    renderer.grid_renderer.grid_scale.height()
                }) else {
                    return;
                };
                let point =
                    self.apply_padding_to_position(pixel_position, window_padding, titlebar_height);

                macos_feature.borrow_mut().handle_force_click_target(
                    &entity,
                    kind,
                    point,
                    guifont,
                    grid_scale_height,
                );
            }
            WindowCommand::Minimize => {
                self.minimize_window();
                if let Some(route) = self.routes.get_mut(&target_window_id) {
                    route.state.is_minimized = true;
                }
            }
            WindowCommand::ThemeChanged(new_theme) => {
                if let Some(route) = self.routes.get_mut(&target_window_id) {
                    if route.state.inferred_theme != new_theme {
                        route.state.inferred_theme = new_theme;
                        let WindowSettings { theme, .. } = self.settings.get::<WindowSettings>();
                        if matches!(theme, ThemeSettings::BgColor) {
                            self.apply_theme_for_window(target_window_id);
                        }
                    }
                }
            }
            #[cfg(windows)]
            WindowCommand::RegisterRightClick => register_right_click(),
            #[cfg(windows)]
            WindowCommand::UnregisterRightClick => unregister_right_click(),
        }
    }

    pub fn handle_window_settings_changed(&mut self, changed_setting: WindowSettingsChanged) {
        tracy_zone!("handle_window_settings_changed");
        match changed_setting {
            WindowSettingsChanged::ObservedColumns(columns) => {
                log::info!("columns changed");
                if let Some(route) = self.focused_route_mut() {
                    route.state.requested_columns = columns.and_then(|v| match u32::try_from(v) {
                        Ok(value) => Some(value),
                        Err(_) => {
                            log::warn!("Invalid columns value {v}, ignoring");
                            None
                        }
                    });
                }
            }
            WindowSettingsChanged::ObservedLines(lines) => {
                log::info!("lines changed");
                if let Some(route) = self.focused_route_mut() {
                    route.state.requested_lines = lines.and_then(|v| match u32::try_from(v) {
                        Ok(value) => Some(value),
                        Err(_) => {
                            log::warn!("Invalid lines value {v}, ignoring");
                            None
                        }
                    });
                }
            }
            WindowSettingsChanged::Fullscreen(fullscreen) => {
                self.set_fullscreen(fullscreen);
            }
            WindowSettingsChanged::InputIme(ime_enabled) => {
                self.set_ime(ime_enabled);
            }
            WindowSettingsChanged::ScaleFactor(user_scale_factor) => {
                if let Some(route) = self.focused_route_mut() {
                    let mut renderer = route.window.renderer.borrow_mut();
                    let scale_factor = renderer.os_scale_factor;
                    let renderer_user_scale_factor = renderer.user_scale_factor;
                    renderer.user_scale_factor = user_scale_factor.into();
                    renderer
                        .grid_renderer
                        .handle_scale_factor_update(scale_factor * renderer_user_scale_factor);
                    route.state.font_changed_last_frame = true;
                }
            }
            WindowSettingsChanged::WindowBlurred(blur) => {
                let Some(route) = self.focused_route() else {
                    return;
                };
                let window = route.window.winit_window.clone();
                let WindowSettings { opacity, .. } = self.settings.get::<WindowSettings>();
                let transparent = opacity < 1.0;
                window.set_blur(blur && transparent);
            }
            WindowSettingsChanged::Opacity(..) | WindowSettingsChanged::NormalOpacity(..) => {
                let Some(route) = self.focused_route() else {
                    return;
                };
                let mut renderer = route.window.renderer.borrow_mut();
                renderer.prepare_lines(true);
            }
            WindowSettingsChanged::Theme(..) => {
                self.handle_theme_changed();
            }
            #[cfg(target_os = "windows")]
            WindowSettingsChanged::TitleBackgroundColor(color) => {
                self.handle_title_background_color(&color);
            }

            #[cfg(target_os = "windows")]
            WindowSettingsChanged::TitleTextColor(color) => {
                self.handle_title_text_color(&color);
            }

            #[cfg(target_os = "macos")]
            WindowSettingsChanged::InputMacosOptionKeyIsMeta(option) => {
                self.set_macos_option_as_meta(option);
            }

            #[cfg(target_os = "macos")]
            WindowSettingsChanged::InputMacosAltIsMeta(enabled) => {
                if enabled {
                    error_msg!(concat!(
                        "neovide_input_macos_alt_is_meta has now been removed. ",
                        "Use neovide_input_macos_option_key_is_meta instead. ",
                        "Please check https://neovide.dev/configuration.html#macos-option-key-is-meta for more information.",
                    ));
                }
            }
            #[cfg(target_os = "macos")]
            WindowSettingsChanged::MacosSimpleFullscreen(fullscreen) => {
                self.set_simple_fullscreen(fullscreen);
            }
            _ => {}
        }
        #[cfg(target_os = "macos")]
        {
            if let Some(macos_feature) = self
                .focused_route()
                .and_then(|route| route.window.macos_feature.as_ref())
            {
                macos_feature
                    .borrow_mut()
                    .handle_settings_changed(changed_setting);
            }
        }
    }

    fn handle_render_settings_changed(&mut self, changed_setting: RendererSettingsChanged) {
        match changed_setting {
            RendererSettingsChanged::TextGamma(..) | RendererSettingsChanged::TextContrast(..) => {
                for route in self.routes.values_mut() {
                    let mut skia_renderer = route.window.skia_renderer.borrow_mut();
                    skia_renderer.resize();
                    route.state.font_changed_last_frame = true;
                }
            }
            _ => {}
        }
    }

    pub fn handle_title_changed(&mut self, window_id: WindowId, new_title: String) {
        if let Some(route) = self.routes.get_mut(&window_id) {
            let window = route.window.winit_window.clone();
            route.window.title = new_title.clone();
            window.set_title(&new_title);
        }
    }

    fn get_theme_for(&self, inferred_theme: Option<Theme>) -> Option<Theme> {
        let WindowSettings { theme, .. } = self.settings.get::<WindowSettings>();
        match theme {
            ThemeSettings::Auto => None,
            ThemeSettings::Light => Some(Theme::Light),
            ThemeSettings::Dark => Some(Theme::Dark),
            ThemeSettings::BgColor => inferred_theme,
        }
    }

    fn apply_theme_for_window(&self, window_id: WindowId) {
        if let Some(route) = self.routes.get(&window_id) {
            let window = route.window.winit_window.clone();
            let theme = self.get_theme_for(route.state.inferred_theme);
            window.set_theme(theme);
        }
    }

    pub fn handle_theme_changed(&mut self) {
        for window_id in self.routes.keys().copied().collect::<Vec<_>>() {
            self.apply_theme_for_window(window_id);
        }
    }

    pub fn send_font_names(&self, window_id: WindowId) {
        let Some(route) = self.routes.get(&window_id) else {
            return;
        };
        let renderer = route.window.renderer.borrow();
        let neovim_handler = &route.window.neovim_handler;
        let font_names = renderer.font_names();
        send_ui(
            ParallelCommand::DisplayAvailableFonts(font_names),
            neovim_handler,
        );
    }

    pub fn handle_quit(&mut self, window_id: WindowId) {
        let Some(route) = self.routes.get(&window_id) else {
            return;
        };
        let neovim_handler = &route.window.neovim_handler;
        send_ui(ParallelCommand::Quit, neovim_handler);
    }

    pub fn handle_focus_lost(&mut self, window_id: WindowId) {
        let Some(route) = self.routes.get(&window_id) else {
            return;
        };
        let neovim_handler = &route.window.neovim_handler;
        send_ui(ParallelCommand::FocusLost, neovim_handler);
    }

    pub fn handle_focus_gained(&mut self, window_id: WindowId) {
        {
            let Some(route) = self.routes.get(&window_id) else {
                return;
            };
            let neovim_handler = &route.window.neovim_handler;
            send_ui(ParallelCommand::FocusGained, neovim_handler);
            // Got focus back after being minimized previously
            if route.state.is_minimized {
                // Sending <NOP> after suspend triggers the `VimResume` AutoCmd
                send_ui(SerialCommand::Keyboard("<NOP>".into()), neovim_handler);

                if let Some(route) = self.routes.get_mut(&window_id) {
                    route.state.is_minimized = false;
                }
            }
        }
        #[cfg(target_os = "macos")]
        self.handle_focus_gain_for_shortcuts(window_id);
    }

    pub fn handle_window_event(&mut self, window_id: WindowId, event: WindowEvent) -> bool {
        // Events can still arrive after the associated window has been torn down.
        let Some(route_entry) = self.routes.get_mut(&window_id) else {
            return false;
        };

        // The renderer and vsync should always be created when a window event is received
        let route = route_entry;
        let neovim_handler = &route.window.neovim_handler;

        #[cfg(target_os = "macos")]
        let mut consumed_key_event = false;
        #[cfg(target_os = "macos")]
        {
            if native_tab_bar_enabled() {
                if let WindowEvent::KeyboardInput {
                    event: ref key_event,
                    ..
                } = event
                {
                    let modifiers = self.keyboard_manager.current_modifiers();
                    if let Some(action) = self
                        .tab_navigation_hotkeys
                        .action_for(key_event, &modifiers)
                    {
                        if let Some(feature) = &route.window.macos_feature {
                            let feature_ref = feature.borrow();
                            if feature_ref.can_navigate_tabs() {
                                match action {
                                    TabNavigationAction::Next => feature_ref.select_next_tab(),
                                    TabNavigationAction::Previous => {
                                        feature_ref.select_previous_tab()
                                    }
                                }
                                consumed_key_event = true;
                            }
                        }
                    }
                }
            }
        }

        {
            let mut mouse_manager = route.window.mouse_manager.borrow_mut();
            let window = route.window.winit_window.clone();
            let renderer = route.window.renderer.borrow_mut();
            mouse_manager.handle_event(
                &event,
                &self.keyboard_manager,
                &renderer,
                &window,
                neovim_handler,
            );
        }

        #[cfg(target_os = "macos")]
        if !consumed_key_event {
            self.keyboard_manager.handle_event(&event, neovim_handler);
        }
        #[cfg(not(target_os = "macos"))]
        self.keyboard_manager.handle_event(&event, neovim_handler);
        {
            let mut renderer = route.window.renderer.borrow_mut();
            renderer.handle_event(&event);
        }
        let mut should_render = true;
        let mut pending_focus_event: Option<bool> = None;

        match event {
            WindowEvent::CloseRequested => {
                tracy_zone!("CloseRequested");
                self.handle_quit(window_id);
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                tracy_zone!("ScaleFactorChanged");
                self.handle_scale_factor_update(window_id, scale_factor);
            }
            WindowEvent::Resized { .. } => {
                let mut skia_renderer = route.window.skia_renderer.borrow_mut();
                skia_renderer.resize();
                #[cfg(target_os = "macos")]
                {
                    if let Some(macos_feature) = &mut route.window.macos_feature {
                        macos_feature.borrow_mut().handle_size_changed();
                    }
                }
            }
            WindowEvent::DroppedFile(path) => {
                tracy_zone!("DroppedFile");
                let file_path = path.into_os_string().into_string().unwrap_or_else(|path| {
                    let lossy = path.to_string_lossy().to_string();
                    log::warn!("Dropped file path was not valid UTF-8; using lossy path");
                    lossy
                });
                send_ui(ParallelCommand::FileDrop(file_path), neovim_handler);
            }
            WindowEvent::Focused(focus) => {
                tracy_zone!("Focused");
                pending_focus_event = Some(focus);
            }
            WindowEvent::Moved(_) => {
                tracy_zone!("Moved");
                let window = route.window.winit_window.clone();
                if let Some(vsync) = route.state.vsync.as_mut() {
                    vsync.update(&window);
                }
            }
            #[cfg(target_os = "macos")]
            WindowEvent::TouchpadPressure { stage, .. } => {
                tracy_zone!("TouchpadPressure");
                if let Some(macos_feature) = &route.window.macos_feature {
                    match TouchpadStage::from_stage(stage) {
                        TouchpadStage::Soft | TouchpadStage::Click => {
                            macos_feature.borrow_mut().set_definition_is_active(false)
                        }
                        TouchpadStage::ForceClick => {
                            macos_feature.borrow_mut().handle_touchpad_force_click()
                        }
                    }
                }
            }
            WindowEvent::Ime(Ime::Enabled) => {
                log::info!("Ime enabled");
                route.state.ime_enabled = true;
                self.update_ime_position(window_id, true);
            }
            WindowEvent::Ime(Ime::Disabled) => {
                log::info!("Ime disabled");
                route.state.ime_enabled = false;
            }
            _ => {
                tracy_zone!("Unknown WindowEvent");
                should_render = false;
            }
        }

        if let Some(focus) = pending_focus_event {
            #[cfg(target_os = "macos")]
            {
                if is_focus_suppressed() {
                    log::trace!(
                        "Suppressing focus event during tab detach (focus = {})",
                        focus
                    );
                    return self.ui_state >= UIState::FirstFrame && should_render;
                }
                if focus {
                    if let Some(route) = self.routes.get(&window_id) {
                        let ns_window =
                            crate::window::macos::get_ns_window(route.window.winit_window.as_ref());
                        let host_ptr = crate::window::macos::get_last_host_window();
                        let window_ptr =
                            crate::window::macos::window_identifier(ns_window.as_ref());
                        if host_ptr != 0 && window_ptr != host_ptr {
                            log::trace!(
                                "Focus gained for non-host window; refocusing host {:?}",
                                host_ptr
                            );
                            ns_window.makeKeyAndOrderFront(None);
                            ns_window.orderFrontRegardless();
                        }
                    }
                }
            }

            if focus {
                self.handle_focus_gained(window_id);
            } else {
                self.handle_focus_lost(window_id);
            }
        }

        self.ui_state >= UIState::FirstFrame && should_render
    }

    pub fn handle_user_event(&mut self, event: EventPayload) {
        let EventPayload { payload, target } = event;
        let needs_window = matches!(
            payload,
            UserEvent::DrawCommandBatch(_)
                | UserEvent::WindowCommand(_)
                | UserEvent::SettingsChanged(_)
                | UserEvent::ConfigsChanged(_)
        );

        if needs_window && !self.has_routes_for_target(target) {
            return;
        }

        match payload {
            UserEvent::DrawCommandBatch(batch) => {
                let EventTarget::Window(window_id) = target else {
                    log::warn!("DrawCommandBatch event missing window target");
                    return;
                };
                self.handle_draw_commands(window_id, batch);
            }
            UserEvent::WindowCommand(e) => {
                self.handle_window_command(target, e);
            }
            UserEvent::SettingsChanged(SettingsChanged::Window(e)) => {
                self.handle_window_settings_changed(e);
            }
            UserEvent::SettingsChanged(SettingsChanged::Renderer(e)) => {
                self.handle_render_settings_changed(e);
            }
            UserEvent::ConfigsChanged(config) => {
                self.handle_config_changed(*config);
            }
            #[cfg(target_os = "macos")]
            UserEvent::MacShortcut(command) => {
                self.handle_mac_shortcut(command);
            }
            UserEvent::ShowProgressBar { percent, .. } => {
                self.handle_progress_bar(percent);
            }
            _ => {}
        }
    }

    pub fn clear_renderer(&mut self, window_id: WindowId) {
        if let Some(route) = self.routes.get(&window_id) {
            route.window.renderer.borrow_mut().clear();
        }
    }

    #[cfg(target_os = "macos")]
    pub fn handle_mac_shortcut(&mut self, command: MacShortcutCommand) {
        match command {
            MacShortcutCommand::TogglePinnedWindow => self.toggle_pinned_window(),
            MacShortcutCommand::ShowEditorSwitcher => self.show_editor_switcher(),
        }
    }

    #[cfg(target_os = "macos")]
    fn record_window_usage(&mut self, window_id: WindowId) {
        self.window_mru.retain(|id| *id != window_id);
        self.window_mru.push_front(window_id);
    }

    #[cfg(target_os = "macos")]
    fn cleanup_window_mru(&mut self) {
        self.window_mru.retain(|id| self.routes.contains_key(id));
    }

    #[cfg(target_os = "macos")]
    fn handle_focus_gain_for_shortcuts(&mut self, window_id: WindowId) {
        if self.ignore_next_focus_gain {
            self.ignore_next_focus_gain = false;
        } else {
            self.record_window_usage(window_id);
        }
    }

    #[cfg(target_os = "macos")]
    fn pinned_candidate(&mut self) -> Option<WindowId> {
        self.cleanup_window_mru();
        if let Some(id) = self.window_mru.front().copied() {
            Some(id)
        } else {
            self.routes.keys().next().copied()
        }
    }

    #[cfg(target_os = "macos")]
    fn capture_focus_target(&mut self, pinned_id: WindowId) {
        let current = self.get_focused_route();
        if current != Some(pinned_id) {
            self.focus_return_target = current;
        } else {
            self.focus_return_target = None;
        }
    }

    #[cfg(target_os = "macos")]
    fn restore_focus_target(&mut self) -> bool {
        let Some(target_id) = self.focus_return_target.take() else {
            return false;
        };

        let Some(route) = self.routes.get(&target_id) else {
            return false;
        };

        if let Some(feature) = &route.window.macos_feature {
            feature.borrow().activate_application();
        }
        let window = route.window.winit_window.clone();
        window.focus_window();
        true
    }

    #[cfg(target_os = "macos")]
    fn toggle_pinned_window(&mut self) {
        let Some(window_id) = self.pinned_candidate() else {
            return;
        };

        let Some(route) = self.routes.get(&window_id) else {
            return;
        };

        let window = route.window.winit_window.clone();
        let is_active = {
            if let Some(feature) = &route.window.macos_feature {
                feature.borrow().is_key_window()
            } else {
                window.has_focus()
            }
        };

        self.ignore_next_focus_gain = true;

        if is_active {
            let uses_native_tabs = native_tab_bar_enabled()
                && self.settings.get::<CmdLineSettings>().macos_native_tabs;

            if uses_native_tabs {
                hide_application();
                return;
            }

            if !self.restore_focus_target() {
                hide_application();
            }
        } else {
            if let Some(feature) = &route.window.macos_feature {
                feature.borrow().activate_application();
            }
            #[cfg(target_os = "macos")]
            self.capture_focus_target(window_id);
            window.focus_window();
            self.record_window_usage(window_id);
        }
    }

    #[cfg(target_os = "macos")]
    fn show_editor_switcher(&mut self) {
        if is_tab_overview_active() {
            trigger_tab_overview();
            return;
        }
        let window_count = self.routes.len();
        if window_count == 0 {
            return;
        }
        if window_count == 1 {
            self.toggle_pinned_window();
            return;
        }

        #[cfg(target_os = "macos")]
        {
            let mut opened_overview = false;
            if let Some(window_id) = self.pinned_candidate() {
                if let Some(route) = self.routes.get(&window_id) {
                    if let Some(feature_rc) = &route.window.macos_feature {
                        {
                            let feature = feature_rc.borrow();
                            if feature.is_simple_fullscreen_enabled() {
                                drop(feature);
                                self.toggle_pinned_window();
                                return;
                            }
                        }
                        feature_rc.borrow().activate_application();
                        opened_overview = true;
                    }
                }
            }

            if opened_overview {
                trigger_tab_overview();
            } else {
                self.toggle_pinned_window();
            }
        }
    }

    pub fn draw_frame(&mut self, window_id: WindowId, dt: f32) {
        tracy_zone!("draw_frame");
        let Some(route) = self.routes.get_mut(&window_id) else {
            return;
        };
        let mut renderer = route.window.renderer.borrow_mut();
        let window = route.window.winit_window.clone();
        let mut skia_renderer = route.window.skia_renderer.borrow_mut();
        let Some(vsync) = route.state.vsync.as_mut() else {
            return;
        };

        renderer.draw_frame(skia_renderer.canvas(), dt);
        skia_renderer.flush();
        {
            tracy_gpu_zone!("wait for vsync");
            vsync.wait_for_vsync();
        }
        skia_renderer.swap_buffers();
        if self.ui_state == UIState::FirstFrame {
            window.set_visible(true);
            self.ui_state = UIState::Showing;
        }
        tracy_frame();
        tracy_gpu_collect();
    }

    pub fn refresh_rate_for_window(&self, window_id: WindowId, settings: &Settings) -> Option<f32> {
        let route = self.routes.get(&window_id)?;
        let vsync = route.state.vsync.as_ref()?;
        Some(vsync.get_refresh_rate(&route.window.winit_window, settings))
    }

    pub fn request_redraw_for_window(&mut self, window_id: WindowId) -> Option<bool> {
        let route = self.routes.get_mut(&window_id)?;
        let vsync = route.state.vsync.as_mut()?;
        if vsync.uses_winit_throttling() {
            vsync.request_redraw(&route.window.winit_window);
            Some(true)
        } else {
            Some(false)
        }
    }

    pub fn animate_frame(&mut self, window_id: WindowId, dt: f32) -> bool {
        tracy_zone!("animate_frame", 0);
        let route = match self.routes.get(&window_id) {
            Some(route) => route,
            None => return false,
        };
        let mut renderer = route.window.renderer.borrow_mut();

        let grid_scale = renderer.grid_renderer.grid_scale;

        let res = renderer.animate_frame(
            &self.get_grid_rect_from_window(window_id, grid_scale, GridSize::default()),
            dt,
        );
        tracy_plot!("animate_frame", res as u8 as f64);
        renderer.prepare_lines(false);
        #[allow(clippy::let_and_return)]
        res
    }

    // TODO: must be decided if the renderder should be a global state or not
    // Rc<> Reference counters are not thread safe, so we can't share the window or anything
    // else between threads.
    //
    // The renderer is a global state, so it should be shared between threads. (this must be
    // validated, but it's the current idea)
    // Instead, we can use std::sync::Arc, which stands for "atomically reference counted."
    // It’s identical to Rc, except it guarantees that modifications to the reference counter
    // are indivisible atomic operations, making it safe to use it with multiple threads.
    pub fn try_create_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        proxy: &EventLoopProxy<EventPayload>,
    ) {
        let creating_initial_window = self.routes.is_empty();
        if creating_initial_window && self.ui_state != UIState::WaitingForWindowCreate {
            return;
        }
        tracy_zone!("try_create_window");
        let persisted_window_settings = load_last_window_settings().ok();
        let mut desired_window_size =
            determine_window_size(persisted_window_settings.as_ref(), &self.settings.clone());
        let mut desired_grid_size =
            determine_grid_size(&desired_window_size, persisted_window_settings);

        #[cfg(target_os = "macos")]
        let mut host_window_position: Option<winit::dpi::PhysicalPosition<i32>> = None;

        if !self.routes.is_empty() {
            if let Some(host_id) = self.get_focused_route() {
                if let Some(host_route) = self.routes.get(&host_id) {
                    desired_window_size =
                        WindowSize::Size(host_route.window.last_applied_window_size);
                    desired_grid_size = host_route.window.last_synced_grid_size.or_else(|| {
                        let renderer = host_route.window.renderer.borrow();
                        Some(renderer.get_grid_size())
                    });
                    #[cfg(target_os = "macos")]
                    {
                        host_window_position = host_route.window.winit_window.outer_position().ok();
                    }
                }
            }
        }
        let theme = self.get_theme_for(None);
        let maximized = matches!(desired_window_size, WindowSize::Maximized);
        let window_config = create_window(event_loop, maximized, "Neovide", &self.settings, theme);
        let window = Rc::new(window_config.window.clone());

        let config = Config::init();
        let mut renderer = Renderer::new(1.0, config, self.settings.clone());

        let WindowSettings {
            input_ime,
            opacity,
            normal_opacity,
            window_blurred,
            fullscreen,
            #[cfg(target_os = "macos")]
            input_macos_option_key_is_meta,
            #[cfg(target_os = "macos")]
            macos_simple_fullscreen,

            #[cfg(target_os = "windows")]
            title_background_color,
            #[cfg(target_os = "windows")]
            title_text_color,
            ..
        } = self.settings.get::<WindowSettings>();

        window.set_ime_allowed(input_ime);

        let scale_factor = window.scale_factor();
        renderer.handle_os_scale_factor_change(scale_factor);

        let mut pending_initial_window_size = None;
        let mut initial_pixel_size: Option<PhysicalSize<u32>> = None;
        match desired_window_size.clone() {
            WindowSize::Maximized => {}
            WindowSize::Grid(_) | WindowSize::NeovimGrid => {
                pending_initial_window_size = Some(desired_window_size.clone());
            }
            WindowSize::Size(window_size) => {
                initial_pixel_size = Some(window_size);
            }
        };
        if !maximized {
            if let Some(size) = initial_pixel_size {
                tracy_zone!("request_inner_size");
                let _ = window.request_inner_size(size);
            }
        }

        #[cfg(target_os = "macos")]
        if let Some(position) = host_window_position {
            window.set_outer_position(position);
        }

        // Check that window is visible in some monitor, and reposition it if not.
        if let Ok(previous_position) = window.outer_position() {
            if let Some(current_monitor) = window.current_monitor() {
                let monitor_position = current_monitor.position();
                let monitor_size = current_monitor.size();
                let monitor_width = monitor_size.width as i32;
                let monitor_height = monitor_size.height as i32;

                let window_position = previous_position;

                let window_size = window.outer_size();
                let window_width = window_size.width as i32;
                let window_height = window_size.height as i32;

                if window_position.x + window_width < monitor_position.x
                    || window_position.y + window_height < monitor_position.y
                    || window_position.x > monitor_position.x + monitor_width
                    || window_position.y > monitor_position.y + monitor_height
                {
                    window.set_outer_position(monitor_position);
                };
            };
        }
        let logged_size = initial_pixel_size.unwrap_or_default();
        log::info!("Showing window size: {logged_size:#?}, maximized: {maximized}");
        let is_wayland = match window.window_handle() {
            Ok(handle) => matches!(handle.as_raw(), RawWindowHandle::Wayland(_)),
            Err(err) => {
                log::warn!("Failed to read window handle: {err}");
                false
            }
        };
        // On Wayland we can show the window now, since internally it's only shown after the first rendering
        // On the other platforms the window is shown after rendering to avoid flickering
        if is_wayland {
            window.set_visible(true);
        }

        let cmd_line_settings = self.settings.get::<CmdLineSettings>();
        let srgb = cmd_line_settings.srgb;
        let vsync_enabled = cmd_line_settings.vsync;
        let skia_renderer: Rc<RefCell<Box<dyn SkiaRenderer>>> = Rc::new(RefCell::new(
            create_skia_renderer(&window_config, srgb, vsync_enabled, self.settings.clone()),
        ));

        // Create a separate binding for the mutable borrow
        let window = skia_renderer.borrow_mut().window();

        #[cfg(target_os = "windows")]
        {
            if let Some(winit_color) = Self::parse_winit_color(&title_background_color) {
                window.set_title_background_color(Some(winit_color));
            }

            if let Some(winit_color) = Self::parse_winit_color(&title_text_color) {
                window.set_title_text_color(winit_color);
            }
        }

        let saved_inner_size = window.inner_size();

        log::info!(
            "window created (scale_factor: {:.4}, font_dimensions: {:?})",
            scale_factor,
            renderer.grid_renderer.grid_scale
        );

        window.set_blur(window_blurred && opacity.min(normal_opacity) < 1.0);

        #[cfg(target_os = "windows")]
        if window_blurred {
            window.set_system_backdrop(BackdropType::TransientWindow); // Acrylic blur
        }

        if fullscreen {
            let handle = window.current_monitor();
            window.set_fullscreen(Some(Fullscreen::Borderless(handle)));
        }

        #[cfg(target_os = "windows")]
        {
            if let Some(winit_color) = Self::parse_winit_color(&title_background_color) {
                window.set_title_background_color(Some(winit_color));
            }

            if let Some(winit_color) = Self::parse_winit_color(&title_text_color) {
                window.set_title_text_color(winit_color);
            }
        }

        let skia_renderer_ref: &dyn SkiaRenderer = &**skia_renderer.borrow();
        let vsync = VSync::new(
            vsync_enabled,
            skia_renderer_ref,
            proxy.clone(),
            self.settings.clone(),
        );

        {
            tracy_zone!("request_redraw");
            window.request_redraw();
        }

        let colorscheme_stream = self.take_colorscheme_stream();
        let runtime = self
            .runtime
            .as_mut()
            .expect("Neovim runtime has not been initialized");
        let neovim_handler = runtime
            .launch(
                window.id(),
                proxy.clone(),
                desired_grid_size,
                self.runtime_tracker.clone(),
                self.settings.clone(),
                colorscheme_stream,
            )
            .expect("Failed to launch neovim runtime");

        // It's important that this is created before the window is resized, since it can change the padding and affect the size
        #[cfg(target_os = "macos")]
        let macos_feature = {
            let feature = MacosWindowFeature::from_winit_window(
                &window,
                self.settings.clone(),
                proxy.clone(),
                neovim_handler.clone(),
            );
            feature.activate_and_focus();
            feature
        };

        if creating_initial_window {
            self.ui_state = UIState::FirstFrame;
        } else {
            window.set_visible(true);
        }

        let mouse_manager = MouseManager::new(self.settings.clone());
        let mut state = RouteState::new();
        state.saved_inner_size = saved_inner_size;
        state.vsync = Some(vsync);
        let route = Route {
            window: RouteWindow {
                renderer: Rc::new(RefCell::new(Box::new(renderer))),
                skia_renderer: skia_renderer.clone(),
                winit_window: window.clone(),
                neovim_handler,
                mouse_manager: Rc::new(RefCell::new(Box::new(mouse_manager))),
                #[cfg(target_os = "macos")]
                macos_feature: Some(Rc::new(RefCell::new(Box::new(macos_feature)))),
                title: String::from("Neovide"),
                last_applied_window_size: saved_inner_size,
                last_synced_grid_size: None,
            },
            pending_initial_window_size,
            state,
        };
        self.routes.insert(window.id(), route);
        #[cfg(target_os = "macos")]
        self.record_window_usage(window.id());
        #[cfg(target_os = "macos")]
        self.set_macos_option_as_meta(input_macos_option_key_is_meta);
        #[cfg(target_os = "macos")]
        self.set_simple_fullscreen(macos_simple_fullscreen);
    }

    pub fn handle_draw_commands(&mut self, window_id: WindowId, batch: Vec<DrawCommand>) {
        tracy_zone!("handle_draw_commands");
        let Some(route) = self.routes.get(&window_id) else {
            return;
        };
        let handle_draw_commands_result = {
            let mut renderer = route.window.renderer.borrow_mut();
            renderer.handle_draw_commands(batch)
        };

        if let Some(route) = self.routes.get_mut(&window_id) {
            route.state.font_changed_last_frame |= handle_draw_commands_result.font_changed;
        }

        if handle_draw_commands_result.should_show {
            self.apply_pending_initial_window_size(window_id);
        }
    }

    fn apply_pending_initial_window_size(&mut self, window_id: WindowId) {
        let pending = match self.routes.get(&window_id) {
            Some(route) => match &route.pending_initial_window_size {
                Some(value) => value.clone(),
                None => return,
            },
            None => return,
        };

        let window = match self.routes.get(&window_id) {
            Some(route) => route.window.winit_window.clone(),
            None => return,
        };

        match pending {
            WindowSize::Grid(grid_size) => {
                let window_size = self.get_window_size_from_grid(window_id, &grid_size);
                let _ = window
                    .request_inner_size(PhysicalSize::new(window_size.width, window_size.height));
            }
            WindowSize::NeovimGrid => {
                let grid_size = match self.routes.get(&window_id) {
                    Some(route) => {
                        let renderer = route.window.renderer.borrow();
                        renderer.get_grid_size()
                    }
                    None => return,
                };
                let window_size = self.get_window_size_from_grid(window_id, &grid_size);
                let _ = window
                    .request_inner_size(PhysicalSize::new(window_size.width, window_size.height));
            }
            WindowSize::Size(size) => {
                let _ = window.request_inner_size(size);
            }
            WindowSize::Maximized => {
                window.set_maximized(true);
            }
        }

        if let Some(route) = self.routes.get_mut(&window_id) {
            route.pending_initial_window_size = None;
        }
    }

    pub fn queue_restart(&mut self, window_id: WindowId, details: RestartDetails) {
        let grid_size = match self.routes.get(&window_id) {
            Some(route) => route.window.renderer.borrow().get_grid_size(),
            None => return,
        };

        self.pending_restart
            .insert(window_id, RestartRequest { details, grid_size });
        self.clear_renderer(window_id);
        if let Some(route) = self.routes.get_mut(&window_id) {
            route.window.last_synced_grid_size = None;
        }
    }

    fn restart_neovim(
        &mut self,
        window_id: WindowId,
        restart: RestartRequest,
        proxy: &EventLoopProxy<EventPayload>,
    ) -> Result<(), ()> {
        let runtime = self.runtime.as_mut().ok_or(())?;
        let handler = self
            .routes
            .get(&window_id)
            .map(|route| route.window.neovim_handler.clone())
            .ok_or(())?;

        runtime
            .restart(
                window_id,
                proxy.clone(),
                handler,
                restart.grid_size,
                self.settings.clone(),
                restart.details,
            )
            .map_err(|error| {
                log::error!("Failed to restart Neovim: {error:?}");
            })
    }

    pub fn handle_neovim_exit(
        &mut self,
        window_id: WindowId,
        proxy: &EventLoopProxy<EventPayload>,
    ) {
        if let Some(restart) = self.pending_restart.remove(&window_id) {
            if self.restart_neovim(window_id, restart, proxy).is_ok() {
                return;
            }
        }
        if let Some(route) = self.routes.remove(&window_id) {
            let window = route.window.winit_window.clone();
            window.set_visible(false);

            #[cfg(target_os = "macos")]
            {
                self.window_mru.retain(|id| *id != window_id);
                if self.focus_return_target == Some(window_id) {
                    self.focus_return_target = None;
                }
            }

            drop(route);
        }

        if self.routes.is_empty() {
            self.ui_state = UIState::Initing;
        }
    }

    fn handle_config_changed(&mut self, config: HotReloadConfigs) {
        tracy_zone!("handle_config_changed");
        let Some(route) = self.focused_route_mut() else {
            return;
        };
        let mut renderer = route.window.renderer.borrow_mut();
        renderer.handle_config_changed(config);
        route.state.font_changed_last_frame = true;
    }

    fn handle_progress_bar(&mut self, percent: f32) {
        tracy_zone!("handle_progress_bar");
        let Some(route) = self.focused_route() else {
            return;
        };
        let mut renderer = route.window.renderer.borrow_mut();
        renderer.progress_bar.start(percent);
    }

    #[cfg_attr(not(target_os = "macos"), allow(unused_variables))]
    fn calculate_window_padding(&self, window_id: WindowId) -> WindowPadding {
        let window_settings = self.settings.get::<WindowSettings>();

        #[cfg(not(target_os = "macos"))]
        let window_padding_top = window_settings.padding_top;

        #[cfg(target_os = "macos")]
        let window_padding_top = {
            let mut padding_top = window_settings.padding_top;
            if let Some(route) = self.routes.get(&window_id) {
                if let Some(macos_feature) = &route.window.macos_feature {
                    padding_top += macos_feature.borrow().extra_titlebar_height_in_pixels();
                }
            }
            padding_top
        };

        WindowPadding {
            top: window_padding_top,
            left: window_settings.padding_left,
            right: window_settings.padding_right,
            bottom: window_settings.padding_bottom,
        }
    }

    #[cfg(target_os = "macos")]
    pub fn grid_to_pixel_position(
        &mut self,
        window_id: WindowId,
        col: i64,
        row: i64,
    ) -> Point2<Pixel<f32>> {
        let grid_position = GridPos::new(col, row);
        let mut renderer = {
            let route = self.routes.get(&window_id).expect("window must exist");
            route.window.renderer.borrow_mut()
        };
        let root_region_offset = renderer
            .window_regions
            .first()
            .map(|region| region.region.min);

        // Align the lookup point with the actual font baseline instead of a heuristic offset.
        let grid_scale = renderer.grid_renderer.grid_scale;
        let baseline_offset = renderer.grid_renderer.shaper.baseline_offset();
        drop(renderer);

        let mut position = grid_position * grid_scale;

        if let Some(offset) = root_region_offset {
            position.x += offset.x;
            position.y += offset.y;
        }

        position.y += baseline_offset;

        position
    }

    #[cfg(target_os = "macos")]
    pub fn apply_padding_to_position(
        &self,
        position: Point2<Pixel<f32>>,
        padding: WindowPadding,
        titlebar_height: f32,
    ) -> Point2<Pixel<f32>> {
        let _ = (padding, titlebar_height);
        position
    }

    pub fn get_focused_route(&self) -> Option<WindowId> {
        if let Some(id) = self.routes.iter().find_map(|(key, val)| {
            if (!val.window.winit_window.has_focus() && self.routes.len() == 1)
                || val.window.winit_window.has_focus()
            {
                Some(*key)
            } else {
                None
            }
        }) {
            return Some(id);
        }

        #[cfg(target_os = "macos")]
        {
            if let Some(id) = self
                .window_mru
                .iter()
                .copied()
                .find(|candidate| self.routes.contains_key(candidate))
            {
                return Some(id);
            }
        }

        self.routes.keys().next().copied()
    }

    fn resolve_target_window_id(&self, target: EventTarget) -> Option<WindowId> {
        match target {
            EventTarget::Focused | EventTarget::All => self.get_focused_route(),
            EventTarget::Window(window_id) => {
                self.routes.contains_key(&window_id).then_some(window_id)
            }
        }
    }

    fn focused_route(&self) -> Option<&Route> {
        self.resolve_target_window_id(EventTarget::Focused)
            .and_then(|id| self.routes.get(&id))
    }

    fn focused_route_mut(&mut self) -> Option<&mut Route> {
        let id = self.resolve_target_window_id(EventTarget::Focused)?;
        self.routes.get_mut(&id)
    }

    fn has_routes_for_target(&self, target: EventTarget) -> bool {
        self.resolve_target_window_id(target).is_some()
    }

    pub fn prepare_frame(&mut self, window_id: WindowId) -> ShouldRender {
        tracy_zone!("prepare_frame", 0);
        if !self.routes.contains_key(&window_id) {
            return ShouldRender::Wait;
        }

        let mut should_render = ShouldRender::Wait;

        let window_padding = self.calculate_window_padding(window_id);
        let padding_changed = self
            .routes
            .get(&window_id)
            .map(|route| route.state.window_padding != window_padding)
            .unwrap_or(false);

        // Don't render until the UI is fully entered and the window is shown
        if self.ui_state < UIState::FirstFrame {
            return ShouldRender::Wait;
        } else if self.ui_state == UIState::FirstFrame {
            should_render = ShouldRender::Immediately;
        }

        let is_minimized = self
            .routes
            .get(&window_id)
            .map(|route| route.window.winit_window.is_minimized() == Some(true))
            .unwrap_or(false);

        let resize_requested = self
            .routes
            .get(&window_id)
            .map(|route| {
                route.state.requested_columns.is_some() || route.state.requested_lines.is_some()
            })
            .unwrap_or(false);
        if resize_requested {
            // Resize requests (columns/lines) have priority over normal window sizing.
            // So, deal with them first and resize the window programmatically.
            // The new window size will then be processed in the following frame.
            self.update_window_size_from_grid(window_id);
        } else if !is_minimized {
            // NOTE: Only actually resize the grid when the window is not minimized
            // Some platforms return a zero size when that is the case, so we should not try to resize to that.
            let new_window_size = match self.routes.get(&window_id) {
                Some(route) => route.window.winit_window.inner_size(),
                None => return should_render,
            };

            let mut needs_window_update = false;
            if let Some(route) = self.routes.get(&window_id) {
                if route.state.saved_inner_size != new_window_size
                    || route.state.font_changed_last_frame
                    || padding_changed
                    || route.window.last_applied_window_size != route.state.saved_inner_size
                {
                    needs_window_update = true;
                }
            }

            if needs_window_update {
                if let Some(route) = self.routes.get_mut(&window_id) {
                    route.state.window_padding = window_padding;
                    route.state.saved_inner_size = new_window_size;
                }
                self.update_grid_size_from_window(window_id);
                if let Some(route) = self.routes.get_mut(&window_id) {
                    route.window.last_applied_window_size = route.state.saved_inner_size;
                }
                should_render = ShouldRender::Immediately;
            }
        }

        self.update_ime_position(window_id, false);

        if let Some(route) = self.routes.get(&window_id) {
            let mut renderer = route.window.renderer.borrow_mut();
            should_render.update(renderer.prepare_frame());
        }

        if let Some(route) = self.routes.get_mut(&window_id) {
            if route.state.font_changed_last_frame {
                let mut renderer = route.window.renderer.borrow_mut();
                renderer.prepare_lines(true);
                route.state.font_changed_last_frame = false;
            }
        }

        should_render
    }

    pub fn get_grid_size(&self) -> GridSize<u32> {
        let Some(route) = self.focused_route() else {
            return DEFAULT_GRID_SIZE;
        };
        let renderer = route.window.renderer.borrow();
        renderer.get_grid_size()
    }

    fn get_window_size_from_grid(
        &self,
        window_id: WindowId,
        grid_size: &GridSize<u32>,
    ) -> PixelSize<u32> {
        let window_padding = self.calculate_window_padding(window_id);

        let window_padding_size = PixelSize::new(
            window_padding.left + window_padding.right,
            window_padding.top + window_padding.bottom,
        );
        let round_or_ceil = |v: PixelSize<f32>| -> PixelSize<f32> {
            PixelSize::new(
                round_or_op(v.width, f32::ceil),
                round_or_op(v.height, f32::ceil),
            )
        };

        let Some(route) = self.routes.get(&window_id) else {
            return PixelSize::new(0, 0);
        };
        let renderer = route.window.renderer.borrow();
        let window_size = round_or_ceil(*grid_size * renderer.grid_renderer.grid_scale)
            .try_cast()
            .unwrap_or_default()
            + window_padding_size;

        log::info!(
            "get_window_size_from_grid: Grid Size: {grid_size:?}, Window Size {window_size:?}"
        );
        window_size
    }

    fn update_window_size_from_grid(&mut self, window_id: WindowId) {
        let grid_size = {
            let Some(route) = self.routes.get_mut(&window_id) else {
                return;
            };
            clamped_grid_size(&GridSize::new(
                route.state.requested_columns.take().unwrap_or(
                    route
                        .state
                        .saved_grid_size
                        .map_or(DEFAULT_GRID_SIZE.width, |v| v.width),
                ),
                route.state.requested_lines.take().unwrap_or(
                    route
                        .state
                        .saved_grid_size
                        .map_or(DEFAULT_GRID_SIZE.height, |v| v.height),
                ),
            ))
        };
        let new_size = self.get_window_size_from_grid(window_id, &grid_size);
        let window = match self.routes.get(&window_id) {
            Some(route) => route.window.winit_window.clone(),
            None => return,
        };

        let new_size = winit::dpi::PhysicalSize {
            width: new_size.width,
            height: new_size.height,
        };
        let _ = window.request_inner_size(new_size);

        if let Some(route) = self.routes.get(&window_id) {
            let mut skia_renderer = route.window.skia_renderer.borrow_mut();
            skia_renderer.resize();
        }
    }

    fn get_grid_size_from_window(
        &self,
        window_id: WindowId,
        grid_scale: GridScale,
        min: GridSize<u32>,
    ) -> GridSize<u32> {
        let route = match self.routes.get(&window_id) {
            Some(route) => route,
            None => return min,
        };
        let window_padding = route.state.window_padding;
        let window_padding_size: PixelSize<u32> = PixelSize::new(
            window_padding.left + window_padding.right,
            window_padding.top + window_padding.bottom,
        );

        let content_size = PixelSize::new(
            route.state.saved_inner_size.width,
            route.state.saved_inner_size.height,
        ) - window_padding_size;

        let round_or_floor = |v: GridSize<f32>| -> GridSize<f32> {
            GridSize::new(
                round_or_op(v.width, f32::floor),
                round_or_op(v.height, f32::floor),
            )
        };

        let grid_size = round_or_floor(content_size / grid_scale)
            .try_cast()
            .unwrap_or(min);

        grid_size.max(min)
    }

    fn get_grid_rect_from_window(
        &self,
        window_id: WindowId,
        grid_scale: GridScale,
        min: GridSize<u32>,
    ) -> GridRect<f32> {
        let size = self
            .get_grid_size_from_window(window_id, grid_scale, min)
            .try_cast()
            .unwrap_or_default();
        let pos = self
            .routes
            .get(&window_id)
            .map(|route| {
                PixelPos::new(
                    route.state.window_padding.left,
                    route.state.window_padding.top,
                )
                .cast()
                    / grid_scale
            })
            .unwrap_or_else(|| PixelPos::new(0, 0).cast() / grid_scale);
        GridRect::<f32>::from_origin_and_size(pos, size)
    }

    fn update_grid_size_from_window(&mut self, window_id: WindowId) {
        let (grid_scale, neovim_handler, last_synced, saved_inner_size) =
            match self.routes.get(&window_id) {
                Some(route) => {
                    let renderer = route.window.renderer.borrow();
                    (
                        renderer.grid_renderer.grid_scale,
                        route.window.neovim_handler.clone(),
                        route.window.last_synced_grid_size,
                        route.state.saved_inner_size,
                    )
                }
                None => return,
            };
        let grid_size = self.get_grid_size_from_window(window_id, grid_scale, MIN_GRID_SIZE);
        if last_synced.as_ref() == Some(&grid_size) {
            trace!("Grid matched route size, skip update.");
            return;
        }
        if let Some(route) = self.routes.get_mut(&window_id) {
            route.state.saved_grid_size = Some(grid_size);
        }
        log::info!(
            "Resizing grid based on window size. Grid Size: {:?}, Window Size {:?}",
            grid_size,
            saved_inner_size
        );
        send_ui(
            ParallelCommand::Resize {
                width: grid_size.width.into(),
                height: grid_size.height.into(),
            },
            &neovim_handler,
        );

        if let Some(route_mut) = self.routes.get_mut(&window_id) {
            route_mut.window.last_synced_grid_size = Some(grid_size);
        }
    }

    fn update_ime_position(&mut self, window_id: WindowId, force: bool) {
        let (window, grid_scale, position, current_area) = {
            let route = match self.routes.get(&window_id) {
                Some(route) => route,
                None => return,
            };
            if !route.state.ime_enabled {
                return;
            }
            let window = route.window.winit_window.clone();
            let renderer = route.window.renderer.borrow();
            let grid_scale = renderer.grid_renderer.grid_scale;
            let position = renderer.get_cursor_destination();
            let position = match position.try_cast::<u32>() {
                Some(position) => position,
                None => return,
            };
            let position = dpi::PhysicalPosition {
                x: position.x,
                y: position.y,
            };
            (window, grid_scale, position, route.state.ime_area)
        };
        let font_dimensions = GridSize::new(1.0, 1.0) * grid_scale;
        // NOTE: some compositors don't like excluding too much and try to render popup at the
        // bottom right corner of the provided area, so exclude just the full-width char to not
        // obscure the cursor and not render popup at the end of the window.
        let width = (font_dimensions.width * 2.0).ceil() as u32;
        let height = font_dimensions.height.ceil() as u32;
        let size = dpi::PhysicalSize::new(width, height);
        let area = (position, size);
        if force || current_area != area {
            if let Some(route) = self.routes.get_mut(&window_id) {
                route.state.ime_area = (position, size);
            }
            window.set_ime_cursor_area(position, size);
        }
    }

    fn handle_scale_factor_update(&mut self, window_id: WindowId, scale_factor: f64) {
        let Some(route) = self.routes.get(&window_id) else {
            return;
        };
        let mut renderer = route.window.renderer.borrow_mut();
        let mut skia_renderer = route.window.skia_renderer.borrow_mut();
        #[cfg(target_os = "macos")]
        {
            if let Some(macos_feature) = &route.window.macos_feature {
                macos_feature
                    .borrow_mut()
                    .handle_scale_factor_update(scale_factor);
            }
        }
        renderer.handle_os_scale_factor_change(scale_factor);
        skia_renderer.resize();
    }

    #[cfg(windows)]
    fn parse_winit_color(color: &str) -> Option<Color> {
        match csscolorparser::parse(color) {
            Ok(color) => {
                let color = color.to_rgba8();
                Some(Color::from_rgb(color[0], color[1], color[2]))
            }
            _ => None,
        }
    }

    #[cfg(windows)]
    fn handle_title_background_color(&self, color: &str) {
        let Some(route) = self.focused_route() else {
            return;
        };

        let skia_renderer = route.window.skia_renderer.borrow();
        let winit_color = Self::parse_winit_color(color);
        skia_renderer
            .window()
            .set_title_background_color(winit_color);
    }

    #[cfg(windows)]
    fn handle_title_text_color(&self, color: &str) {
        let Some(route) = self.focused_route() else {
            return;
        };

        let skia_renderer = route.window.skia_renderer.borrow();
        if let Some(winit_color) = Self::parse_winit_color(color) {
            skia_renderer.window().set_title_text_color(winit_color);
        }
    }
}
