use std::{
    io::{stdout, IsTerminal},
    process::{ExitCode, Termination},
};

use anyhow::{Error, Result};
use clap::error::Error as ClapError;
use itertools::Itertools;
use log::error;
use winit::{error::EventLoopError, event_loop::EventLoop};

use crate::{
    bridge::{send_ui, ParallelCommand},
    running_tracker::RUNNING_TRACKER,
    window::{show_error_window, UserEvent},
};

fn show_error(explanation: &str) -> ! {
    error!("{}", explanation);
    panic!("{}", explanation.to_string());
}

pub fn show_nvim_error(msg: &str) {
    send_ui(ParallelCommand::ShowError {
        lines: msg.split('\n').map(|s| s.to_string()).collect_vec(),
    });
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

pub trait OptionPanicExplanation<T> {
    fn unwrap_or_explained_panic(self, explanation: &str) -> T;
}

impl<T> OptionPanicExplanation<T> for Option<T> {
    fn unwrap_or_explained_panic(self, explanation: &str) -> T {
        match self {
            None => {
                show_error(explanation);
            }
            Some(content) => content,
        }
    }
}

fn format_and_log_error_message(err: Error) -> String {
    let msg = format!("\
Neovide just crashed :(
This is the error that caused the crash. In case you don't know what to do with this, please feel free to report this on https://github.com/neovide/neovide/issues!

{:?}",
        err
    );
    log::error!("{}", msg);
    msg
}

fn handle_terminal_startup_errors(err: Error) -> i32 {
    if let Some(clap_error) = err.downcast_ref::<ClapError>() {
        let _ = clap_error.print();
        clap_error.exit_code()
    } else {
        eprintln!("{}", &format_and_log_error_message(err));
        1
    }
}

fn handle_gui_startup_errors(err: Error, event_loop: EventLoop<UserEvent>) -> i32 {
    if let Some(clap_error) = err.downcast_ref::<ClapError>() {
        let text = clap_error.render().to_string();
        show_error_window(&text, event_loop);
        clap_error.exit_code()
    } else {
        show_error_window(&format_and_log_error_message(err), event_loop);
        1
    }
}

pub fn handle_startup_errors(err: Error, event_loop: EventLoop<UserEvent>) -> i32 {
    if stdout().is_terminal() {
        handle_terminal_startup_errors(err)
    } else {
        handle_gui_startup_errors(err, event_loop)
    }
}

pub struct NeovideExitCode(ExitCode);

impl From<Result<(), EventLoopError>> for NeovideExitCode {
    fn from(res: Result<(), EventLoopError>) -> Self {
        match res {
            Ok(_) => RUNNING_TRACKER.exit_code().into(),
            Err(EventLoopError::ExitFailure(code)) => code.into(),
            _ => Self(ExitCode::FAILURE),
        }
    }
}

impl From<i32> for NeovideExitCode {
    fn from(res: i32) -> Self {
        // All error codes have to be u8, so just do a direct cast with wrap around, even if the value is negative,
        // that's how it's normally done on operating systems that don't support negative return values.
        Self((res as u8).into())
    }
}

impl Termination for NeovideExitCode {
    fn report(self) -> ExitCode {
        self.0
    }
}
