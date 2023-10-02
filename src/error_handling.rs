use anyhow::Error;
use clap::error::Error as ClapError;
use log::error;
use std::io::{stdout, IsTerminal};
use winit::event_loop::EventLoop;

use crate::window::{show_error_window, UserEvent};

fn show_error(explanation: &str) -> ! {
    error!("{}", explanation);
    panic!("{}", explanation.to_string());
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

fn handle_terminal_startup_errors(err: Error) -> i32 {
    if let Some(clap_error) = err.downcast_ref::<ClapError>() {
        let _ = clap_error.print();
        clap_error.exit_code()
    } else {
        eprintln!("ERROR: {}", err);
        1
    }
}

fn handle_gui_startup_errors(err: Error, event_loop: EventLoop<UserEvent>) -> i32 {
    if let Some(clap_error) = err.downcast_ref::<ClapError>() {
        let text = clap_error.render().to_string();
        show_error_window(&text, event_loop);
        clap_error.exit_code()
    } else {
        eprintln!("ERROR: {}", err);
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
