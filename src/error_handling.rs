use std::{
    io::{IsTerminal, stdout},
    process::ExitCode,
    sync::{Arc, Mutex},
};

use anyhow::{Error, Result};
use clap::error::Error as ClapError;
use itertools::Itertools;
use log::error;
use winit::event_loop::EventLoop;

#[cfg(target_os = "windows")]
use crate::windows_attach_to_console;

use crate::{
    bridge::{ParallelCommand, require_active_handler, send_ui},
    clipboard::Clipboard,
    settings::Settings,
    window::{EventPayload, show_error_window},
};

pub enum StartupErrorOutput {
    Stderr,
    Logger,
}

fn show_error(explanation: &str) -> ! {
    error!("{explanation}");
    panic!("{}", explanation.to_string());
}

pub fn show_nvim_error(msg: &str) {
    let handler = require_active_handler();
    send_ui(
        ParallelCommand::ShowError { lines: msg.split('\n').map(|s| s.to_string()).collect_vec() },
        &handler,
    );
}

/// Formats, logs and displays the given message.
#[macro_export]
macro_rules! error_msg {
    ($($arg:tt)+) => {
        let msg = format!($($arg)+);
        log::error!("{}", msg);
        $crate::error_handling::show_nvim_error(&msg);
    }
}

pub trait ResultPanicExplanation<T, E: ToString> {
    fn unwrap_or_explained_panic(self, explanation: &str) -> T;
}

impl<T, E: ToString> ResultPanicExplanation<T, E> for Result<T, E> {
    fn unwrap_or_explained_panic(self, explanation: &str) -> T {
        match self {
            Err(error) => {
                let explanation = format!("{}: {}", explanation, error.to_string());
                show_error(&explanation);
            }
            Ok(content) => content,
        }
    }
}

fn format_and_log_error_message(err: Error) -> String {
    let msg = format!("\
Neovide just crashed :(
This is the error that caused the crash. In case you don't know what to do with this, please feel free to report this on https://github.com/neovide/neovide/issues!

{err:?}"
    );
    msg
}

fn handle_clap_startup_error(err: &Error) -> Option<ExitCode> {
    let clap_error = err.downcast_ref::<ClapError>()?;
    #[cfg(target_os = "windows")]
    windows_attach_to_console();
    let _ = clap_error.print();
    Some(ExitCode::from(clap_error.exit_code() as u8))
}

pub fn report_startup_error(err: Error, output: StartupErrorOutput) -> ExitCode {
    if let Some(exit_code) = handle_clap_startup_error(&err) {
        return exit_code;
    }

    let msg = format_and_log_error_message(err);
    match output {
        StartupErrorOutput::Stderr => eprintln!("{msg}"),
        StartupErrorOutput::Logger => log::error!("{msg}"),
    }

    ExitCode::from(1)
}

pub fn handle_startup_errors(
    err: Error,
    event_loop: EventLoop<EventPayload>,
    settings: Arc<Settings>,
    clipboard: Arc<Mutex<Clipboard>>,
) -> ExitCode {
    // Command line output is always printed to the stdout/stderr
    if let Some(exit_code) = handle_clap_startup_error(&err) {
        exit_code
    } else if stdout().is_terminal() {
        // The logger already writes to stderr
        report_startup_error(err, StartupErrorOutput::Logger)
    } else {
        show_error_window(&format_and_log_error_message(err), event_loop, settings, clipboard);
        ExitCode::from(1)
    }
}
