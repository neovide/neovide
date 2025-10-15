use anyhow::{Error, Result};
use itertools::Itertools;
use log::error;

use crate::bridge::{send_ui, ParallelCommand};

fn show_error(explanation: &str) -> ! {
    error!("{explanation}");
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

pub fn format_and_log_error_message(err: Error) -> String {
    let msg = format!("\
Neovide just crashed :(
This is the error that caused the crash. In case you don't know what to do with this, please feel free to report this on https://github.com/neovide/neovide/issues!

{err:?}"
    );
    msg
}
