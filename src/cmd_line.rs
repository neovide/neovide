use crate::settings::*;

#[derive(Clone)]
pub struct CmdLineSettings {
    pub verbosity: u64,
    pub log_to_file: bool,
    pub neovim_args: Vec<String>,
}

impl Default for CmdLineSettings {
    fn default() -> Self {
        Self {
            verbosity: 0,
            log_to_file: false,
            neovim_args: vec![],
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
    });
}
