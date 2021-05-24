use crate::settings::*;

#[derive(Clone)]
pub struct CmdLineSettings {
    pub verbosity: u64,
    pub log_to_file: bool,
    pub neovim_args: Vec<String>,
    pub files_to_open: Vec<String>,

    pub disowned: bool,
    pub geometry: Option<String>,
    pub wsl: bool,
    pub remote_tcp: Option<String>,
    pub multi_grid: bool,
    pub maximized: bool,
}

impl Default for CmdLineSettings {
    fn default() -> Self {
        Self {
            verbosity: 0,
            log_to_file: false,
            neovim_args: vec![],
            files_to_open: vec![],
            disowned: false,
            geometry: None,
            wsl: false,
            remote_tcp: None,
            multi_grid: false,
            maximized: false,
        }
    }
}

pub fn handle_command_line_arguments() {
    let clapp = clap_app!(neovide =>
    (author: crate_authors!())
    (version: crate_version!())
    (about: crate_description!())
    (@arg verbosity: -v ... "Set the level of output information")
    (@arg log_to_file: --log "Log to a file")
    (@arg disowned: --disowned "Disown the process. (only on macos)")
    (@arg maximized: --maximized "Maximize the window.")
    (@arg multi_grid: --multi-grid "Enable Multigrid")
    (@arg wsl: --wsl "Run in WSL")
    (@arg remote_tcp: --remote-tcp +takes_value "Connect to Remote TCP")
    (@arg geometry: --geometry +takes_value "Specify the Geometry of the window")

    (@arg files: +takes_value +multiple "Open Files")
    (@arg neovim_args: +takes_value +multiple +last "Args to pass to Neovim")
                        );
    let matches = clapp.get_matches();

    SETTINGS.set::<CmdLineSettings>(&CmdLineSettings {
        neovim_args: matches
            .values_of("neovim_args")
            .map(|opt| opt.map(|v| v.to_owned()).collect())
            .unwrap_or_default(),
        verbosity: matches.occurrences_of("verbosity"),
        log_to_file: matches.is_present("log_to_file"),
        files_to_open: matches
            .values_of("files")
            .map(|opt| opt.map(|v| v.to_owned()).collect())
            .unwrap_or_default(),
        maximized: matches.is_present("maximized"),
        multi_grid: matches.is_present("multi_grid"),
        remote_tcp: matches.value_of("remote_tcp").map(|i| i.to_owned()),
        disowned: matches.is_present("disowned"),
        wsl: matches.is_present("wsl"),
        geometry: matches.value_of("geometry").map(|i| i.to_owned()),
    });
}
