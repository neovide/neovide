use std::error::Error;
use std::sync::OnceLock;

#[cfg(target_os = "linux")]
use copypasta::wayland_clipboard;
use copypasta::{ClipboardContext, ClipboardProvider};
use parking_lot::Mutex;
use raw_window_handle::HasRawDisplayHandle;
#[cfg(target_os = "linux")]
use raw_window_handle::{RawDisplayHandle, WaylandDisplayHandle};
use winit::event_loop::EventLoop;

use crate::window::UserEvent;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync + 'static>>;

static CLIPBOARD: OnceLock<Mutex<Box<dyn ClipboardProvider>>> = OnceLock::new();

pub fn init(event_loop: &EventLoop<UserEvent>) {
    CLIPBOARD
        .set(Mutex::new(match event_loop.raw_display_handle() {
            #[cfg(target_os = "linux")]
            RawDisplayHandle::Wayland(WaylandDisplayHandle { display, .. }) => unsafe {
                Box::new(wayland_clipboard::create_clipboards_from_external(display).1)
            },
            _ => Box::new(ClipboardContext::new().unwrap()),
        }))
        .ok();
}

pub fn get_contents() -> Result<String> {
    CLIPBOARD.get().unwrap().lock().get_contents()
}

pub fn set_contents(lines: String) -> Result<()> {
    CLIPBOARD.get().unwrap().lock().set_contents(lines)
}
