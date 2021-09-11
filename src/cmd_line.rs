use crate::settings::*;
use crate::utils::Dimensions;

use clap::{App, Arg};

#[derive(Clone, Debug)]
pub struct CmdLineSettings {
    pub verbosity: u64,
    pub log_to_file: bool,
    pub neovim_args: Vec<String>,
    pub neovim_bin: Option<String>,
    pub files_to_open: Vec<String>,

    pub no_fork: bool,
    pub no_idle: bool,
    pub srgb: bool,
    pub geometry: Dimensions,
    pub wsl: bool,
    pub remote_tcp: Option<String>,
    pub multi_grid: bool,
    pub maximized: bool,
    pub frameless: bool,
    pub wayland_app_id: String,
    pub x11_wm_class: String,
}

impl Default for CmdLineSettings {
    fn default() -> Self {
        Self {
            verbosity: 0,
            log_to_file: false,
            neovim_bin: None,
            neovim_args: vec![],
            files_to_open: vec![],

            no_fork: false,
            no_idle: false,
            srgb: true,
            geometry: DEFAULT_WINDOW_GEOMETRY,
            wsl: false,
            remote_tcp: None,
            multi_grid: false,
            maximized: false,
            frameless: false,
            wayland_app_id: String::new(),
            x11_wm_class: String::new(),
        }
    }
}

pub fn handle_command_line_arguments() -> Result<(), String> {
    let clapp = App::new("Neovide")
        .version(crate_version!())
        .author(crate_authors!())
        .about(crate_description!())
        .arg(
            Arg::with_name("verbosity")
                .short("v")
                .multiple(true)
                .help("Increase verbosity level (repeatable up to 4 times; implies --nofork)"),
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
            Arg::with_name("noidle")
                .long("noidle")
                .help("Render every frame. Takes more power and cpu time but possibly fixes animation issues"),
        )
        .arg(
            Arg::with_name("srgb")
                .long("srgb")
                .help("Use standard color space to initialize the window. Swapping this variable sometimes fixes issues on startup"),
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
            Arg::with_name("frameless")
            .long("frameless")
            .help("Removes the window frame. NOTE: Window might not be resizable after this setting is enabled.")
        )
        .arg(
            Arg::with_name("wsl")
                .long("wsl")
                .help("Run in WSL")
        )
        .arg(
            Arg::with_name("remote_tcp")
                .long("remote-tcp")
                .takes_value(true)
                .help("Connect to Remote TCP"),
        )
        .arg(
            Arg::with_name("geometry")
                .long("geometry")
                .takes_value(true)
                .help("Specify the Geometry of the window"),
        )
        .arg(
            Arg::with_name("files")
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

    let matches = clapp.get_matches();

    /*
     * Integrate Environment Variables as Defaults to the command-line ones.
     *
     * NEOVIM_BIN
     * NEOVIDE_MULTIGRID || --multigrid
     */
    SETTINGS.set::<CmdLineSettings>(&CmdLineSettings {
        verbosity: matches.occurrences_of("verbosity"),
        log_to_file: matches.is_present("log_to_file"),
        neovim_args: matches
            .values_of("neovim_args")
            .map(|opt| opt.map(|v| v.to_owned()).collect())
            .unwrap_or_default(),
        neovim_bin: std::env::var("NEOVIM_BIN").ok(),
        files_to_open: matches
            .values_of("files")
            .map(|opt| opt.map(|v| v.to_owned()).collect())
            .unwrap_or_default(),

        no_fork: matches.is_present("nofork") || matches.is_present("verbosity"),
        no_idle: matches.is_present("noidle") || std::env::var("NEOVIDE_NOIDLE").is_ok(),
        srgb: matches.is_present("srgb") || !std::env::var("NEOVIDE_NO_SRGB").is_ok(),
        geometry: parse_window_geometry(matches.value_of("geometry").map(|i| i.to_owned()))?,
        wsl: matches.is_present("wsl"),
        remote_tcp: matches.value_of("remote_tcp").map(|i| i.to_owned()),
        multi_grid: std::env::var("NEOVIDE_MULTIGRID").is_ok() || matches.is_present("multi_grid"),
        maximized: matches.is_present("maximized") || std::env::var("NEOVIDE_MAXIMIZED").is_ok(),
        frameless: matches.is_present("frameless") || std::env::var("NEOVIDE_FRAMELESS").is_ok(),
        wayland_app_id: match std::env::var("NEOVIDE_APP_ID") {
            Ok(val) => val,
            Err(_) => matches
                .value_of("wayland_app_id")
                .unwrap_or("neovide")
                .to_string(),
        },
        x11_wm_class: match std::env::var("NEOVIDE_WM_CLASS") {
            Ok(val) => val,
            Err(_) => matches
                .value_of("x11_wm_class")
                .unwrap_or("neovide")
                .to_string(),
        },
    });
    Ok(())
}
