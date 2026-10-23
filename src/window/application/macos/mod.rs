pub mod delegate;
mod hotkey;
pub mod icon;
mod menu;
mod open_files;

use std::cell::RefCell;
use std::sync::OnceLock;

use objc2::msg_send;
use objc2::runtime::AnyObject;
use objc2_app_kit::NSApplication;
use objc2_app_kit::NSImage;
use objc2_foundation::MainThreadMarker;
use winit::event_loop::EventLoopProxy;
use winit::window::WindowId;

use self::hotkey::GlobalHotkeys;
use self::icon::load_neovide_icon;
use self::menu::Menu;
use crate::cmd_line::CmdLineSettings;
use crate::settings::Settings;
use crate::window::EventPayload;
use crate::window::MacShortcutCommand;
use crate::window::UserEvent;

thread_local! {
    static APP_MENU: RefCell<Option<Menu>> = const { RefCell::new(None) };
}

static EVENT_LOOP_PROXY: OnceLock<EventLoopProxy<EventPayload>> = OnceLock::new();

pub struct State {
    global_hotkeys: Option<GlobalHotkeys>,
}

impl State {
    pub fn new(proxy: &EventLoopProxy<EventPayload>) -> Self {
        store_event_loop_proxy(proxy.clone());
        Self { global_hotkeys: None }
    }

    pub fn resumed(&mut self, proxy: &EventLoopProxy<EventPayload>) {
        if self.global_hotkeys.is_none() {
            self.global_hotkeys = GlobalHotkeys::register(proxy.clone(), true);
        }
    }

    pub fn exiting(&mut self) { self.global_hotkeys.take(); }
}

fn store_event_loop_proxy(proxy: EventLoopProxy<EventPayload>) {
    let _ = EVENT_LOOP_PROXY.set(proxy);
}

fn request_new_window() {
    let Some(proxy) = EVENT_LOOP_PROXY.get() else {
        log::warn!("New window requested before event loop proxy became available");
        return;
    };

    if let Err(error) = proxy.send_event(EventPayload::all(UserEvent::CreateWindow)) {
        log::error!("Failed to request a new window: {:?}", error);
    }
}

fn request_editor_switcher() {
    let Some(proxy) = EVENT_LOOP_PROXY.get() else {
        log::warn!("Editor switcher requested before event loop proxy became available");
        return;
    };

    let event = UserEvent::MacShortcut(MacShortcutCommand::ShowEditorSwitcher);
    if let Err(error) = proxy.send_event(EventPayload::all(event)) {
        log::error!("Failed to request editor switcher: {:?}", error);
    }
}

pub(crate) fn request_activate_window(window_id: WindowId) {
    let Some(proxy) = EVENT_LOOP_PROXY.get() else {
        log::warn!("Window activation requested before event loop proxy became available");
        return;
    };

    if let Err(error) = proxy.send_event(EventPayload::all(UserEvent::ActivateWindow(window_id))) {
        log::error!("Failed to request window activation: {:?}", error);
    }
}

pub(crate) fn ensure_app_initialized(settings: &Settings) {
    let mtm = MainThreadMarker::new().expect("Menu must be created on the main thread");
    APP_MENU.with(|menu_cell| {
        if menu_cell.borrow().is_some() {
            return;
        }

        *menu_cell.borrow_mut() = Some(Menu::new(mtm, settings));
        let app = NSApplication::sharedApplication(mtm);
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);

        // Make sure the icon is loaded when launched from terminal
        let icon = load_neovide_icon(settings.get::<CmdLineSettings>().icon.as_ref());
        let icon_ref: Option<&NSImage> = icon.as_ref().map(|img| img.as_ref());
        unsafe { app.setApplicationIconImage(icon_ref) }
    });
}

pub(crate) fn hide_application() {
    match MainThreadMarker::new() {
        Some(mtm) => {
            let app = NSApplication::sharedApplication(mtm);
            let app_ref: &NSApplication = app.as_ref();
            unsafe {
                let _: () = msg_send![app_ref, hide: None::<&AnyObject>];
            }
        }
        None => {
            log::warn!(
                "macOS pinned shortcut attempted to hide application outside the main thread"
            );
        }
    }
}
