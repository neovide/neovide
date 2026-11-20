use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::Mutex;

use objc2::runtime::AnyObject;
use objc2_app_kit::NSWindow;

struct WindowRegistration {
    title_hidden: bool,
    toolbar_enabled: bool,
    translucent_background: bool,
    pending_content_height: Option<f64>,
}

// Each entry keeps the configured title preference and suspends the toolbar
// during simple fullscreen. A new full frame tab keeps the host content height
// until its tab view attaches. A missing entry keeps callbacks from changing it.
static WINDOWS: LazyLock<Mutex<HashMap<usize, WindowRegistration>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// The owner passes `NSWindow` and AppKit callbacks pass `AnyObject`, but both
/// refer to one `WinitWindow`, so their addresses select the same map entry.
fn key<T>(object: &T) -> usize { object as *const T as usize }

pub fn register(window: &NSWindow, title_hidden: bool, pending_content_height: Option<f64>) {
    let previous = WINDOWS.lock().unwrap().insert(
        key(window),
        WindowRegistration {
            title_hidden,
            toolbar_enabled: true,
            translucent_background: false,
            pending_content_height,
        },
    );
    assert!(previous.is_none(), "inline system tabs already registered for this window");
}

pub fn unregister(window: &NSWindow) { WINDOWS.lock().unwrap().remove(&key(window)); }

pub fn is_registered(window: &AnyObject) -> bool {
    WINDOWS.lock().unwrap().contains_key(&key(window))
}

pub fn title_hidden(window: &NSWindow) -> Option<bool> {
    WINDOWS.lock().unwrap().get(&key(window)).map(|entry| entry.title_hidden)
}

pub fn set_title_hidden(window: &NSWindow, hidden: bool) {
    let mut windows = WINDOWS.lock().unwrap();
    windows.get_mut(&key(window)).expect("system tabs must be registered").title_hidden = hidden;
}

pub fn toolbar_enabled(window: &NSWindow) -> Option<bool> {
    WINDOWS.lock().unwrap().get(&key(window)).map(|entry| entry.toolbar_enabled)
}

pub fn set_toolbar_enabled(window: &NSWindow, enabled: bool) {
    let mut windows = WINDOWS.lock().unwrap();
    windows.get_mut(&key(window)).expect("system tabs must be registered").toolbar_enabled =
        enabled;
}

pub fn translucent_background(window: &NSWindow) -> Option<bool> {
    WINDOWS.lock().unwrap().get(&key(window)).map(|entry| entry.translucent_background)
}

pub fn set_translucent_background(window: &NSWindow, translucent: bool) -> bool {
    let mut windows = WINDOWS.lock().unwrap();
    let entry = windows.get_mut(&key(window)).expect("system tabs must be registered");
    std::mem::replace(&mut entry.translucent_background, translucent)
}

pub fn pending_content_height(window: &NSWindow) -> Option<f64> {
    WINDOWS.lock().unwrap().get(&key(window)).and_then(|entry| entry.pending_content_height)
}

pub fn clear_pending_content_height(window: &NSWindow) {
    if let Some(entry) = WINDOWS.lock().unwrap().get_mut(&key(window)) {
        entry.pending_content_height = None;
    }
}
