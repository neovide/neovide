use crate::settings::*;

use clap::{App, Arg};

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
    let clapp = App::new("Neovide")
        .version(crate_version!())
        .author(crate_authors!())
        .about(crate_description!())
        .arg(
            Arg::with_name("verbosity")
                .short("v")
                .multiple(true)
                .help("Set the level of verbosity"),
        )
        .arg(
            Arg::with_name("log_to_file")
                .long("log")
                .help("Log to a file"),
        )
        .arg(
            Arg::with_name("disowned")
                .long("disowned")
                .help("Disown the process. (only on macos)"),
        )
        .arg(
            Arg::with_name("maximized")
                .long("maximized")
                .help("Maximize the window"),
        )
        .arg(
            Arg::with_name("multi_grid")
                //.long("multi-grid") TODO: multiGrid is the current way to call this, but I
                //personally would prefer sticking to a unix-y way of naming things...
                .long("multiGrid")
                .help("Enable Multigrid"),
        )
        .arg(Arg::with_name("wsl").long("wsl").help("Run in WSL"))
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
                .help("Specify the Geometry of the window"),
        )
        .arg(
            Arg::with_name("neovim_args")
                .multiple(true)
                .takes_value(true)
                .last(true)
                .help("Specify Arguments to pass down to neovim"),
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
