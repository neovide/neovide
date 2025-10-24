use std::error::Error;

use copypasta::{
    wayland_clipboard,
    x11_clipboard::{Primary as X11SelectionClipboard, X11ClipboardContext},
    ClipboardContext, ClipboardProvider,
};
use parking_lot::Mutex;
use raw_window_handle::{HasDisplayHandle, RawDisplayHandle, WaylandDisplayHandle};
use winit::event_loop::EventLoop;

use crate::window::UserEvent;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync + 'static>>;

pub struct Clipboard {
    clipboard: Box<dyn ClipboardProvider>,
    selection: Box<dyn ClipboardProvider>,
}

static CLIPBOARD: Mutex<Option<Clipboard>> = Mutex::new(None);

pub fn init(event_loop: &EventLoop<UserEvent>) {
    let mut guard = CLIPBOARD.lock();
    *guard = Some(match event_loop.display_handle().unwrap().as_raw() {
        RawDisplayHandle::Wayland(WaylandDisplayHandle { mut display, .. }) => unsafe {
            let (selection, clipboard) =
                wayland_clipboard::create_clipboards_from_external(display.as_mut());
            Clipboard {
                clipboard: Box::new(clipboard),
                selection: Box::new(selection),
            }
        },
        _ => Clipboard {
            clipboard: Box::new(ClipboardContext::new().unwrap()),
            selection: Box::new(X11ClipboardContext::<X11SelectionClipboard>::new().unwrap()),
        },
    });
}

pub fn get_contents(register: &str) -> Result<String> {
    let mut guard = CLIPBOARD.lock();
    let clipboard = guard.as_mut().unwrap();
    match register {
        "*" => clipboard.selection.get_contents(),
        _ => clipboard.clipboard.get_contents(),
    }
}

pub fn set_contents(lines: String, register: &str) -> Result<()> {
    let mut guard = CLIPBOARD.lock();
    let clipboard = guard.as_mut().unwrap();
    match register {
        "*" => clipboard.selection.set_contents(lines),
        _ => clipboard.clipboard.set_contents(lines),
    }
}
