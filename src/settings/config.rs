//! Config file handling

use std::{env, fs, sync::mpsc, time::Duration};

use notify_debouncer_full::{new_debouncer, notify::RecursiveMode};
use serde::{
    Deserialize, Deserializer,
    de::{Error as DeError, Unexpected},
};
use winit::event_loop::EventLoopProxy;

use crate::{
    cmd_line::{GeometryArgs, MouseCursorIcon},
    error_msg,
    frame::Frame,
    renderer::box_drawing::BoxDrawingSettings,
    window::{EventPayload, UserEvent},
};

use std::path::{Path, PathBuf};

use super::font::FontSettings;

const CONFIG_FILE: &str = "config.toml";

#[cfg(unix)]
fn neovide_config_dir() -> PathBuf {
    let xdg_dirs = xdg::BaseDirectories::with_prefix("neovide");
    xdg_dirs.get_config_home().unwrap()
}

#[cfg(windows)]
fn neovide_config_dir() -> PathBuf {
    let mut path = dirs::config_dir().unwrap();
    path.push("neovide");
    path
}

pub fn config_path() -> PathBuf {
    env::var("NEOVIDE_CONFIG")
        .ok()
        .map(PathBuf::from)
        .filter(|path| path.exists() && path.is_file())
        .unwrap_or_else(|| {
            let mut path = neovide_config_dir();
            path.push(CONFIG_FILE);
            path
        })
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum HotkeyConfigValue {
    String(String),
    Bool(bool),
}

fn deserialize_optional_hotkey<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<HotkeyConfigValue>::deserialize(deserializer)? {
        None => Ok(None),
        Some(HotkeyConfigValue::String(value)) => Ok(Some(value)),
        Some(HotkeyConfigValue::Bool(false)) => Ok(Some("false".to_string())),
        Some(HotkeyConfigValue::Bool(true)) => {
            Err(D::Error::invalid_value(Unexpected::Bool(true), &"a shortcut string or false"))
        }
    }
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    pub font: Option<FontSettings>,
    pub box_drawing: Option<BoxDrawingSettings>,
    pub server: Option<String>,
    pub fork: Option<bool>,
    pub frame: Option<Frame>,
    pub size: Option<String>,
    pub grid: Option<String>,
    pub idle: Option<bool>,
    pub maximized: Option<bool>,
    pub neovim_bin: Option<StringOrArray>,
    pub no_multigrid: Option<bool>,
    pub srgb: Option<bool>,
    pub tabs: Option<bool>,
    pub system_native_tabs: Option<bool>,
    pub mouse_cursor_icon: Option<String>,
    pub title_hidden: Option<bool>,
    pub vsync: Option<bool>,
    pub wsl: Option<bool>,
    pub backtraces_path: Option<PathBuf>,
    #[serde(default, deserialize_with = "deserialize_optional_hotkey")]
    pub system_pinned_hotkey: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_hotkey")]
    pub system_switcher_hotkey: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_hotkey")]
    pub system_new_window_hotkey: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_hotkey")]
    pub system_hide_hotkey: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_hotkey")]
    pub system_hide_others_hotkey: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_hotkey")]
    pub system_quit_hotkey: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_hotkey")]
    pub system_minimize_hotkey: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_hotkey")]
    pub system_fullscreen_hotkey: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_hotkey")]
    pub system_show_all_tabs_hotkey: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_hotkey")]
    pub system_tab_prev_hotkey: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_hotkey")]
    pub system_tab_next_hotkey: Option<String>,
    pub icon: Option<String>,
    pub chdir: Option<PathBuf>,
    pub opengl: Option<bool>,
    pub wayland_app_id: Option<String>,
    pub x11_wm_class: Option<String>,
    pub x11_wm_class_instance: Option<String>,
}

/// Accepts an array of strings, or a scalar string (equivalent to a single-element array).
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum StringOrArray {
    Single(String),
    Array(Vec<String>),
}

impl From<StringOrArray> for Vec<String> {
    fn from(value: StringOrArray) -> Self {
        match value {
            StringOrArray::Single(value) => vec![value],
            StringOrArray::Array(value) => value,
        }
    }
}

impl From<String> for StringOrArray {
    fn from(value: String) -> Self {
        StringOrArray::Single(value)
    }
}

impl From<Vec<String>> for StringOrArray {
    fn from(value: Vec<String>) -> Self {
        StringOrArray::Array(value)
    }
}

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum HotReloadConfigs {
    App(AppHotReloadConfigs),
    Renderer(RendererHotReloadConfigs),
    Window(WindowHotReloadConfigs),
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppHotReloadConfigs {
    Idle(bool),
}

#[derive(Debug, Clone, PartialEq)]
pub enum RendererHotReloadConfigs {
    Font(Box<Option<FontSettings>>),
    BoxDrawing(Option<BoxDrawingSettings>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum WindowHotReloadConfigs {
    TitleHidden(Option<bool>),
    MouseCursorIcon(MouseCursorIcon),
    Geometry(GeometryArgs),
}

impl Config {
    /// Loads config from `config_path()` and writes it to env variables.
    pub fn init() -> Config {
        let config = Config::load_from_path(&config_path());
        match &config {
            Ok(config) => config.write_to_env(),
            Err(Some(err)) => eprintln!("{err}"),
            Err(None) => {}
        };
        config.unwrap_or_default()
    }

    pub fn watch_config_file(init_config: Config, event_loop_proxy: EventLoopProxy<EventPayload>) {
        std::thread::spawn(move || watcher_thread(init_config, event_loop_proxy));
    }

    /// Modifies the local environment to match the values in the config file.
    ///
    /// The values set in the config file override the values in the execution environment. This
    /// method is responsible for that behavior.
    fn write_to_env(&self) {
        if let Some(server) = &self.server {
            unsafe { env::set_var("NEOVIDE_SERVER", server) };
        }
        if let Some(wsl) = self.wsl {
            unsafe { env::set_var("NEOVIDE_WSL", wsl.to_string()) };
        }
        if let Some(no_multigrid) = self.no_multigrid {
            unsafe { env::set_var("NEOVIDE_NO_MULTIGRID", no_multigrid.to_string()) };
        }
        if let Some(maximized) = self.maximized {
            unsafe { env::set_var("NEOVIDE_MAXIMIZED", maximized.to_string()) };
        }
        if let Some(vsync) = self.vsync {
            unsafe { env::set_var("NEOVIDE_VSYNC", vsync.to_string()) };
        }
        if let Some(srgb) = self.srgb {
            unsafe { env::set_var("NEOVIDE_SRGB", srgb.to_string()) };
        }
        if let Some(fork) = self.fork {
            unsafe { env::set_var("NEOVIDE_FORK", fork.to_string()) };
        }
        if let Some(opengl) = self.opengl {
            unsafe { env::set_var("NEOVIDE_OPENGL", opengl.to_string()) };
        }
        if let Some(idle) = self.idle {
            unsafe { env::set_var("NEOVIDE_IDLE", idle.to_string()) };
        }
        if let Some(frame) = self.frame {
            unsafe { env::set_var("NEOVIDE_FRAME", frame.to_string()) };
        }
        if let Some(size) = &self.size {
            unsafe { env::set_var("NEOVIDE_SIZE", size) };
        }
        if let Some(grid) = &self.grid {
            unsafe { env::set_var("NEOVIDE_GRID", grid) };
        }
        if self.neovim_bin.is_some() {
            // We can't just set NEOVIM_BIN to a string, because it needs to be treated as an array.
            // Instead, we just clear any NEOVIM_BIN environment variable if there is a value set in
            // the config file, and then handle it separately in handle_command_line_arguments.
            unsafe { env::remove_var("NEOVIM_BIN") };
        }
        if let Some(mouse_cursor_icon) = &self.mouse_cursor_icon {
            unsafe { env::set_var("NEOVIDE_MOUSE_CURSOR_ICON", mouse_cursor_icon) };
        }
        if let Some(title_hidden) = &self.title_hidden {
            unsafe { env::set_var("NEOVIDE_TITLE_HIDDEN", title_hidden.to_string()) };
        }
        if let Some(tabs) = &self.tabs {
            unsafe { env::set_var("NEOVIDE_TABS", tabs.to_string()) };
        }
        if let Some(system_native_tabs) = &self.system_native_tabs {
            unsafe { env::set_var("NEOVIDE_SYSTEM_NATIVE_TABS", system_native_tabs.to_string()) };
        }
        if let Some(pinned_hotkey) = &self.system_pinned_hotkey {
            unsafe { env::set_var("NEOVIDE_SYSTEM_PINNED_HOTKEY", pinned_hotkey) };
        }
        if let Some(switcher_hotkey) = &self.system_switcher_hotkey {
            unsafe { env::set_var("NEOVIDE_SYSTEM_SWITCHER_HOTKEY", switcher_hotkey) };
        }
        if let Some(new_window_hotkey) = &self.system_new_window_hotkey {
            unsafe { env::set_var("NEOVIDE_SYSTEM_NEW_WINDOW_HOTKEY", new_window_hotkey) };
        }
        if let Some(hide_hotkey) = &self.system_hide_hotkey {
            unsafe { env::set_var("NEOVIDE_SYSTEM_HIDE_HOTKEY", hide_hotkey) };
        }
        if let Some(hide_others_hotkey) = &self.system_hide_others_hotkey {
            unsafe { env::set_var("NEOVIDE_SYSTEM_HIDE_OTHERS_HOTKEY", hide_others_hotkey) };
        }
        if let Some(quit_hotkey) = &self.system_quit_hotkey {
            unsafe { env::set_var("NEOVIDE_SYSTEM_QUIT_HOTKEY", quit_hotkey) };
        }
        if let Some(minimize_hotkey) = &self.system_minimize_hotkey {
            unsafe { env::set_var("NEOVIDE_SYSTEM_MINIMIZE_HOTKEY", minimize_hotkey) };
        }
        if let Some(fullscreen_hotkey) = &self.system_fullscreen_hotkey {
            unsafe { env::set_var("NEOVIDE_SYSTEM_FULLSCREEN_HOTKEY", fullscreen_hotkey) };
        }
        if let Some(show_all_tabs_hotkey) = &self.system_show_all_tabs_hotkey {
            unsafe { env::set_var("NEOVIDE_SYSTEM_SHOW_ALL_TABS_HOTKEY", show_all_tabs_hotkey) };
        }
        if let Some(tab_prev_hotkey) = &self.system_tab_prev_hotkey {
            unsafe { env::set_var("NEOVIDE_SYSTEM_TAB_PREV_HOTKEY", tab_prev_hotkey) };
        }
        if let Some(tab_next_hotkey) = &self.system_tab_next_hotkey {
            unsafe { env::set_var("NEOVIDE_SYSTEM_TAB_NEXT_HOTKEY", tab_next_hotkey) };
        }
        if let Some(icon) = &self.icon {
            unsafe { env::set_var("NEOVIDE_ICON", icon) };
        }
        if let Some(wayland_app_id) = &self.wayland_app_id {
            unsafe { env::set_var("NEOVIDE_APP_ID", wayland_app_id) };
        }
        if let Some(x11_wm_class) = &self.x11_wm_class {
            unsafe { env::set_var("NEOVIDE_WM_CLASS", x11_wm_class) };
        }
        if let Some(x11_wm_class_instance) = &self.x11_wm_class_instance {
            unsafe { env::set_var("NEOVIDE_WM_CLASS_INSTANCE", x11_wm_class_instance) };
        }
        if let Some(chdir) = &self.chdir {
            unsafe { env::set_var("NEOVIDE_CHDIR", chdir.to_string_lossy().to_string()) };
        }
    }

    // TODO: should maybe return well-typed error?
    fn load_from_path(path: &Path) -> Result<Self, Option<String>> {
        if !path.exists() {
            return Err(None);
        }
        let toml = fs::read_to_string(path).map_err(|e| {
            format!(
                "Error while trying to open config file {}:\n{}\nContinuing with default config.",
                path.to_string_lossy(),
                e
            )
        })?;
        let config = toml::from_str(&toml).map_err(|e| {
            format!(
                "Error while parsing config file {}:\n{}\nContinuing with default config.",
                path.to_string_lossy(),
                e
            )
        })?;
        Ok(config)
    }
}

fn watcher_thread(init_config: Config, event_loop_proxy: EventLoopProxy<EventPayload>) {
    let config_path = config_path();
    let parent_path = match config_path.parent() {
        Some(dir) => dir,
        None => return,
    };
    let (tx, rx) = mpsc::channel();
    let mut debouncer = new_debouncer(Duration::from_millis(500), None, tx).unwrap();

    if let Err(e) = debouncer.watch(
        // watching the directory rather than the config file itself to also allow it to be deleted/created later on
        parent_path,
        RecursiveMode::NonRecursive,
    ) {
        log::warn!("Error while trying to watch config file parent directory for changes: {e}");
        return;
    }

    let mut previous_config = init_config;
    loop {
        if let Err(e) = rx.recv() {
            eprintln!("Error while watching config file: {e}");
            continue;
        }

        let config = match Config::load_from_path(&config_path) {
            Ok(config) => config,
            Err(maybe_err) => {
                if let Some(err) = maybe_err {
                    error_msg!("While reloading config file: {err}");
                }
                continue;
            }
        };

        // notify if font changed
        if config.font != previous_config.font {
            event_loop_proxy
                .send_event(EventPayload::all(UserEvent::ConfigsChanged(Box::new(
                    HotReloadConfigs::Renderer(RendererHotReloadConfigs::Font(Box::new(
                        config.font.clone(),
                    ))),
                ))))
                .unwrap();
        }
        if config.box_drawing != previous_config.box_drawing {
            event_loop_proxy
                .send_event(EventPayload::all(UserEvent::ConfigsChanged(Box::new(
                    HotReloadConfigs::Renderer(RendererHotReloadConfigs::BoxDrawing(
                        config.box_drawing.clone(),
                    )),
                ))))
                .unwrap();
        }
        if config.idle != previous_config.idle {
            event_loop_proxy
                .send_event(EventPayload::all(UserEvent::ConfigsChanged(Box::new(
                    HotReloadConfigs::App(AppHotReloadConfigs::Idle(config.idle.unwrap_or(true))),
                ))))
                .unwrap();
        }
        if config.title_hidden != previous_config.title_hidden {
            event_loop_proxy
                .send_event(EventPayload::all(UserEvent::ConfigsChanged(Box::new(
                    HotReloadConfigs::Window(WindowHotReloadConfigs::TitleHidden(
                        config.title_hidden,
                    )),
                ))))
                .unwrap();
        }
        if config.mouse_cursor_icon != previous_config.mouse_cursor_icon {
            match MouseCursorIcon::from_config(config.mouse_cursor_icon.as_deref()) {
                Ok(mouse_cursor_icon) => {
                    event_loop_proxy
                        .send_event(EventPayload::all(UserEvent::ConfigsChanged(Box::new(
                            HotReloadConfigs::Window(WindowHotReloadConfigs::MouseCursorIcon(
                                mouse_cursor_icon,
                            )),
                        ))))
                        .unwrap();
                }
                Err(err) => {
                    error_msg!("While reloading config file: invalid mouse-cursor-icon: {err}");
                }
            }
        }
        if config.size != previous_config.size
            || config.grid != previous_config.grid
            || config.maximized != previous_config.maximized
        {
            match GeometryArgs::from_config(
                config.size.as_deref(),
                config.grid.as_deref(),
                config.maximized,
            ) {
                Ok(geometry) => {
                    event_loop_proxy
                        .send_event(EventPayload::all(UserEvent::ConfigsChanged(Box::new(
                            HotReloadConfigs::Window(WindowHotReloadConfigs::Geometry(geometry)),
                        ))))
                        .unwrap();
                }
                Err(err) => {
                    error_msg!("While reloading config file: invalid geometry: {err}");
                }
            }
        }
        previous_config = config;
    }
}
