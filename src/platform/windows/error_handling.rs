use std::{
    io::{stdout, IsTerminal},
    process::ExitCode,
    sync::Arc,
};

use anyhow::Error;
use clap::error::Error as ClapError;
use winit::event_loop::EventLoop;

use crate::{
    platform::windows::windows_attach_to_console,
    settings::Settings,
    window::{show_error_window, UserEvent},
};

fn format_and_log_error_message(err: Error) -> String {
    let msg = format!("
Neovide just crashed :(
This is the error that caused the crash. In case you don't know what to do with this, please feel free to report this on https://github.com/neovide/neovide/issues!

{err:?}"
    );
    msg
}

pub fn handle_startup_errors(
    err: Error,
    event_loop: EventLoop<UserEvent>,
    settings: Arc<Settings>,
) -> ExitCode {
    // Command line output is always printed to the stdout/stderr
    if let Some(clap_error) = err.downcast_ref::<ClapError>() {
        windows_attach_to_console();
        let _ = clap_error.print();
        ExitCode::from(clap_error.exit_code() as u8)
    } else if stdout().is_terminal() {
        // The logger already writes to stderr
        log::error!("{}", &format_and_log_error_message(err));
        ExitCode::from(1)
    } else {
        show_error_window(&format_and_log_error_message(err), event_loop, settings);
        ExitCode::from(1)
    }
}
