use copypasta::{ClipboardContext, ClipboardProvider};

use parking_lot::Mutex;

use std::error::Error;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync + 'static>>;

lazy_static! {
    static ref CLIPBOARD_CONTEXT: Mutex<ClipboardContext> =
        Mutex::new(ClipboardContext::new().unwrap());
}

pub fn get_contents() -> Result<String> {
    CLIPBOARD_CONTEXT.lock().get_contents()
}

pub fn set_contents(lines: String) -> Result<()> {
    CLIPBOARD_CONTEXT.lock().set_contents(lines)
}
