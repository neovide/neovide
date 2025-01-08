use std::error::Error;
use std::sync::OnceLock;

#[cfg(target_os = "linux")]
use copypasta::{
    wayland_clipboard,
    x11_clipboard::{Primary as X11SelectionClipboard, X11ClipboardContext},
};
use copypasta::{ClipboardContext, ClipboardProvider};
use parking_lot::Mutex;
use raw_window_handle::HasDisplayHandle;
#[cfg(target_os = "linux")]
use raw_window_handle::{RawDisplayHandle, WaylandDisplayHandle};
use winit::event_loop::EventLoop;

use crate::window::EventPayload;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync + 'static>>;

pub struct Clipboard {
    clipboard: Box<dyn ClipboardProvider>,
    #[cfg(target_os = "linux")]
    selection: Box<dyn ClipboardProvider>,
}

static CLIPBOARD: OnceLock<Mutex<Clipboard>> = OnceLock::new();

pub fn init(event_loop: &EventLoop<EventPayload>) {
    CLIPBOARD
        .set(Mutex::new(
            match event_loop.display_handle().unwrap().as_raw() {
                #[cfg(target_os = "linux")]
                RawDisplayHandle::Wayland(WaylandDisplayHandle { mut display, .. }) => unsafe {
                    let (selection, clipboard) =
                        wayland_clipboard::create_clipboards_from_external(display.as_mut());
                    Clipboard {
                        clipboard: Box::new(clipboard),
                        selection: Box::new(selection),
                    }
                },
                #[cfg(target_os = "linux")]
                _ => Clipboard {
                    clipboard: Box::new(ClipboardContext::new().unwrap()),
                    selection: Box::new(
                        X11ClipboardContext::<X11SelectionClipboard>::new().unwrap(),
                    ),
                },
                #[cfg(not(target_os = "linux"))]
                _ => Clipboard {
                    clipboard: Box::new(ClipboardContext::new().unwrap()),
                },
            },
        ))
        .ok();
}

pub fn get_contents(register: &str) -> Result<String> {
    match register {
        #[cfg(target_os = "linux")]
        "*" => CLIPBOARD.get().unwrap().lock().selection.get_contents(),
        _ => CLIPBOARD.get().unwrap().lock().clipboard.get_contents(),
    }
}

pub fn set_contents(lines: String, register: &str) -> Result<()> {
    match register {
        #[cfg(target_os = "linux")]
        "*" => CLIPBOARD
            .get()
            .unwrap()
            .lock()
            .selection
            .set_contents(lines),
        _ => CLIPBOARD
            .get()
            .unwrap()
            .lock()
            .clipboard
            .set_contents(lines),
    }
}
