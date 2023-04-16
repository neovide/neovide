use std::{iter, mem};

use crate::{dimensions::Dimensions, frame::Frame, settings::*};

use clap::{builder::FalseyValueParser, ArgAction, Parser};

#[derive(Clone, Debug, Parser)]
#[command(version, about, long_about = None)]
pub struct CmdLineSettings {
    /// Files to open (plainly appended to NeoVim args)
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

    /// The geometry of the window
    #[arg(long)]
    pub geometry: Option<Dimensions>,

    /// The size of the window in pixel
    #[arg(long)]
    pub size: Option<Dimensions>,

    /// If to enable logging to a file in the current directory
    #[arg(long = "log")]
    pub log_to_file: bool,

    /// Connect to the named pipe or socket at ADDRESS
    #[arg(long, alias = "remote-tcp", value_name = "ADDRESS")]
    pub server: Option<String>,

    /// Run NeoVim in WSL rather than on the host
    #[arg(long)]
    pub wsl: bool,

    /// Which window decorations to use (do note that the window might not be resizable
    /// if this is "none")
    #[arg(long, env = "NEOVIDE_FRAME", default_value_t)]
    pub frame: Frame,

    /// Maximize the window on startup (not equivalent to fullscreen)
    #[arg(long, env = "NEOVIDE_MAXIMIZED", value_parser = FalseyValueParser::new())]
    pub maximized: bool,

    /// Enable the Multigrid extension (enables smooth scrolling and floating blur)
    #[arg(long = "multigrid", env = "NEOVIDE_MULTIGRID", value_parser = FalseyValueParser::new())]
    pub multi_grid: bool,

    /// Instead of spawning a child process and leaking it, be "blocking" and let the shell persist
    /// as parent process
    #[arg(long = "nofork")]
    pub no_fork: bool,

    /// Render every frame, takes more power and CPU time but possibly helps with frame timing
    /// issues
    #[arg(long = "noidle", env = "NEOVIDE_NO_IDLE", value_parser = FalseyValueParser::new())]
    pub no_idle: bool,

    /// Disable opening multiple files supplied in tabs (they're still buffers)
    #[arg(long = "notabs")]
    pub no_tabs: bool,

    /// Do not request sRGB when initializing the window, may help with GPUs with weird pixel
    /// formats
    #[arg(long = "nosrgb", env = "NEOVIDE_NO_SRGB", action = ArgAction::SetFalse, value_parser = FalseyValueParser::new())]
    pub srgb: bool,

    /// Do not try to request VSync on the window
    #[arg(long = "novsync", env = "NEOVIDE_NO_VSYNC", action = ArgAction::SetFalse, value_parser = FalseyValueParser::new())]
    pub vsync: bool,

    /// Which NeoVim binary to invoke headlessly instead of `nvim` found on $PATH
    #[arg(long = "neovim-bin", env = "NEOVIM_BIN")]
    pub neovim_bin: Option<String>,

    /// The app ID to show to the compositor (Wayland only, useful for setting WM rules)
    #[arg(
        long = "wayland_app_id",
        env = "NEOVIDE_APP_ID",
        default_value = "neovide"
    )]
    pub wayland_app_id: String,

    /// The class part of the X11 WM_CLASS property (X only, useful for setting WM rules)
    #[arg(
        long = "x11-wm-class",
        env = "NEOVIDE_WM_CLASS",
        default_value = "neovide"
    )]
    pub x11_wm_class: String,

    /// The instance part of the X11 WM_CLASS property (X only, useful for setting WM rules)
    #[arg(
        long = "x11-wm-class-instance",
        env = "NEOVIDE_WM_CLASS_INSTANCE",
        default_value = "neovide"
    )]
    pub x11_wm_class_instance: String,
}

impl Default for CmdLineSettings {
    fn default() -> Self {
        Self::parse_from(iter::empty::<String>())
    }
}

pub fn handle_command_line_arguments(args: Vec<String>) -> Result<(), String> {
    let mut cmdline = CmdLineSettings::parse_from(args);

    // The neovim_args in cmdline are unprocessed, actually add options to it
    let maybe_tab_flag = (!cmdline.no_tabs).then(|| "-p".to_string());

    cmdline.neovim_args = maybe_tab_flag
        .into_iter()
        .chain(mem::take(&mut cmdline.files_to_open))
        .chain(cmdline.neovim_args)
        .collect();

    SETTINGS.set::<CmdLineSettings>(&cmdline);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::env::set_var;
    use std::sync::Mutex;

    use lazy_static::lazy_static;

    use super::*;

    // Use a mutex to ensure that the settings are initialized and accessed in series
    lazy_static! {
        static ref ACCESSING_SETTINGS: Mutex<bool> = Mutex::new(false);
    }

    #[test]
    fn test_neovim_passthrough() {
        let args: Vec<String> = vec!["neovide", "--notabs", "--", "--clean"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let _accessing_settings = ACCESSING_SETTINGS.lock().unwrap();
        handle_command_line_arguments(args).expect("Could not parse arguments");
        assert_eq!(
            SETTINGS.get::<CmdLineSettings>().neovim_args,
            vec!["--clean"]
        );
    }

    #[test]
    fn test_files_to_open() {
        let args: Vec<String> = vec!["neovide", "./foo.txt", "--notabs", "./bar.md"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let _accessing_settings = ACCESSING_SETTINGS.lock().unwrap();
        handle_command_line_arguments(args).expect("Could not parse arguments");
        assert_eq!(
            SETTINGS.get::<CmdLineSettings>().neovim_args,
            vec!["./foo.txt", "./bar.md"]
        );
    }

    #[test]
    fn test_files_to_open_with_passthrough() {
        let args: Vec<String> = vec![
            "neovide",
            "--notabs",
            "./foo.txt",
            "./bar.md",
            "--",
            "--clean",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let _accessing_settings = ACCESSING_SETTINGS.lock().unwrap();
        handle_command_line_arguments(args).expect("Could not parse arguments");
        assert_eq!(
            SETTINGS.get::<CmdLineSettings>().neovim_args,
            vec!["./foo.txt", "./bar.md", "--clean"]
        );
    }

    #[test]
    fn test_files_to_open_with_flag() {
        let args: Vec<String> = vec!["neovide", "./foo.txt", "./bar.md", "--geometry=42x24"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let _accessing_settings = ACCESSING_SETTINGS.lock().unwrap();
        handle_command_line_arguments(args).expect("Could not parse arguments");
        assert_eq!(
            SETTINGS.get::<CmdLineSettings>().neovim_args,
            vec!["-p", "./foo.txt", "./bar.md"]
        );

        assert_eq!(
            SETTINGS.get::<CmdLineSettings>().geometry,
            Some(Dimensions {
                width: 42,
                height: 24
            }),
        );
    }

    #[test]
    fn test_geometry() {
        let args: Vec<String> = vec!["neovide", "--geometry=42x24"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let _accessing_settings = ACCESSING_SETTINGS.lock().unwrap();
        handle_command_line_arguments(args).expect("Could not parse arguments");
        assert_eq!(
            SETTINGS.get::<CmdLineSettings>().geometry,
            Some(Dimensions {
                width: 42,
                height: 24
            }),
        );
    }

    #[test]
    fn test_size() {
        let args: Vec<String> = vec!["neovide", "--size=420x240"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let _accessing_settings = ACCESSING_SETTINGS.lock().unwrap();
        handle_command_line_arguments(args).expect("Could not parse arguments");
        assert_eq!(
            SETTINGS.get::<CmdLineSettings>().size,
            Some(Dimensions {
                width: 420,
                height: 240,
            }),
        );
    }

    #[test]
    fn test_log_to_file() {
        let args: Vec<String> = vec!["neovide", "--log"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let _accessing_settings = ACCESSING_SETTINGS.lock().unwrap();
        handle_command_line_arguments(args).expect("Could not parse arguments");
        assert!(SETTINGS.get::<CmdLineSettings>().log_to_file);
    }

    #[test]
    fn test_frameless_flag() {
        let args: Vec<String> = vec!["neovide", "--frame=full"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let _accessing_settings = ACCESSING_SETTINGS.lock().unwrap();
        handle_command_line_arguments(args).expect("Could not parse arguments");
        assert_eq!(SETTINGS.get::<CmdLineSettings>().frame, Frame::Full);
    }

    #[test]
    fn test_frameless_environment_variable() {
        let args: Vec<String> = vec!["neovide"].iter().map(|s| s.to_string()).collect();

        let _accessing_settings = ACCESSING_SETTINGS.lock().unwrap();
        set_var("NEOVIDE_FRAME", "none");
        handle_command_line_arguments(args).expect("Could not parse arguments");
        assert_eq!(SETTINGS.get::<CmdLineSettings>().frame, Frame::None);
    }

    #[test]
    fn test_neovim_bin_arg() {
        let args: Vec<String> = vec!["neovide", "--neovim-bin", "foo"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let _accessing_settings = ACCESSING_SETTINGS.lock().unwrap();
        handle_command_line_arguments(args).expect("Could not parse arguments");
        assert_eq!(
            SETTINGS.get::<CmdLineSettings>().neovim_bin,
            Some("foo".to_owned())
        );
    }

    #[test]
    fn test_neovim_bin_environment_variable() {
        let args: Vec<String> = vec!["neovide"].iter().map(|s| s.to_string()).collect();

        let _accessing_settings = ACCESSING_SETTINGS.lock().unwrap();
        set_var("NEOVIM_BIN", "foo");
        handle_command_line_arguments(args).expect("Could not parse arguments");
        assert_eq!(
            SETTINGS.get::<CmdLineSettings>().neovim_bin,
            Some("foo".to_owned())
        );
    }
}
