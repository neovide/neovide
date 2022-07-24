use crate::{dimensions::Dimensions, frame::Frame, settings::*};

use clap::{Arg, Command};

#[derive(Clone, Debug)]
pub struct CmdLineSettings {
    // Pass through arguments
    pub neovim_args: Vec<String>,
    // Command-line arguments only
    pub geometry: Dimensions,
    pub log_to_file: bool,
    pub no_fork: bool,
    pub remote_tcp: Option<String>,
    pub wsl: bool,
    // Command-line flags with environment variable fallback
    pub frame: Frame,
    pub maximized: bool,
    pub multi_grid: bool,
    pub no_idle: bool,
    pub srgb: bool,
    // Command-line arguments with environment variable fallback
    pub neovim_bin: Option<String>,
    pub wayland_app_id: String,
    pub x11_wm_class: String,
    // Disable open multiple files as tabs
    pub no_tabs: bool,
}

impl Default for CmdLineSettings {
    fn default() -> Self {
        Self {
            // Pass through arguments
            neovim_args: vec![],
            // Command-line arguments only
            geometry: DEFAULT_WINDOW_GEOMETRY,
            log_to_file: false,
            no_fork: false,
            remote_tcp: None,
            wsl: false,
            // Command-line flags with environment variable fallback
            frame: Frame::Full,
            maximized: false,
            multi_grid: false,
            no_idle: false,
            srgb: true,
            // Command-line arguments with environment variable fallback
            neovim_bin: None,
            wayland_app_id: String::new(),
            x11_wm_class: String::new(),
            // Disable open multiple files as tabs
            no_tabs: false,
        }
    }
}

pub fn handle_command_line_arguments(args: Vec<String>) -> Result<(), String> {
    let clapp = Command::new("Neovide")
        .version(crate_version!())
        .author(crate_authors!())
        .about(crate_description!())
        // Pass through arguments
        .arg(
            Arg::new("files_to_open")
                .multiple_values(true)
                .takes_value(true)
                .help("Files to open"),
        )
        .arg(
            Arg::new("neovim_args")
                .multiple_values(true)
                .takes_value(true)
                .last(true)
                .help("Specify Arguments to pass down to neovim"),
        )
        // Command-line arguments only
        .arg(
            Arg::new("geometry")
                .long("geometry")
                .takes_value(true)
                .help("Specify the Geometry of the window"),
        )
        .arg(
            Arg::new("log_to_file")
                .long("log")
                .help("Log to a file"),
        )
        .arg(
            Arg::new("nofork")
                .long("nofork")
                .help(r#"Instead of spawning a child process and leaking it, be "blocking" and have the shell directly as parent process"#),
        )
        .arg(
            Arg::new("no_tabs")
                .long("notabs")
                .help("Disable open multiple files as tabs"),
        )
        .arg(
            Arg::new("remote_tcp")
                .long("remote-tcp")
                .takes_value(true)
                .help("Connect to Remote TCP"),
        )
        .arg(
            Arg::new("wsl")
                .long("wsl")
                .help("Run in WSL")
        )
        // Command-line flags with environment variable fallback
        .arg(
            Arg::new("frame")
            .long("frame")
            .takes_value(true)
            .help("Configure the window frame. NOTE: Window might not be resizable if setting is None.")
        )
        .arg(
            Arg::new("maximized")
                .long("maximized")
                .help("Maximize the window"),
        )
        .arg(
            Arg::new("multi_grid")
                .long("multigrid")
                .help("Enable Multigrid"),
        )
        .arg(
            Arg::new("noidle")
                .long("noidle")
                .help("Render every frame. Takes more power and cpu time but possibly fixes animation issues"),
        )
        .arg(
            Arg::new("nosrgb")
                .long("nosrgb")
                .help("Do not use standard color space to initialize the window. Swapping this variable sometimes fixes issues on startup"),
        )
        // Command-line arguments with environment variable fallback
        .arg(
            Arg::new("neovim_bin")
                .long("neovim-bin")
                .takes_value(true)
                .help("Specify path to neovim"),
        )
        .arg(
            Arg::new("wayland_app_id")
                .long("wayland-app-id")
                .takes_value(true)
                .help("Specify an App ID for Wayland"),
        )
        .arg(
            Arg::new("x11_wm_class")
                .long("x11-wm-class")
                .takes_value(true)
                .help("Specify an X11 WM class"),
        );

    let matches = clapp.get_matches_from(args);
    let mut neovim_args: Vec<String> = matches
        .values_of("neovim_args")
        .map(|opt| opt.map(|v| v.to_owned()).collect())
        .unwrap_or_default();

    let files_to_open: Vec<String> = matches
        .values_of("files_to_open")
        .map(|opt| opt.map(String::from).collect())
        .unwrap_or_default();

    if files_to_open.len() > 1
        && !neovim_args.contains(&String::from("-p"))
        && !matches.is_present("no_tabs")
    {
        neovim_args.push("-p".to_owned());
    }

    if cfg!(target_os = "macos") {
        // escape filepath which contain spaces
        neovim_args.extend(
            files_to_open
                .iter()
                .map(|file| shlex::quote(file).into_owned()),
        );
    } else {
        neovim_args.extend::<Vec<String>>(files_to_open);
    }

    /*
     * Integrate Environment Variables as Defaults to the command-line ones.
     *
     * If the command-line argument is not set, the environment variable is used.
     */
    SETTINGS.set::<CmdLineSettings>(&CmdLineSettings {
        // Pass through arguments
        neovim_args,
        // Command-line arguments only
        geometry: parse_window_geometry(matches.value_of("geometry").map(|i| i.to_owned()))?,
        log_to_file: matches.is_present("log_to_file"),
        no_fork: matches.is_present("nofork"),
        remote_tcp: matches.value_of("remote_tcp").map(|i| i.to_owned()),
        wsl: matches.is_present("wsl"),
        // Command-line flags with environment variable fallback
        frame: match matches.value_of("frame") {
            Some(val) => Frame::from_string(val.to_string()),
            None => match std::env::var("NEOVIDE_FRAME") {
                Ok(f) => Frame::from_string(f),
                Err(_) => Frame::Full,
            },
        },
        maximized: matches.is_present("maximized") || std::env::var("NEOVIDE_MAXIMIZED").is_ok(),
        multi_grid: matches.is_present("multi_grid") || std::env::var("NEOVIDE_MULTIGRID").is_ok(),
        no_idle: matches.is_present("noidle") || std::env::var("NEOVIDE_NO_IDLE").is_ok(),
        // Srgb is enabled by default, so set it to false if nosrgb or NOEVIDE_NO_SRGB is set
        srgb: !(matches.is_present("nosrgb") || std::env::var("NEOVIDE_NO_SRGB").is_ok()),
        // Command-line arguments with environment variable fallback
        neovim_bin: matches
            .value_of("neovim_bin")
            .map(|v| v.to_owned())
            .or_else(|| std::env::var("NEOVIM_BIN").ok()),
        wayland_app_id: matches
            .value_of("wayland_app_id")
            .map(|v| v.to_owned())
            .or_else(|| std::env::var("NEOVIDE_APP_ID").ok())
            .unwrap_or_else(|| "neovide".to_owned()),
        x11_wm_class: matches
            .value_of("x11_wm_class")
            .map(|v| v.to_owned())
            .or_else(|| std::env::var("NEOVIDE_X11_WM_CLASS").ok())
            .unwrap_or_else(|| "neovide".to_owned()),
        // Disable open multiple files as tabs
        no_tabs: matches.is_present("no_tabs") || std::env::var("NEOVIDE_NO_TABS").is_ok(),
    });
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
        let args: Vec<String> = vec!["neovide", "--", "--clean"]
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
        let args: Vec<String> = vec!["neovide", "./foo.txt", "./bar.md"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let _accessing_settings = ACCESSING_SETTINGS.lock().unwrap();
        handle_command_line_arguments(args).expect("Could not parse arguments");
        assert_eq!(
            SETTINGS.get::<CmdLineSettings>().neovim_args,
            vec!["-p", "./foo.txt", "./bar.md"]
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
            vec!["--clean", "./foo.txt", "./bar.md"]
        );
    }

    #[test]
    fn test_files_to_open_with_flag() {
        let args: Vec<String> = vec![
            "neovide",
            "--notabs",
            "./foo.txt",
            "./bar.md",
            "--geometry=42x24",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let _accessing_settings = ACCESSING_SETTINGS.lock().unwrap();
        handle_command_line_arguments(args).expect("Could not parse arguments");
        assert_eq!(
            SETTINGS.get::<CmdLineSettings>().neovim_args,
            vec!["./foo.txt", "./bar.md"]
        );

        assert_eq!(
            SETTINGS.get::<CmdLineSettings>().geometry,
            Dimensions {
                width: 42,
                height: 24
            }
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
            Dimensions {
                width: 42,
                height: 24
            }
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
