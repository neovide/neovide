use log::error;

fn show_error(explanation: &str) -> ! {
    error!("{}", explanation);
    panic!("{}", explanation.to_string());
}

/// Formats, logs and displays the given message.
#[macro_export]
macro_rules! error_msg {
    ($($arg:tt)+) => {
        let msg = format!($($arg)+);
        log::error!("{}", msg);
        EVENT_AGGREGATOR.send(UiCommand::Parallel(ParallelCommand::ShowError {
            lines: msg.split('\n').map(|s| s.to_string()).collect_vec(),
        }));
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
