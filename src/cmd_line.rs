#[cfg(target_os = "macos")]
use std::convert::Infallible;
use std::iter;
use std::process::ExitStatus;

use anyhow::Context;
use anyhow::Result;
use clap::ArgAction;
#[cfg(target_os = "macos")]
use clap::CommandFactory;
use clap::Parser;
use clap::ValueEnum;
use clap::builder::FalseyValueParser;
use clap::builder::Styles;
use clap::builder::styling;
#[cfg(target_os = "macos")]
use clap::parser::ValueSource;
use winit::window::CursorIcon;

use crate::bridge::create_blocking_nvim_command;
use crate::dimensions::Dimensions;
use crate::frame::Frame;
use crate::settings::*;
use crate::version::BUILD_VERSION;

#[cfg(target_os = "windows")]
pub const SRGB_DEFAULT: &str = "1";
#[cfg(not(target_os = "windows"))]
pub const SRGB_DEFAULT: &str = "0";

const NEOVIM_PASSTHROUGH_FLAGS: &[&str] = &["-h", "--help", "-?", "-v", "--version", "--api-info"];

fn get_styles() -> Styles {
    styling::Styles::styled()
        .header(styling::AnsiColor::Green.on_default() | styling::Effects::BOLD)
        .usage(styling::AnsiColor::Green.on_default() | styling::Effects::BOLD)
        .literal(styling::AnsiColor::Blue.on_default() | styling::Effects::BOLD)
        .placeholder(styling::AnsiColor::Cyan.on_default())
}

#[derive(Clone, Debug, Parser)]
#[command(version = BUILD_VERSION, about, long_about = None, styles = get_styles())]
pub struct CmdLineSettings {
    /// Files to open (usually plainly appended to NeoVim args, except when --wsl is used)
    #[arg(
        num_args = ..,
        action = ArgAction::Append,
    )]
    pub files_to_open: Vec<String>,

    /// Arguments to pass down to NeoVim without interpreting them
    #[arg(
        num_args = ..,
        action = ArgAction::Append,
        last = true,
        allow_hyphen_values = true
    )]
    pub neovim_args: Vec<String>,

    /// If to enable logging to a file in the current directory
    #[arg(long = "log")]
    pub log_to_file: bool,

    /// Connect to the named pipe or socket at ADDRESS
    #[arg(long, alias = "remote-tcp", env = "NEOVIDE_SERVER", value_name = "ADDRESS")]
    pub server: Option<String>,

    /// Open files in an existing Neovide app instance if one is already running
    #[cfg(target_os = "macos")]
    #[arg(long = "reuse-instance", action = ArgAction::SetTrue, default_value = "0", value_parser = FalseyValueParser::new())]
    pub reuse_instance: bool,

    /// Open files in a new window when reusing an existing Neovide instance
    #[cfg(target_os = "macos")]
    #[arg(long = "new-window", requires = "reuse_instance", action = ArgAction::SetTrue, default_value = "0", value_parser = FalseyValueParser::new())]
    pub new_window: bool,

    /// Run NeoVim in WSL rather than on the host
    #[arg(long, env = "NEOVIDE_WSL")]
    pub wsl: bool,

    /// Which window decorations to use (do note that the window might not be resizable
    /// if this is "none")
    #[arg(long, env = "NEOVIDE_FRAME", default_value_t)]
    pub frame: Frame,

    /// Disable the Multigrid extension (disables smooth scrolling, window animations, and floating blur)
    #[arg(long = "no-multigrid", env = "NEOVIDE_NO_MULTIGRID", value_parser = FalseyValueParser::new())]
    pub no_multi_grid: bool,

    /// Which mouse cursor icon to use
    #[arg(long = "mouse-cursor-icon", env = "NEOVIDE_MOUSE_CURSOR_ICON", default_value = "arrow")]
    pub mouse_cursor_icon: MouseCursorIcon,

    /// Sets title hidden for the window
    #[arg(long = "title-hidden", env = "NEOVIDE_TITLE_HIDDEN", value_parser = FalseyValueParser::new())]
    pub title_hidden: bool,

    /// Spawn a child process and leak it
    #[arg(long = "fork", env = "NEOVIDE_FORK", action = ArgAction::SetTrue, default_value = "0", value_parser = FalseyValueParser::new())]
    pub fork: bool,

    /// Be "blocking" and let the shell persist as parent process. Takes precedence over `--fork`. [DEFAULT]
    #[arg(long = "no-fork", action = ArgAction::SetTrue, value_parser = FalseyValueParser::new())]
    _no_fork: bool,

    /// Render every frame, takes more power and CPU time but possibly helps with frame timing
    /// issues
    #[arg(long = "no-idle", env = "NEOVIDE_IDLE", action = ArgAction::SetFalse, value_parser = FalseyValueParser::new())]
    pub idle: bool,

    /// Enable opening multiple files supplied in tabs [DEFAULT]
    #[arg(long = "tabs", env = "NEOVIDE_TABS", action = ArgAction::SetTrue, default_value = "1", value_parser = FalseyValueParser::new())]
    pub tabs: bool,

    /// Disable opening multiple files supplied in tabs (they're still buffers)
    #[arg(long = "no-tabs", action = ArgAction::SetTrue, value_parser = FalseyValueParser::new())]
    _no_tabs: bool,

    /// Keep the native tab bar visible when full or transparent windows merge
    #[cfg(target_os = "macos")]
    #[arg(long = "system-native-tabs", env = "NEOVIDE_SYSTEM_NATIVE_TABS", action = ArgAction::SetTrue, default_value = "0", value_parser = FalseyValueParser::new())]
    pub system_native_tabs: bool,

    /// Hide the native system tab bar even if the config enables it
    #[cfg(target_os = "macos")]
    #[arg(long = "no-system-native-tabs", action = ArgAction::SetTrue, value_parser = FalseyValueParser::new())]
    _no_system_native_tabs: bool,

    /// Set the Window > New Window shortcut
    #[cfg(target_os = "macos")]
    #[arg(
        long = "system-new-window-hotkey",
        env = "NEOVIDE_SYSTEM_NEW_WINDOW_HOTKEY",
        default_value = "cmd+n"
    )]
    pub system_new_window_hotkey: String,

    /// Set the Neovide > Hide shortcut
    #[cfg(target_os = "macos")]
    #[arg(
        long = "system-hide-hotkey",
        env = "NEOVIDE_SYSTEM_HIDE_HOTKEY",
        default_value = "cmd+h"
    )]
    pub system_hide_hotkey: String,

    /// Set the Neovide > Hide Others shortcut
    #[cfg(target_os = "macos")]
    #[arg(
        long = "system-hide-others-hotkey",
        env = "NEOVIDE_SYSTEM_HIDE_OTHERS_HOTKEY",
        default_value = "cmd+alt+h"
    )]
    pub system_hide_others_hotkey: String,

    /// Set the Neovide > Quit shortcut
    #[cfg(target_os = "macos")]
    #[arg(
        long = "system-quit-hotkey",
        env = "NEOVIDE_SYSTEM_QUIT_HOTKEY",
        default_value = "cmd+q"
    )]
    pub system_quit_hotkey: String,

    /// Set the Window > Minimize shortcut
    #[cfg(target_os = "macos")]
    #[arg(
        long = "system-minimize-hotkey",
        env = "NEOVIDE_SYSTEM_MINIMIZE_HOTKEY",
        default_value = "cmd+m"
    )]
    pub system_minimize_hotkey: String,

    /// Set the Window > Enter Full Screen shortcut
    #[cfg(target_os = "macos")]
    #[arg(
        long = "system-fullscreen-hotkey",
        env = "NEOVIDE_SYSTEM_FULLSCREEN_HOTKEY",
        default_value = "cmd+ctrl+f"
    )]
    pub system_fullscreen_hotkey: String,

    /// Set the Window > Editors shortcut
    #[cfg(target_os = "macos")]
    #[arg(
        long = "system-show-all-tabs-hotkey",
        env = "NEOVIDE_SYSTEM_SHOW_ALL_TABS_HOTKEY",
        default_value = "cmd+shift+e"
    )]
    pub system_show_all_tabs_hotkey: String,

    /// Set the Editors switcher glass style to regular or clear on macOS 26+
    //
    /// NOTE: the intention is to require the right value without adding any
    /// hack and without blocking the launcher in case of a non accepted value
    /// using the value_parser, but just `Option` makes clap expect a String,
    /// with absence handled by them. Surprisingly, using std::option::Option
    /// makes clap store the whole Option<String> returned by our custom
    /// parser, including None for an invalid type.
    ///
    /// See https://github.com/clap-rs/clap/issues/4626
    #[cfg(target_os = "macos")]
    #[arg(
        long = "system-switcher-glass-style",
        env = "NEOVIDE_SYSTEM_SWITCHER_GLASS_STYLE",
        value_parser = parse_system_switcher_glass_style,
        default_value = "",
        hide_default_value = true,
        value_name = "STYLE"
    )]
    pub system_switcher_glass_style: std::option::Option<String>,

    /// Set the Clear switcher backing opacity from 0 to 1 on macOS 26+
    #[cfg(target_os = "macos")]
    #[arg(
        long = "system-switcher-glass-dimming",
        env = "NEOVIDE_SYSTEM_SWITCHER_GLASS_DIMMING",
        default_value = "0.30",
        value_name = "ALPHA",
        allow_negative_numbers = true
    )]
    pub system_switcher_glass_dimming: String,

    /// Cycle to the previous system tab when pressed inside Neovide
    #[cfg(target_os = "macos")]
    #[arg(
        long = "system-tab-prev-hotkey",
        env = "NEOVIDE_SYSTEM_TAB_PREV_HOTKEY",
        default_value = "cmd+shift+["
    )]
    pub system_tab_prev_hotkey: String,

    /// Cycle to the next system tab when pressed inside Neovide
    #[cfg(target_os = "macos")]
    #[arg(
        long = "system-tab-next-hotkey",
        env = "NEOVIDE_SYSTEM_TAB_NEXT_HOTKEY",
        default_value = "cmd+shift+]"
    )]
    pub system_tab_next_hotkey: String,

    /// Request sRGB when initializing the window, may help with GPUs with weird pixel
    /// formats. Default on Windows.
    #[arg(long = "srgb", env = "NEOVIDE_SRGB", action = ArgAction::SetTrue, default_value = SRGB_DEFAULT, value_parser = FalseyValueParser::new())]
    pub srgb: bool,

    /// Do not request sRGB when initializing the window, may help with GPUs with weird pixel
    /// formats. Default on Linux and macOS.
    #[arg(long = "no-srgb", action = ArgAction::SetTrue, value_parser = FalseyValueParser::new())]
    _no_srgb: bool,

    /// Request VSync on the window [DEFAULT]
    #[arg(long = "vsync", env = "NEOVIDE_VSYNC", action = ArgAction::SetTrue, default_value = "1", value_parser = FalseyValueParser::new())]
    pub vsync: bool,

    /// Do not try to request VSync on the window
    #[arg(long = "no-vsync", action = ArgAction::SetTrue, value_parser = FalseyValueParser::new())]
    _no_vsync: bool,

    /// Temporarily capture startup messages before the first grid render [DEFAULT]
    #[arg(long = "startup-message-capture", env = "NEOVIDE_STARTUP_MESSAGE_CAPTURE", action = ArgAction::SetTrue, default_value = "1", value_parser = FalseyValueParser::new())]
    pub startup_message_capture: bool,

    /// Do not temporarily capture startup messages before the first grid render
    #[arg(long = "no-startup-message-capture", action = ArgAction::SetTrue, value_parser = FalseyValueParser::new())]
    _no_startup_message_capture: bool,

    /// Which NeoVim binary to invoke headlessly instead of `nvim` found on $PATH
    #[arg(long = "neovim-bin", env = "NEOVIM_BIN", num_args = 1, action = ArgAction::Set)]
    pub neovim_bin: Option<Vec<String>>,

    /// The app ID to show to the compositor (Wayland only, useful for setting WM rules)
    #[arg(
        long = "wayland-app-id",
        alias = "wayland_app_id",
        env = "NEOVIDE_APP_ID",
        default_value = "neovide"
    )]
    pub wayland_app_id: String,

    /// The class part of the X11 WM_CLASS property (X only, useful for setting WM rules)
    #[arg(long = "x11-wm-class", env = "NEOVIDE_WM_CLASS", default_value = "neovide")]
    pub x11_wm_class: String,

    /// The instance part of the X11 WM_CLASS property (X only, useful for setting WM rules)
    #[arg(
        long = "x11-wm-class-instance",
        env = "NEOVIDE_WM_CLASS_INSTANCE",
        default_value = "neovide"
    )]
    pub x11_wm_class_instance: String,

    /// The custom icon to use for the app.
    #[arg(long, env = "NEOVIDE_ICON")]
    pub icon: Option<String>,

    #[command(flatten)]
    pub geometry: GeometryArgs,

    /// Force opengl on Windows or macOS
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    #[arg(long = "opengl", env = "NEOVIDE_OPENGL", action = ArgAction::SetTrue, value_parser = FalseyValueParser::new())]
    pub opengl: bool,

    /// Change to this directory during startup.
    #[arg(long = "chdir", env = "NEOVIDE_CHDIR")]
    pub chdir: Option<String>,
}

// geometry, size and maximized are mutually exclusive
#[derive(Clone, Debug, Args, PartialEq)]
#[group(required = false, multiple = false)]
pub struct GeometryArgs {
    /// The initial grid size of the window [<columns>x<lines>]. Defaults to columns/lines from init.vim/lua if no value is given.
    /// If --grid is not set then it's inferred from the window size
    #[arg(long, env = "NEOVIDE_GRID")]
    pub grid: Option<Option<Dimensions>>,

    /// The size of the window in pixels.
    #[arg(long, env = "NEOVIDE_SIZE")]
    pub size: Option<Dimensions>,

    /// Maximize the window on startup (not equivalent to fullscreen)
    #[arg(long, env = "NEOVIDE_MAXIMIZED", value_parser = FalseyValueParser::new())]
    pub maximized: bool,
}

#[derive(ValueEnum, Clone, Debug, PartialEq, Eq)]
pub enum MouseCursorIcon {
    Arrow,
    IBeam,
}

impl MouseCursorIcon {
    pub fn from_config(value: Option<&str>) -> Result<Self, String> {
        value.map_or(Ok(Self::Arrow), |value| <Self as ValueEnum>::from_str(value, false))
    }

    pub fn parse(&self) -> CursorIcon {
        match self {
            MouseCursorIcon::Arrow => CursorIcon::Default,
            MouseCursorIcon::IBeam => CursorIcon::Text,
        }
    }
}

impl GeometryArgs {
    pub fn from_config(
        size: Option<&str>,
        grid: Option<&str>,
        maximized: Option<bool>,
    ) -> Result<Self, String> {
        let maximized = maximized.unwrap_or(false);
        let has_size = size.is_some();
        let has_grid = grid.is_some();
        let conflicting = (has_size && has_grid) || (maximized && (has_size || has_grid));
        if conflicting {
            return Err("size, grid and maximized are mutually exclusive".to_owned());
        }

        Ok(Self {
            grid: grid.map(|grid| grid.parse::<Dimensions>().map(Some)).transpose()?,
            size: size.map(str::parse::<Dimensions>).transpose()?,
            maximized,
        })
    }
}

impl Default for CmdLineSettings {
    fn default() -> Self { Self::parse_from(iter::empty::<String>()) }
}

pub fn handle_command_line_arguments(
    args: Vec<String>,
    settings: &Settings,
    config: &Config,
) -> Result<()> {
    let mut cmdline = CmdLineSettings::try_parse_from(args)?;

    if cmdline._no_tabs {
        cmdline.tabs = false;
    }

    #[cfg(target_os = "macos")]
    if cmdline._no_system_native_tabs {
        cmdline.system_native_tabs = false;
    }

    if cmdline._no_fork {
        cmdline.fork = false;
    }

    if cmdline._no_srgb {
        cmdline.srgb = false;
    }

    if cmdline._no_vsync {
        cmdline.vsync = false;
    }

    if cmdline._no_startup_message_capture {
        cmdline.startup_message_capture = false;
    }

    // $NEOVIM_BIN is cleared by Config::write_to_env when the config key is set, so the config
    // key cannot take precedence over it here.
    if cmdline.neovim_bin.is_none() {
        cmdline.neovim_bin = config.neovim_bin.clone().filter(|command| !command.is_empty());
    }

    settings.set::<CmdLineSettings>(&cmdline);
    Ok(())
}

#[cfg(target_os = "macos")]
fn parse_system_switcher_glass_style(value: &str) -> Result<Option<String>, Infallible> {
    Ok(matches!(value, "regular" | "clear").then(|| value.to_owned()))
}

#[cfg(target_os = "macos")]
pub fn argv_chdir() -> Option<String> {
    let matches = CmdLineSettings::command().try_get_matches_from(std::env::args_os()).ok()?;

    (matches.value_source("chdir") == Some(ValueSource::CommandLine))
        .then(|| matches.get_one::<String>("chdir").cloned())
        .flatten()
}

pub fn maybe_passthrough_to_neovim(
    cmdline_settings: &CmdLineSettings,
) -> Result<Option<ExitStatus>> {
    if !neovim_passthrough_requested(&cmdline_settings.neovim_args) {
        return Ok(None);
    }

    let mut command = create_blocking_nvim_command(cmdline_settings, false);
    let binary = cmdline_settings
        .neovim_bin
        .as_ref()
        .and_then(|command| command.first())
        .map_or("nvim", String::as_str);
    let status = command
        .status()
        .with_context(|| format!("Failed to run {binary} for passthrough output"))?;

    Ok(Some(status))
}

pub fn exit_status_code(status: ExitStatus) -> i32 {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        status.code().unwrap_or_else(|| 128 + status.signal().unwrap_or(0))
    }

    #[cfg(windows)]
    {
        status.code().unwrap_or(1)
    }

    #[cfg(not(any(unix, windows)))]
    {
        return status.code().unwrap_or(1);
    }
}

fn neovim_passthrough_requested(args: &[String]) -> bool {
    args.iter().any(|arg| NEOVIM_PASSTHROUGH_FLAGS.contains(&arg.as_str()))
}

#[cfg(test)]
#[allow(clippy::bool_assert_comparison)] // useful here since the explicit true/false comparison matters
#[serial_test::serial]
mod tests {
    use scoped_env::ScopedEnv;

    use super::*;

    #[test]
    fn test_neovim_passthrough() {
        let settings = Settings::new();
        let args: Vec<String> =
            ["neovide", "--no-tabs", "--", "--clean"].iter().map(|s| s.to_string()).collect();

        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        assert_eq!(settings.get::<CmdLineSettings>().neovim_args, vec!["--clean"]);
    }

    #[test]
    fn test_files_to_open() {
        let settings = Settings::new();
        let args: Vec<String> = ["neovide", "./foo.txt", "--no-tabs", "./bar.md"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        assert_eq!(settings.get::<CmdLineSettings>().files_to_open, vec!["./foo.txt", "./bar.md"]);
        assert!(settings.get::<CmdLineSettings>().neovim_args.is_empty());
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_files_to_open_with_wsl() {
        let settings = Settings::new();
        let args: Vec<String> = [
            "neovide",
            "--wsl",
            "C:\\Users\\MyUser\\foo.txt",
            "--no-tabs",
            "C:\\bar.md",
            "C:\\Program Files (x86)\\Some Application\\Settings.ini",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        assert_eq!(
            settings.get::<CmdLineSettings>().files_to_open,
            vec![
                "C:\\Users\\MyUser\\foo.txt",
                "C:\\bar.md",
                "C:\\Program Files (x86)\\Some Application\\Settings.ini"
            ]
        );
        assert!(settings.get::<CmdLineSettings>().neovim_args.is_empty());
    }

    #[test]
    fn test_files_to_open_with_passthrough() {
        let settings = Settings::new();
        let args: Vec<String> = ["neovide", "--no-tabs", "./foo.txt", "./bar.md", "--", "--clean"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        assert_eq!(settings.get::<CmdLineSettings>().neovim_args, vec!["--clean"]);
        assert_eq!(settings.get::<CmdLineSettings>().files_to_open, vec!["./foo.txt", "./bar.md"]);
    }

    #[test]
    fn test_files_to_open_with_flag() {
        let settings = Settings::new();
        let args: Vec<String> = ["neovide", "./foo.txt", "./bar.md", "--grid=420x240"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        assert!(settings.get::<CmdLineSettings>().neovim_args.is_empty());
        assert_eq!(settings.get::<CmdLineSettings>().files_to_open, vec!["./foo.txt", "./bar.md"]);

        assert_eq!(
            settings.get::<CmdLineSettings>().geometry.grid,
            Some(Some(Dimensions { width: 420, height: 240 })),
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_reuse_instance_flag() {
        let settings = Settings::new();
        let args: Vec<String> =
            ["neovide", "--reuse-instance", "./foo.txt"].iter().map(|s| s.to_string()).collect();

        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        assert!(settings.get::<CmdLineSettings>().reuse_instance);
        assert_eq!(settings.get::<CmdLineSettings>().files_to_open, vec!["./foo.txt"]);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_new_window_flag() {
        let settings = Settings::new();
        let args: Vec<String> = ["neovide", "--reuse-instance", "--new-window", "./foo.txt"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        assert!(settings.get::<CmdLineSettings>().reuse_instance);
        assert!(settings.get::<CmdLineSettings>().new_window);
        assert_eq!(settings.get::<CmdLineSettings>().files_to_open, vec!["./foo.txt"]);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_new_window_requires_reuse_instance() {
        let settings = Settings::new();
        let args: Vec<String> =
            ["neovide", "--new-window", "./foo.txt"].iter().map(|s| s.to_string()).collect();

        assert!(handle_command_line_arguments(args, &settings, &Config::default()).is_err());
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_system_new_window_hotkey_flag() {
        let settings = Settings::new();
        let args: Vec<String> = ["neovide", "--system-new-window-hotkey", "cmd+shift+n"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        assert_eq!(settings.get::<CmdLineSettings>().system_new_window_hotkey, "cmd+shift+n");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_system_new_window_hotkey_environment_variable() {
        let settings = Settings::new();
        let args: Vec<String> = ["neovide"].iter().map(|s| s.to_string()).collect();

        let _env = ScopedEnv::set("NEOVIDE_SYSTEM_NEW_WINDOW_HOTKEY", "ctrl+shift+n");
        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        assert_eq!(settings.get::<CmdLineSettings>().system_new_window_hotkey, "ctrl+shift+n");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_system_menu_hotkey_flags() {
        let settings = Settings::new();
        let args: Vec<String> = [
            "neovide",
            "--system-hide-hotkey",
            "ctrl+h",
            "--system-hide-others-hotkey",
            "ctrl+alt+h",
            "--system-quit-hotkey",
            "cmd+shift+q",
            "--system-minimize-hotkey",
            "cmd+shift+m",
            "--system-fullscreen-hotkey",
            "ctrl+alt+f",
            "--system-show-all-tabs-hotkey",
            "cmd+e",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        let cmdline = settings.get::<CmdLineSettings>();
        assert_eq!(cmdline.system_hide_hotkey, "ctrl+h");
        assert_eq!(cmdline.system_hide_others_hotkey, "ctrl+alt+h");
        assert_eq!(cmdline.system_quit_hotkey, "cmd+shift+q");
        assert_eq!(cmdline.system_minimize_hotkey, "cmd+shift+m");
        assert_eq!(cmdline.system_fullscreen_hotkey, "ctrl+alt+f");
        assert_eq!(cmdline.system_show_all_tabs_hotkey, "cmd+e");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_system_menu_hotkey_environment_variables() {
        let settings = Settings::new();
        let args: Vec<String> = ["neovide"].iter().map(|s| s.to_string()).collect();

        let _hide = ScopedEnv::set("NEOVIDE_SYSTEM_HIDE_HOTKEY", "ctrl+h");
        let _hide_others = ScopedEnv::set("NEOVIDE_SYSTEM_HIDE_OTHERS_HOTKEY", "ctrl+alt+h");
        let _quit = ScopedEnv::set("NEOVIDE_SYSTEM_QUIT_HOTKEY", "ctrl+q");
        let _minimize = ScopedEnv::set("NEOVIDE_SYSTEM_MINIMIZE_HOTKEY", "ctrl+m");
        let _fullscreen = ScopedEnv::set("NEOVIDE_SYSTEM_FULLSCREEN_HOTKEY", "ctrl+shift+f");
        let _show_all_tabs = ScopedEnv::set("NEOVIDE_SYSTEM_SHOW_ALL_TABS_HOTKEY", "ctrl+shift+e");

        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        let cmdline = settings.get::<CmdLineSettings>();
        assert_eq!(cmdline.system_hide_hotkey, "ctrl+h");
        assert_eq!(cmdline.system_hide_others_hotkey, "ctrl+alt+h");
        assert_eq!(cmdline.system_quit_hotkey, "ctrl+q");
        assert_eq!(cmdline.system_minimize_hotkey, "ctrl+m");
        assert_eq!(cmdline.system_fullscreen_hotkey, "ctrl+shift+f");
        assert_eq!(cmdline.system_show_all_tabs_hotkey, "ctrl+shift+e");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_system_switcher_glass_defaults() {
        let _style = ScopedEnv::remove("NEOVIDE_SYSTEM_SWITCHER_GLASS_STYLE");
        let _dimming = ScopedEnv::remove("NEOVIDE_SYSTEM_SWITCHER_GLASS_DIMMING");
        let cmdline = CmdLineSettings::try_parse_from(["neovide"]).unwrap();

        assert_eq!(cmdline.system_switcher_glass_style, None);
        assert_eq!(cmdline.system_switcher_glass_dimming, "0.30");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_system_switcher_glass_environment_requires_style() {
        let _style = ScopedEnv::remove("NEOVIDE_SYSTEM_SWITCHER_GLASS_STYLE");
        let _dimming = ScopedEnv::set("NEOVIDE_SYSTEM_SWITCHER_GLASS_DIMMING", "0.75");
        let settings = Settings::new();
        handle_command_line_arguments(vec!["neovide".into()], &settings, &Config::default())
            .unwrap();
        assert_eq!(settings.get::<CmdLineSettings>().system_switcher_glass_style, None);

        let _style = ScopedEnv::set("NEOVIDE_SYSTEM_SWITCHER_GLASS_STYLE", "regular");
        handle_command_line_arguments(vec!["neovide".into()], &settings, &Config::default())
            .unwrap();
        let cmdline = settings.get::<CmdLineSettings>();
        assert_eq!(cmdline.system_switcher_glass_style.as_deref(), Some("regular"));
        assert_eq!(cmdline.system_switcher_glass_dimming, "0.75");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_system_switcher_glass_flags_override_environment_independently() {
        let _style = ScopedEnv::set("NEOVIDE_SYSTEM_SWITCHER_GLASS_STYLE", "clear");
        let _dimming = ScopedEnv::set("NEOVIDE_SYSTEM_SWITCHER_GLASS_DIMMING", "0.75");
        let cmdline = CmdLineSettings::try_parse_from(["neovide"]).unwrap();
        assert_eq!(cmdline.system_switcher_glass_style.as_deref(), Some("clear"));
        assert_eq!(cmdline.system_switcher_glass_dimming, "0.75");

        let cmdline = CmdLineSettings::try_parse_from([
            "neovide",
            "--system-switcher-glass-style",
            "regular",
        ])
        .unwrap();
        assert_eq!(cmdline.system_switcher_glass_style.as_deref(), Some("regular"));
        assert_eq!(cmdline.system_switcher_glass_dimming, "0.75");

        for dimming in ["0", "1"] {
            let cmdline = CmdLineSettings::try_parse_from([
                "neovide",
                "--system-switcher-glass-dimming",
                dimming,
            ])
            .unwrap();
            assert_eq!(cmdline.system_switcher_glass_style.as_deref(), Some("clear"));
            assert_eq!(cmdline.system_switcher_glass_dimming, dimming);
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_system_switcher_glass_invalid_environment_is_ignored() {
        let _dimming = ScopedEnv::remove("NEOVIDE_SYSTEM_SWITCHER_GLASS_DIMMING");
        for value in ["", " ", "unknown", "opaque", "Clear", "CLEAR", "clear "] {
            let _style = ScopedEnv::set("NEOVIDE_SYSTEM_SWITCHER_GLASS_STYLE", value);
            let cmdline = CmdLineSettings::try_parse_from(["neovide"]).unwrap();
            assert_eq!(cmdline.system_switcher_glass_style, None, "{value:?}");
            assert_eq!(cmdline.system_switcher_glass_dimming, "0.30");

            for style in ["regular", "clear"] {
                let args = vec!["neovide".into(), format!("--system-switcher-glass-style={style}")];
                let cmdline = CmdLineSettings::try_parse_from(args).unwrap();
                assert_eq!(cmdline.system_switcher_glass_style.as_deref(), Some(style));
            }
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_system_switcher_glass_missing_value_is_an_error() {
        let _style = ScopedEnv::remove("NEOVIDE_SYSTEM_SWITCHER_GLASS_STYLE");
        let _dimming = ScopedEnv::remove("NEOVIDE_SYSTEM_SWITCHER_GLASS_DIMMING");
        let settings = Settings::new();
        let args = vec!["neovide".into(), "--system-switcher-glass-style".into()];

        let error = handle_command_line_arguments(args, &settings, &Config::default()).unwrap_err();
        assert_eq!(
            error.downcast_ref::<clap::Error>().unwrap().kind(),
            clap::error::ErrorKind::InvalidValue
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_system_switcher_glass_dimming_reaches_material_validation() {
        let _style = ScopedEnv::set("NEOVIDE_SYSTEM_SWITCHER_GLASS_STYLE", "clear");
        let _dimming = ScopedEnv::set("NEOVIDE_SYSTEM_SWITCHER_GLASS_DIMMING", "NaN");
        let cmdline = CmdLineSettings::try_parse_from(["neovide"]).unwrap();
        assert_eq!(cmdline.system_switcher_glass_style.as_deref(), Some("clear"));
        assert_eq!(cmdline.system_switcher_glass_dimming, "NaN");

        for dimming in ["1.1", "-0.1"] {
            let cmdline = CmdLineSettings::try_parse_from([
                "neovide",
                "--system-switcher-glass-dimming",
                dimming,
            ])
            .unwrap();
            assert_eq!(cmdline.system_switcher_glass_style.as_deref(), Some("clear"));
            assert_eq!(cmdline.system_switcher_glass_dimming, dimming);
        }
    }

    #[test]
    fn test_grid() {
        let settings = Settings::new();
        let args: Vec<String> =
            ["neovide", "--grid=420x240"].iter().map(|s| s.to_string()).collect();

        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        assert_eq!(
            settings.get::<CmdLineSettings>().geometry.grid,
            Some(Some(Dimensions { width: 420, height: 240 })),
        );
    }

    #[test]
    fn test_grid_environment_variable() {
        let settings = Settings::new();
        let args: Vec<String> = ["neovide"].iter().map(|s| s.to_string()).collect();

        let _env = ScopedEnv::set("NEOVIDE_GRID", "420x240");
        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        assert_eq!(
            settings.get::<CmdLineSettings>().geometry.grid,
            Some(Some(Dimensions { width: 420, height: 240 })),
        );
    }

    #[test]
    fn test_size() {
        let settings = Settings::new();
        let args: Vec<String> =
            ["neovide", "--size=420x240"].iter().map(|s| s.to_string()).collect();

        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        assert_eq!(
            settings.get::<CmdLineSettings>().geometry.size,
            Some(Dimensions { width: 420, height: 240 }),
        );
    }

    #[test]
    fn test_size_environment_variable() {
        let settings = Settings::new();
        let args: Vec<String> = ["neovide"].iter().map(|s| s.to_string()).collect();

        let _env = ScopedEnv::set("NEOVIDE_SIZE", "420x240");
        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        assert_eq!(
            settings.get::<CmdLineSettings>().geometry.size,
            Some(Dimensions { width: 420, height: 240 }),
        );
    }

    #[test]
    fn test_geometry_args_from_config_size() {
        assert_eq!(
            GeometryArgs::from_config(Some("420x240"), None, None).unwrap(),
            GeometryArgs {
                size: Some(Dimensions { width: 420, height: 240 }),
                grid: None,
                maximized: false,
            }
        );
    }

    #[test]
    fn test_geometry_args_from_config_grid() {
        assert_eq!(
            GeometryArgs::from_config(None, Some("80x24"), None).unwrap(),
            GeometryArgs {
                size: None,
                grid: Some(Some(Dimensions { width: 80, height: 24 })),
                maximized: false,
            }
        );
    }

    #[test]
    fn test_geometry_args_from_config_rejects_conflicts() {
        assert_eq!(
            GeometryArgs::from_config(Some("420x240"), Some("80x24"), None).unwrap_err(),
            "size, grid and maximized are mutually exclusive"
        );
    }

    #[test]
    fn test_server_environment_variable() {
        let settings = Settings::new();
        let args: Vec<String> = ["neovide"].iter().map(|s| s.to_string()).collect();

        let _env = ScopedEnv::set("NEOVIDE_SERVER", "127.0.0.1:7777");
        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        assert_eq!(settings.get::<CmdLineSettings>().server, Some("127.0.0.1:7777".to_string()));
    }

    #[test]
    fn test_log_to_file() {
        let settings = Settings::new();
        let args: Vec<String> = ["neovide", "--log"].iter().map(|s| s.to_string()).collect();

        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        assert!(settings.get::<CmdLineSettings>().log_to_file);
    }

    #[test]
    fn test_passthrough_detection_help() {
        assert!(super::neovim_passthrough_requested(&["-h".into()]));
    }

    #[test]
    fn test_passthrough_detection_none() {
        assert!(!super::neovim_passthrough_requested(&["file".into(), "--clean".into()]));
    }

    #[test]
    fn test_frameless_flag() {
        let settings = Settings::new();
        let args: Vec<String> = ["neovide", "--frame=full"].iter().map(|s| s.to_string()).collect();

        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        assert_eq!(settings.get::<CmdLineSettings>().frame, Frame::Full);
    }

    #[test]
    fn test_frameless_environment_variable() {
        let settings = Settings::new();
        let args: Vec<String> = ["neovide"].iter().map(|s| s.to_string()).collect();

        let _env = ScopedEnv::set("NEOVIDE_FRAME", "none");
        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        assert_eq!(settings.get::<CmdLineSettings>().frame, Frame::None);
    }

    #[test]
    fn test_neovim_bin_arg() {
        let settings = Settings::new();
        let args: Vec<String> =
            ["neovide", "--neovim-bin", "foo"].iter().map(|s| s.to_string()).collect();

        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        assert_eq!(settings.get::<CmdLineSettings>().neovim_bin, Some(vec!["foo".to_owned()]));
    }

    #[test]
    fn config_neovim_bin_args_do_not_trigger_passthrough() {
        let settings = Settings::new();
        let config = Config {
            neovim_bin: Some(vec!["true".to_owned(), "-v".to_owned()]),
            ..Config::default()
        };
        let args: Vec<String> = ["neovide"].iter().map(|s| s.to_string()).collect();

        handle_command_line_arguments(args, &settings, &config).expect("Could not parse arguments");
        let cmdline = settings.get::<CmdLineSettings>();
        assert!(maybe_passthrough_to_neovim(&cmdline).unwrap().is_none());
    }

    #[test]
    fn test_neovim_bin_environment_variable() {
        let settings = Settings::new();
        let args: Vec<String> = ["neovide"].iter().map(|s| s.to_string()).collect();

        let _env = ScopedEnv::set("NEOVIM_BIN", "foo");
        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        assert_eq!(settings.get::<CmdLineSettings>().neovim_bin, Some(vec!["foo".to_owned()]));
    }

    #[test]
    fn test_srgb_default() {
        let settings = Settings::new();
        let args: Vec<String> = ["neovide"].iter().map(|s| s.to_string()).collect();

        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        #[cfg(target_os = "windows")]
        let default_value = true;
        #[cfg(not(target_os = "windows"))]
        let default_value = false;
        assert_eq!(settings.get::<CmdLineSettings>().srgb, default_value);
    }

    #[test]
    fn test_srgb() {
        let settings = Settings::new();
        let args: Vec<String> = ["neovide", "--srgb"].iter().map(|s| s.to_string()).collect();

        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        assert_eq!(settings.get::<CmdLineSettings>().srgb, true);
    }

    #[test]
    fn test_nosrgb() {
        let settings = Settings::new();
        let args: Vec<String> = ["neovide", "--no-srgb"].iter().map(|s| s.to_string()).collect();

        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        assert_eq!(settings.get::<CmdLineSettings>().srgb, false);
    }

    #[test]
    fn test_no_srgb_environment() {
        let settings = Settings::new();
        let args: Vec<String> = ["neovide"].iter().map(|s| s.to_string()).collect();

        let _env = ScopedEnv::set("NEOVIDE_SRGB", "0");
        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        assert_eq!(settings.get::<CmdLineSettings>().srgb, false);
    }

    #[test]
    fn test_override_srgb_environment() {
        let settings = Settings::new();
        let args: Vec<String> = ["neovide", "--no-srgb"].iter().map(|s| s.to_string()).collect();

        let _env = ScopedEnv::set("NEOVIDE_SRGB", "1");
        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        assert_eq!(settings.get::<CmdLineSettings>().srgb, false);
    }

    #[test]
    fn test_override_nosrgb_environment() {
        let settings = Settings::new();
        let args: Vec<String> = ["neovide", "--srgb"].iter().map(|s| s.to_string()).collect();

        let _env = ScopedEnv::set("NEOVIDE_SRGB", "0");
        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        assert_eq!(settings.get::<CmdLineSettings>().srgb, true,);
    }

    #[test]
    fn test_vsync_default() {
        let settings = Settings::new();
        let args: Vec<String> = ["neovide"].iter().map(|s| s.to_string()).collect();

        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        assert_eq!(settings.get::<CmdLineSettings>().vsync, true);
    }

    #[test]
    fn test_vsync() {
        let settings = Settings::new();
        let args: Vec<String> = ["neovide", "--vsync"].iter().map(|s| s.to_string()).collect();

        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        assert_eq!(settings.get::<CmdLineSettings>().vsync, true);
    }

    #[test]
    fn test_novsync() {
        let settings = Settings::new();
        let args: Vec<String> = ["neovide", "--no-vsync"].iter().map(|s| s.to_string()).collect();

        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        assert_eq!(settings.get::<CmdLineSettings>().vsync, false);
    }

    #[test]
    fn test_no_vsync_environment() {
        let settings = Settings::new();
        let args: Vec<String> = ["neovide"].iter().map(|s| s.to_string()).collect();

        let _env = ScopedEnv::set("NEOVIDE_VSYNC", "0");
        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        assert_eq!(settings.get::<CmdLineSettings>().vsync, false);
    }

    #[test]
    fn test_override_vsync_environment() {
        let settings = Settings::new();
        let args: Vec<String> = ["neovide", "--no-vsync"].iter().map(|s| s.to_string()).collect();

        let _env = ScopedEnv::set("NEOVIDE_VSYNC", "1");
        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        assert_eq!(settings.get::<CmdLineSettings>().vsync, false);
    }

    #[test]
    fn test_override_novsync_environment() {
        let settings = Settings::new();
        let args: Vec<String> = ["neovide", "--vsync"].iter().map(|s| s.to_string()).collect();

        let _env = ScopedEnv::set("NEOVIDE_VSYNC", "0");
        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        assert_eq!(settings.get::<CmdLineSettings>().vsync, true,);
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn test_system_native_tabs_flag() {
        let settings = Settings::new();
        let args: Vec<String> =
            ["neovide", "--system-native-tabs"].iter().map(|s| s.to_string()).collect();

        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        assert!(settings.get::<CmdLineSettings>().system_native_tabs);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_system_native_tabs_env() {
        let settings = Settings::new();
        let args: Vec<String> = ["neovide"].iter().map(|s| s.to_string()).collect();

        let _env = ScopedEnv::set("NEOVIDE_SYSTEM_NATIVE_TABS", "1");
        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        assert!(settings.get::<CmdLineSettings>().system_native_tabs);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_system_native_tabs_override_env() {
        let settings = Settings::new();
        let args: Vec<String> =
            ["neovide", "--no-system-native-tabs"].iter().map(|s| s.to_string()).collect();

        let _env = ScopedEnv::set("NEOVIDE_SYSTEM_NATIVE_TABS", "1");
        handle_command_line_arguments(args, &settings, &Config::default())
            .expect("Could not parse arguments");
        assert!(!settings.get::<CmdLineSettings>().system_native_tabs);
    }
}
