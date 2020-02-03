use log::error;

fn show_error(explanation: &str) -> ! {
    error!("{}", explanation);
    panic!(explanation.to_string());
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
            },
            Ok(content) => content
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
            },
            Some(content) => content
        }
    }
}
