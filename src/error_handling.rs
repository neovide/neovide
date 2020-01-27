use msgbox::IconType;

fn show_error(title: &str, explanation: &str) -> ! {
    if cfg!(target_os = "linux") {
        panic!("{}: {}", title, explanation);
    } else {
        msgbox::create(title, explanation, IconType::Error);
        panic!(explanation.to_string());
    }
}

pub trait ResultPanicExplanation<T, E: ToString> {
    fn unwrap_or_explained_panic(self, title: &str, explanation: &str) -> T;
}

impl<T, E: ToString> ResultPanicExplanation<T, E> for Result<T, E> {
    fn unwrap_or_explained_panic(self, title: &str, explanation: &str) -> T {
        match self {
            Err(error) => {
                let explanation = format!("{}: {}", explanation, error.to_string());
                show_error(title, &explanation);
            },
            Ok(content) => content
        }
    }
}

pub trait OptionPanicExplanation<T> {
    fn unwrap_or_explained_panic(self, title: &str, explanation: &str) -> T;
}

impl<T> OptionPanicExplanation<T> for Option<T> {
    fn unwrap_or_explained_panic(self, title: &str, explanation: &str) -> T {
        match self {
            None => {
                show_error(title, explanation);
            },
            Some(content) => content
        }
    }
}
