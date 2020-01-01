use msgbox::IconType;

pub trait ResultPanicExplanation<T, E: ToString> {
    fn unwrap_or_explained_panic(self, title: &str, explanation: &str) -> T;
}

impl<T, E: ToString> ResultPanicExplanation<T, E> for Result<T, E>  {
    fn unwrap_or_explained_panic(self, title: &str, explanation: &str) -> T {
        match self {
            Err(error) => {
                let explanation = format!("{}: {}", explanation, error.to_string());
                msgbox::create(title, &explanation, IconType::Error);
                panic!(explanation);
            },
            Ok(content) => content
        }
    }
}
