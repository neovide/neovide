use crate::settings::*;
use crate::utils::Dimensions;

use clap::{App, Arg};

#[derive(Clone, Debug)]
pub struct CmdLineSettings {
    // Pass through arguments
    pub neovim_args: Vec<String>,
    // Command-line arguments only
    pub geometry: Dimensions,
    pub verbosity: u64,
    pub log_to_file: bool,
    pub no_fork: bool,
    pub remote_tcp: Option<String>,
    pub wsl: bool,
    // Command-line flags with environment variable fallback
    pub frameless: bool,
    pub maximized: bool,
    pub multi_grid: bool,
    pub no_idle: bool,
    pub srgb: bool,
    // Command-line arguments with environment variable fallback
    pub neovim_bin: Option<String>,
    pub wayland_app_id: String,
    pub x11_wm_class: String,
}

impl Default for CmdLineSettings {
    fn default() -> Self {
        Self {
            // Pass through arguments
            neovim_args: vec![],
            // Command-line arguments only
            geometry: DEFAULT_WINDOW_GEOMETRY,
            verbosity: 0,
            log_to_file: false,
            no_fork: false,
            remote_tcp: None,
            wsl: false,
            // Command-line flags with environment variable fallback
            frameless: false,
            maximized: false,
            multi_grid: false,
            no_idle: false,
            srgb: true,
            // Command-line arguments with environment variable fallback
            neovim_bin: None,
            wayland_app_id: String::new(),
            x11_wm_class: String::new(),
        }
    }
}

pub fn handle_command_line_arguments(args: Vec<String>) -> Result<(), String> {
    let clapp = App::new("Neovide")
        .version(crate_version!())
        .author(crate_authors!())
        .about(crate_description!())
        // Pass through arguments
        .arg(
            Arg::with_name("files_to_open")
                .multiple(true)
                .takes_value(true)
                .help("Files to open"),
        )
        .arg(
            Arg::with_name("neovim_args")
                .multiple(true)
                .takes_value(true)
                .last(true)
                .help("Specify Arguments to pass down to neovim"),
        )
        // Command-line arguments only
        .arg(
            Arg::with_name("verbosity")
                .short("v")
                .multiple(true)
                .help("Increase verbosity level (repeatable up to 4 times; implies --nofork)"),
        )
        .arg(
            Arg::with_name("geometry")
                .long("geometry")
                .takes_value(true)
                .help("Specify the Geometry of the window"),
        )
        .arg(
            Arg::with_name("log_to_file")
                .long("log")
                .help("Log to a file"),
        )
        .arg(
            Arg::with_name("nofork")
                .long("nofork")
                .help("Do not detach process from terminal"),
        )
        .arg(
            Arg::with_name("remote_tcp")
                .long("remote-tcp")
                .takes_value(true)
                .help("Connect to Remote TCP"),
        )
        .arg(
            Arg::with_name("wsl")
                .long("wsl")
                .help("Run in WSL")
        )
        // Command-line flags with environment variable fallback
        .arg(
            Arg::with_name("frameless")
            .long("frameless")
            .help("Removes the window frame. NOTE: Window might not be resizable after this setting is enabled.")
        )
        .arg(
            Arg::with_name("maximized")
                .long("maximized")
                .help("Maximize the window"),
        )
        .arg(
            Arg::with_name("multi_grid")
                .long("multigrid")
                .help("Enable Multigrid"),
        )
        .arg(
            Arg::with_name("noidle")
                .long("noidle")
                .help("Render every frame. Takes more power and cpu time but possibly fixes animation issues"),
        )
        .arg(
            Arg::with_name("srgb")
                .long("srgb")
                .help("Use standard color space to initialize the window. Swapping this variable sometimes fixes issues on startup"),
        )
        // Command-line arguments with environment variable fallback
        .arg(
            Arg::with_name("neovim_bin")
                .long("neovim-bin")
                .takes_value(true)
                .help("Specify path to neovim"),
        )
        .arg(
            Arg::with_name("wayland_app_id")
                .long("wayland-app-id")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("x11_wm_class")
                .long("x11-wm-class")
                .takes_value(true)
        );

    let matches = clapp.get_matches_from(args);
    let mut neovim_args: Vec<String> = matches
        .values_of("neovim_args")
        .map(|opt| opt.map(|v| v.to_owned()).collect())
        .unwrap_or_default();
    neovim_args.extend::<Vec<String>>(
        matches
            .values_of("files_to_open")
            .map(|opt| opt.map(|v| v.to_owned()).collect())
            .unwrap_or_default(),
    );

    /*
     * Integrate Environment Variables as Defaults to the command-line ones.
     *
     * If the command-line argument is not set, the environment variable is used.
     */
    SETTINGS.set::<CmdLineSettings>(&CmdLineSettings {
        // Pass through arguments
        neovim_args: neovim_args,
        // Command-line arguments only
        verbosity: matches.occurrences_of("verbosity"),
        geometry: parse_window_geometry(matches.value_of("geometry").map(|i| i.to_owned()))?,
        log_to_file: matches.is_present("log_to_file"),
        no_fork: matches.is_present("nofork") || matches.is_present("verbosity"),
        remote_tcp: matches.value_of("remote_tcp").map(|i| i.to_owned()),
        wsl: matches.is_present("wsl"),
        // Command-line flags with environment variable fallback
        frameless: matches.is_present("frameless") || std::env::var("NEOVIDE_FRAMELESS").is_ok(),
        maximized: matches.is_present("maximized") || std::env::var("NEOVIDE_MAXIMIZED").is_ok(),
        multi_grid: matches.is_present("multi_grid") || std::env::var("NEOVIDE_MULTIGRID").is_ok(),
        no_idle: matches.is_present("noidle") || std::env::var("NEOVIDE_NOIDLE").is_ok(),
        srgb: matches.is_present("srgb") || std::env::var("NEOVIDE_NO_SRGB").is_err(),
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
        assert_eq!(SETTINGS.get::<CmdLineSettings>().log_to_file, true);
    }

    #[test]
    fn test_frameless_flag() {
        let args: Vec<String> = vec!["neovide", "--frameless"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let _accessing_settings = ACCESSING_SETTINGS.lock().unwrap();
        handle_command_line_arguments(args).expect("Could not parse arguments");
        assert_eq!(SETTINGS.get::<CmdLineSettings>().frameless, true);
    }

    #[test]
    fn test_frameless_environment_variable() {
        let args: Vec<String> = vec!["neovide"].iter().map(|s| s.to_string()).collect();

        let _accessing_settings = ACCESSING_SETTINGS.lock().unwrap();
        set_var("NEOVIDE_FRAMELESS", "true");
        handle_command_line_arguments(args).expect("Could not parse arguments");
        assert_eq!(SETTINGS.get::<CmdLineSettings>().frameless, true);
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
