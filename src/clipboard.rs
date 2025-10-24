#[cfg(target_os = "linux")]
use crate::platform::linux;
use std::error::Error;

#[cfg(not(target_os = "linux"))]
use copypasta::ClipboardProvider;

use winit::event_loop::EventLoop;

#[cfg(not(target_os = "linux"))]
use {copypasta::ClipboardContext, parking_lot::Mutex, std::sync::OnceLock};

use crate::window::UserEvent;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync + 'static>>;

#[cfg(not(target_os = "linux"))]
pub struct Clipboard {
    clipboard: Box<dyn ClipboardProvider>,
}

#[cfg(not(target_os = "linux"))]
static CLIPBOARD: OnceLock<Mutex<Clipboard>> = OnceLock::new();

#[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
pub fn init(event_loop: &EventLoop<UserEvent>) {
    #[cfg(target_os = "linux")]
    linux::clipboard::init(event_loop);

    #[cfg(not(target_os = "linux"))]
    CLIPBOARD
        .set(Mutex::new(Clipboard {
            clipboard: Box::new(ClipboardContext::new().unwrap()),
        }))
        .ok();
}

#[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
pub fn get_contents(register: &str) -> Result<String> {
    #[cfg(target_os = "linux")]
    return linux::clipboard::get_contents(register);

    #[cfg(not(target_os = "linux"))]
    CLIPBOARD.get().unwrap().lock().clipboard.get_contents()
}

#[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
pub fn set_contents(lines: String, register: &str) -> Result<()> {
    #[cfg(target_os = "linux")]
    return linux::clipboard::set_contents(lines, register);

    #[cfg(not(target_os = "linux"))]
    CLIPBOARD
        .get()
        .unwrap()
        .lock()
        .clipboard
        .set_contents(lines)
}
