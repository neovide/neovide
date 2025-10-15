use std::error::Error;

#[cfg(target_os = "linux")]
use copypasta::{
    wayland_clipboard,
    x11_clipboard::{Primary as X11SelectionClipboard, X11ClipboardContext},
};
use copypasta::{ClipboardContext, ClipboardProvider};
use raw_window_handle::HasDisplayHandle;
#[cfg(target_os = "linux")]
use raw_window_handle::{RawDisplayHandle, WaylandDisplayHandle};
use winit::event_loop::EventLoop;

use crate::window::UserEvent;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync + 'static>>;

pub struct Clipboard {
    clipboard: Box<dyn ClipboardProvider>,
    #[cfg(target_os = "linux")]
    selection: Box<dyn ClipboardProvider>,
}

impl Clipboard {
    pub fn new(event_loop: &EventLoop<UserEvent>) -> Self {
        match event_loop.display_handle().unwrap().as_raw() {
            #[cfg(target_os = "linux")]
            RawDisplayHandle::Wayland(WaylandDisplayHandle { mut display, .. }) => unsafe {
                let (selection, clipboard) =
                    wayland_clipboard::create_clipboards_from_external(display.as_mut());
                Self {
                    clipboard: Box::new(clipboard),
                    selection: Box::new(selection),
                }
            },
            #[cfg(target_os = "linux")]
            _ => Self {
                clipboard: Box::new(ClipboardContext::new().unwrap()),
                selection: Box::new(X11ClipboardContext::<X11SelectionClipboard>::new().unwrap()),
            },
            #[cfg(not(target_os = "linux"))]
            _ => Self {
                clipboard: Box::new(ClipboardContext::new().unwrap()),
            },
        }
    }

    pub fn get_contents(&mut self, register: &str) -> Result<String> {
        match register {
            #[cfg(target_os = "linux")]
            "*" => self.selection.get_contents(),
            _ => self.clipboard.get_contents(),
        }
    }

    pub fn set_contents(&mut self, lines: String, register: &str) -> Result<()> {
        match register {
            #[cfg(target_os = "linux")]
            "*" => self.selection.set_contents(lines),
            _ => self.clipboard.set_contents(lines),
        }
    }
}
