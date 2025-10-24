use copypasta::{ClipboardContext, ClipboardProvider};
use parking_lot::Mutex;
use std::error::Error;
use std::sync::OnceLock;
use winit::event_loop::EventLoop;

use crate::window::UserEvent;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync + 'static>>;

pub struct Clipboard {
    clipboard: Box<dyn ClipboardProvider>,
}

static CLIPBOARD: OnceLock<Mutex<Clipboard>> = OnceLock::new();

pub fn init(_event_loop: &EventLoop<UserEvent>) {
    CLIPBOARD
        .set(Mutex::new(Clipboard {
            clipboard: Box::new(ClipboardContext::new().unwrap()),
        }))
        .ok();
}

pub fn get_contents(_register: &str) -> Result<String> {
    CLIPBOARD.get().unwrap().lock().clipboard.get_contents()
}

pub fn set_contents(lines: String, _register: &str) -> Result<()> {
    CLIPBOARD
        .get()
        .unwrap()
        .lock()
        .clipboard
        .set_contents(lines)
}
