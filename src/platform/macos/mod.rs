pub mod hotkey;
pub mod settings;
use std::cell::Cell;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    OnceLock,
};
use std::{os::raw::c_void, ptr, str};

use objc2::{
    define_class, msg_send,
    rc::{autoreleasepool, Retained},
    runtime::{AnyClass, AnyObject, ClassBuilder, ProtocolObject},
    sel, AnyThread, MainThreadOnly, Message,
};

use std::sync::Arc;

use objc2_app_kit::{
    NSApplication, NSAutoresizingMaskOptions, NSColor, NSEvent, NSEventModifierFlags, NSImage,
    NSMenu, NSMenuDelegate, NSMenuItem, NSView, NSWindow, NSWindowDidBecomeKeyNotification,
    NSWindowStyleMask, NSWindowTabbingMode,
};
use objc2_foundation::{
    ns_string, MainThreadMarker, NSArray, NSData, NSDictionary, NSNotification,
    NSNotificationCenter, NSObject, NSObjectProtocol, NSPoint, NSProcessInfo, NSRect, NSSize,
    NSString, NSTimer, NSUserDefaults,
};

use csscolorparser::Color;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::{
    event_loop::EventLoopProxy,
    window::{Window, WindowId},
};

use crate::bridge::{send_ui, ParallelCommand, SerialCommand, HANDLER_REGISTRY};
use crate::settings::Settings;
use crate::{cmd_line::CmdLineSettings, error_msg, frame::Frame};

use crate::window::{EventPayload, UserEvent, WindowSettings, WindowSettingsChanged};

use self::hotkey::GlobalHotkeys;

thread_local! {
    static TAB_OVERVIEW_ACTIVE: Cell<bool> = Cell::new(false);
    static PENDING_DETACH_WINDOW: Cell<usize> = Cell::new(0);
    static SUPPRESS_FOCUS_EVENTS: Cell<bool> = Cell::new(false);
    static ACTIVE_HOST_WINDOW: Cell<usize> = Cell::new(0);
    static SUPPRESS_UNTIL_NEXT_KEY_EVENT: Cell<bool> = Cell::new(false);
    static LAST_HOST_WINDOW: Cell<usize> = Cell::new(0);
}

static SHOW_NATIVE_TAB_BAR: AtomicBool = AtomicBool::new(false);
static EVENT_LOOP_PROXY: OnceLock<EventLoopProxy<EventPayload>> = OnceLock::new();

fn should_show_native_tab_bar() -> bool {
    SHOW_NATIVE_TAB_BAR.load(Ordering::Relaxed)
}

fn store_event_loop_proxy(proxy: EventLoopProxy<EventPayload>) {
    let _ = EVENT_LOOP_PROXY.set(proxy);
}

fn request_new_window() {
    let Some(proxy) = EVENT_LOOP_PROXY.get() else {
        log::warn!("New window requested before event loop proxy became available");
        return;
    };

    if let Err(error) = proxy.send_event(EventPayload::new(
        UserEvent::CreateWindow,
        WindowId::from(0),
    )) {
        log::error!("Failed to request a new window: {:?}", error);
    }
}

pub fn native_tab_bar_enabled() -> bool {
    should_show_native_tab_bar()
}

fn merge_all_windows_if_native_tabs(ns_window: &NSWindow) {
    if should_show_native_tab_bar() {
        ns_window.mergeAllWindows(None);
    }
}

pub fn is_focus_suppressed() -> bool {
    SUPPRESS_FOCUS_EVENTS.with(|cell| cell.get())
        || SUPPRESS_UNTIL_NEXT_KEY_EVENT.with(|cell| cell.get())
}

struct FocusSuppressionGuard;

impl FocusSuppressionGuard {
    fn new() -> Self {
        SUPPRESS_FOCUS_EVENTS.with(|flag| flag.set(true));
        FocusSuppressionGuard
    }
}

impl Drop for FocusSuppressionGuard {
    fn drop(&mut self) {
        SUPPRESS_FOCUS_EVENTS.with(|flag| flag.set(false));
    }
}

static DEFAULT_NEOVIDE_ICON_BYTES: &[u8] =
    include_bytes!("../../../extra/osx/Neovide.app/Contents/Resources/Neovide.icns");
const NEOVIDE_TABBING_IDENTIFIER: &str = "NeovideWindowTabGroup";

define_class!(
    // A view to simulate the double-click-to-zoom effect for `--frame transparency`.
    #[derive(Debug)]
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    struct TitlebarClickHandler;

    impl TitlebarClickHandler {
        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            if event.clickCount() == 2 {
                self.window().unwrap().zoom(Some(self));
            }
        }
    }
);

impl TitlebarClickHandler {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        unsafe { msg_send![Self::alloc(mtm), init] }
    }
}

pub fn get_ns_window(window: &Window) -> Retained<NSWindow> {
    match window
        .window_handle()
        .expect("Failed to fetch window handle")
        .as_raw()
    {
        RawWindowHandle::AppKit(handle) => {
            let ns_view: Retained<NSView> = unsafe {
                Retained::retain(handle.ns_view.as_ptr().cast())
                    .expect("Failed to get NSView instance.")
            };
            ns_view
                .window()
                .expect("NSView was not installed in a window")
        }
        _ => panic!("Not an AppKit window"),
    }
}

fn load_icon_from_custom_path(icon_path: &str) -> Option<Retained<NSImage>> {
    let path = NSString::from_str(icon_path);
    NSImage::initWithContentsOfFile(NSImage::alloc(), &path)
}

fn load_icon_from_default_bytes() -> Option<Retained<NSImage>> {
    unsafe {
        let data = NSData::dataWithBytes_length(
            DEFAULT_NEOVIDE_ICON_BYTES.as_ptr() as *mut c_void,
            DEFAULT_NEOVIDE_ICON_BYTES.len(),
        );
        NSImage::initWithData(NSImage::alloc(), data.as_ref())
    }
}

fn load_neovide_icon(custom_icon_path: Option<&String>) -> Option<Retained<NSImage>> {
    custom_icon_path
        .and_then(|path| load_icon_from_custom_path(path))
        .or_else(load_icon_from_default_bytes)
}

#[derive(Debug)]
pub struct MacosWindowFeature {
    ns_window: Retained<NSWindow>,
    system_titlebar_height: f64,
    titlebar_click_handler: Option<Retained<TitlebarClickHandler>>,
    // Extra titlebar height in --frame transparency. 0 in other cases.
    extra_titlebar_height_in_pixel: u32,
    is_fullscreen: bool,
    menu: Option<Menu>,
    settings: Arc<Settings>,
    #[allow(dead_code)]
    activation_hotkey: Option<GlobalHotkeys>,
}

impl MacosWindowFeature {
    pub fn from_winit_window(
        window: &Window,
        settings: Arc<Settings>,
        proxy: EventLoopProxy<EventPayload>,
    ) -> Self {
        let mtm =
            MainThreadMarker::new().expect("MacosWindowFeature must be created in main thread.");

        let system_titlebar_height = Self::system_titlebar_height(mtm);

        let ns_window = get_ns_window(window);

        let cmd_line_settings = settings.get::<CmdLineSettings>();
        SHOW_NATIVE_TAB_BAR.store(cmd_line_settings.macos_native_tabs, Ordering::Relaxed);

        ns_window.setTabbingMode(NSWindowTabbingMode::Preferred);
        Self::configure_native_tabbing(&ns_window);
        merge_all_windows_if_native_tabs(&ns_window);

        let mut extra_titlebar_height_in_pixel: u32 = 0;

        let frame = cmd_line_settings.frame;
        let titlebar_click_handler: Option<Retained<TitlebarClickHandler>> = match frame {
            Frame::Transparent => {
                let titlebar_click_handler = TitlebarClickHandler::new(mtm);

                // Add the titlebar_click_handler into the view of window.
                let content_view = ns_window.contentView().unwrap();
                content_view.addSubview(&titlebar_click_handler);

                // Set the initial size of titlebar_click_handler.
                let content_view_size = content_view.frame().size;
                titlebar_click_handler.setFrame(NSRect::new(
                    NSPoint::new(0., content_view_size.height - system_titlebar_height),
                    NSSize::new(content_view_size.width, system_titlebar_height),
                ));

                // Setup auto layout for titlebar_click_handler.
                titlebar_click_handler.setAutoresizingMask(
                    NSAutoresizingMaskOptions::ViewWidthSizable
                        | NSAutoresizingMaskOptions::ViewMinYMargin,
                );
                titlebar_click_handler.setTranslatesAutoresizingMaskIntoConstraints(true);

                extra_titlebar_height_in_pixel =
                    Self::titlebar_height_in_pixel(system_titlebar_height, window.scale_factor());

                Some(titlebar_click_handler)
            }
            _ => None,
        };

        let is_fullscreen = ns_window
            .styleMask()
            .contains(NSWindowStyleMask::FullScreen);

        store_event_loop_proxy(proxy.clone());
        let activation_hotkey = GlobalHotkeys::register(proxy);

        let macos_window_feature = MacosWindowFeature {
            ns_window,
            system_titlebar_height,
            titlebar_click_handler,
            extra_titlebar_height_in_pixel,
            is_fullscreen,
            menu: None,
            settings: settings.clone(),
            activation_hotkey,
        };

        macos_window_feature.update_background(true);

        macos_window_feature
    }
    fn configure_native_tabbing(ns_window: &NSWindow) {
        ns_window.setTabbingIdentifier(ns_string!(NEOVIDE_TABBING_IDENTIFIER));
        Self::apply_tab_bar_preference(ns_window);
    }

    fn apply_tab_bar_preference(ns_window: &NSWindow) {
        if let Some(tab_group) = ns_window.tabGroup() {
            let should_show = should_show_native_tab_bar();
            if tab_group.isTabBarVisible() != should_show {
                ns_window.toggleTabBar(None);
            }
        }
    }

    fn begin_tab_overview(ns_window: &NSWindow) {
        if Self::merge_windows_for_overview(ns_window) {
            TAB_OVERVIEW_ACTIVE.with(|active| active.set(true));
            ACTIVE_HOST_WINDOW.with(|cell| cell.set(0));
            SUPPRESS_UNTIL_NEXT_KEY_EVENT.with(|cell| cell.set(true));
            ns_window.toggleTabOverview(None);
        }
    }

    fn merge_windows_for_overview(ns_window: &NSWindow) -> bool {
        ns_window.mergeAllWindows(None);

        if let Some(tab_group) = ns_window.tabGroup() {
            let windows = tab_group.windows();
            if windows.len() <= 1 {
                return false;
            }
            tab_group.setSelectedWindow(Some(ns_window));
            true
        } else {
            false
        }
    }

    fn detach_tabs_after_overview(ns_window: &NSWindow) {
        let should_detach = TAB_OVERVIEW_ACTIVE.with(|active| active.get());
        if !should_detach {
            return;
        }

        if should_show_native_tab_bar() {
            TAB_OVERVIEW_ACTIVE.with(|active| active.set(false));
            PENDING_DETACH_WINDOW.with(|ptr| ptr.set(0));
            ACTIVE_HOST_WINDOW.with(|cell| cell.set(0));
            ns_window.makeKeyAndOrderFront(None);
            ns_window.orderFrontRegardless();
            record_host_window(ns_window);
            Self::apply_tab_bar_preference(ns_window);
            if let Some(mtm) = MainThreadMarker::new() {
                let app = NSApplication::sharedApplication(mtm);
                app.setWindowsNeedUpdate(true);
            }
            return;
        }

        let Some(tab_group) = ns_window.tabGroup() else {
            TAB_OVERVIEW_ACTIVE.with(|active| active.set(false));
            return;
        };

        TAB_OVERVIEW_ACTIVE.with(|active| active.set(false));
        PENDING_DETACH_WINDOW.with(|ptr| ptr.set(0));
        ACTIVE_HOST_WINDOW.with(|cell| cell.set(0));
        let _focus_guard = FocusSuppressionGuard::new();
        PENDING_DETACH_WINDOW.with(|ptr| ptr.set(0));

        if tab_group.isOverviewVisible() {
            return;
        }

        let windows_array = tab_group.windows();
        if windows_array.len() <= 1 {
            TAB_OVERVIEW_ACTIVE.with(|active| active.set(false));
            return;
        }

        let retained_windows: Vec<Retained<NSWindow>> =
            windows_array.iter().map(|window| window.retain()).collect();

        for window in &retained_windows {
            let window_ref: &NSWindow = window.as_ref();
            if ptr::eq(window_ref, ns_window) {
                continue;
            }
            window_ref.moveTabToNewWindow(None);
            window_ref.orderBack(None);
            log::trace!(
                "Detached tab window ptr={:?} from host={:?}",
                window_identifier(window_ref),
                window_identifier(ns_window)
            );
            Self::apply_tab_bar_preference(window_ref);
        }

        ns_window.makeKeyAndOrderFront(None);
        ns_window.orderFrontRegardless();
        record_host_window(ns_window);
        Self::apply_tab_bar_preference(ns_window);
        if let Some(mtm) = MainThreadMarker::new() {
            let app = NSApplication::sharedApplication(mtm);
            app.setWindowsNeedUpdate(true);
        }
    }

    // Used to calculate the value of TITLEBAR_HEIGHT, aka, titlebar height in dpi-independent length.
    fn system_titlebar_height(mtm: MainThreadMarker) -> f64 {
        // Do a test to calculate this.
        let mock_content_rect = NSRect::new(NSPoint::new(100., 100.), NSSize::new(100., 100.));
        let frame_rect = NSWindow::frameRectForContentRect_styleMask(
            mock_content_rect,
            NSWindowStyleMask::Titled,
            mtm,
        );
        frame_rect.size.height - mock_content_rect.size.height
    }

    fn titlebar_height_in_pixel(system_titlebar_height: f64, scale_factor: f64) -> u32 {
        (system_titlebar_height * scale_factor) as u32
    }

    pub fn handle_scale_factor_update(&mut self, scale_factor: f64) {
        // If 0, there needs no extra height.
        if self.extra_titlebar_height_in_pixel != 0 {
            self.extra_titlebar_height_in_pixel =
                Self::titlebar_height_in_pixel(self.system_titlebar_height, scale_factor);
        }
    }

    fn set_titlebar_click_handler_visible(&self, visible: bool) {
        if let Some(titlebar_click_handler) = &self.titlebar_click_handler {
            titlebar_click_handler.setHidden(!visible);
        }
    }

    pub fn handle_size_changed(&mut self) {
        let is_fullscreen = self
            .ns_window
            .styleMask()
            .contains(NSWindowStyleMask::FullScreen);
        if is_fullscreen != self.is_fullscreen {
            self.is_fullscreen = is_fullscreen;
            self.set_titlebar_click_handler_visible(!is_fullscreen);
        }
    }

    /// Get the extra titlebar height in pixels, so Neovide can do the correct top padding.
    fn tab_bar_padding_in_pixels(&self) -> u32 {
        if !should_show_native_tab_bar() {
            return 0;
        }
        let Some(tab_group) = self.ns_window.tabGroup() else {
            return 0;
        };
        if !tab_group.isTabBarVisible() {
            return 0;
        }
        let windows = tab_group.windows();
        if windows.len() <= 1 {
            return 0;
        }
        let scale_factor = self.ns_window.backingScaleFactor();
        Self::titlebar_height_in_pixel(self.system_titlebar_height, scale_factor)
    }

    pub fn extra_titlebar_height_in_pixels(&self) -> u32 {
        if self.is_fullscreen {
            return 0;
        }
        self.extra_titlebar_height_in_pixel + self.tab_bar_padding_in_pixels()
    }

    /// Print a deprecation warning for `neovide_background_color`
    fn display_deprecation_warning(&self) {
        error_msg!(concat!(
            "neovide_background_color has now been deprecated. ",
            "Use neovide_opacity instead if you want to get a transparent window titlebar. ",
            "Please check https://neovide.dev/configuration.html#background-color-deprecated-currently-macos-only for more information.",
        ));
    }

    fn update_ns_background_legacy(
        &self,
        color: Color,
        show_border: bool,
        ignore_deprecation_warning: bool,
    ) {
        if !ignore_deprecation_warning {
            self.display_deprecation_warning();
        }
        let [red, green, blue, alpha] = color.to_array();
        let opaque = alpha >= 1.0;
        let ns_background = if opaque && show_border {
            NSColor::colorWithSRGBRed_green_blue_alpha(
                red.into(),
                green.into(),
                blue.into(),
                alpha.into(),
            )
        } else if !opaque {
            // Use white with very low alpha to make borders rendering properly
            NSColor::whiteColor().colorWithAlphaComponent(0.001)
        } else {
            NSColor::clearColor()
        };
        self.ns_window.setBackgroundColor(Some(&ns_background));
        // Show shadow if window is opaque OR has border decoration
        self.ns_window.setHasShadow(opaque || show_border);
        // Setting the window to opaque upon creation shows a permanent subtle grey border on the top edge of the window
        self.ns_window.setOpaque(opaque && show_border);
        self.ns_window.invalidateShadow();
    }

    fn update_ns_background(&self, opaque: bool, show_border: bool) {
        // Setting the background color to `NSColor::windowBackgroundColor()`
        // makes the background opaque and draws a grey border around the window
        let ns_background = if opaque && show_border {
            NSColor::windowBackgroundColor()
        } else if !opaque {
            // Use white with very low alpha to make borders rendering properly
            NSColor::whiteColor().colorWithAlphaComponent(0.001)
        } else {
            NSColor::clearColor()
        };
        self.ns_window.setBackgroundColor(Some(&ns_background));
        // Show shadow if window is opaque OR has border decoration
        self.ns_window.setHasShadow(opaque || show_border);
        // Setting the window to opaque upon creation shows a permanent subtle grey border on the top edge of the window
        self.ns_window.setOpaque(opaque && show_border);
        self.ns_window.invalidateShadow();
    }

    /// Update background color, opacity, shadow and blur of a window.
    fn update_background(&self, ignore_deprecation_warning: bool) {
        let WindowSettings {
            background_color,
            show_border,
            opacity,
            normal_opacity,
            ..
        } = self.settings.get::<WindowSettings>();
        let opaque = opacity.min(normal_opacity) >= 1.0;
        match background_color.parse::<Color>() {
            Ok(color) => {
                self.update_ns_background_legacy(color, show_border, ignore_deprecation_warning)
            }
            _ => self.update_ns_background(opaque, show_border),
        }
    }

    pub fn handle_settings_changed(&self, changed_setting: WindowSettingsChanged) {
        match changed_setting {
            WindowSettingsChanged::BackgroundColor(background_color) => {
                log::info!("background_color changed to {background_color}");
                self.update_background(false);
            }
            WindowSettingsChanged::ShowBorder(show_border) => {
                log::info!("show_border changed to {show_border}");
                self.update_background(true);
            }
            WindowSettingsChanged::Opacity(opacity) => {
                log::info!("opacity changed to {opacity}");
                self.update_background(true);
            }
            WindowSettingsChanged::WindowBlurred(window_blurred) => {
                log::info!("window_blurred changed to {window_blurred}");
                self.update_background(true);
            }
            _ => {}
        }
    }

    pub fn activate_application(&self) {
        match MainThreadMarker::new() {
            Some(mtm) => {
                let app = NSApplication::sharedApplication(mtm);
                #[allow(deprecated)]
                app.activateIgnoringOtherApps(true);
                self.ns_window.makeKeyAndOrderFront(None);
                if should_show_native_tab_bar() {
                    merge_all_windows_if_native_tabs(&self.ns_window);
                    if let Some(tab_group) = self.ns_window.tabGroup() {
                        tab_group.setSelectedWindow(Some(&self.ns_window));
                    }
                    Self::apply_tab_bar_preference(&self.ns_window);
                }
            }
            None => {
                log::warn!("macOS activation shortcut attempted to activate window outside the main thread");
            }
        }
    }

    pub fn hide_window(&self) {
        self.ns_window.orderOut(None);
    }

    pub fn is_key_window(&self) -> bool {
        self.ns_window.isKeyWindow()
    }

    /// Create the application menu and grab initial focus.
    pub fn ensure_app_initialized(&mut self) {
        let mtm = MainThreadMarker::new().expect("Menu must be created on the main thread");
        if self.menu.is_none() {
            self.menu = Some(Menu::new(mtm));
            let app = NSApplication::sharedApplication(mtm);
            #[allow(deprecated)]
            app.activateIgnoringOtherApps(true);

            // Make sure the icon is loaded when launched from terminal
            let icon = load_neovide_icon(self.settings.get::<CmdLineSettings>().icon.as_ref());
            let icon_ref: Option<&NSImage> = icon.as_ref().map(|img| img.as_ref());
            unsafe { app.setApplicationIconImage(icon_ref) }
        }
    }
}

define_class!(
    #[derive(Debug)]
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    struct QuitHandler;

    impl QuitHandler {
        #[unsafe(method(quit:))]
        fn quit(&self, _event: &NSEvent) {
            let handler = {
                let handler_lock = HANDLER_REGISTRY.lock().unwrap();
                handler_lock
                    .clone()
                    .expect("NeovimHandler has not been initialized")
            };
            send_ui(SerialCommand::Keyboard("<D-q>".into()), &handler);
        }
    }
);

impl QuitHandler {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        unsafe { msg_send![Self::alloc(mtm), init] }
    }
}

define_class!(
    #[derive(Debug)]
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    struct NewWindowHandler;

    impl NewWindowHandler {
        #[unsafe(method(neovideCreateWindow:))]
        fn create_window(&self, _sender: Option<&AnyObject>) {
            request_new_window();
        }
    }
);

impl NewWindowHandler {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        unsafe { msg_send![Self::alloc(mtm), init] }
    }
}

#[derive(Clone, Debug)]
struct TabOverviewHandlerIvars {}

define_class!(
    #[derive(Debug)]
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = TabOverviewHandlerIvars]
    struct TabOverviewHandler;

    impl TabOverviewHandler {
        #[unsafe(method(neovideShowAllTabs:))]
        fn show_all_tabs(&self, _sender: Option<&AnyObject>) {
            trigger_tab_overview();
        }
    }
);

impl TabOverviewHandler {
    fn new(mtm: MainThreadMarker) -> Retained<TabOverviewHandler> {
        unsafe { msg_send![Self::alloc(mtm), init] }
    }
}

#[derive(Clone, Debug)]
struct TabOverviewNotificationHandlerIvars {}

define_class!(
    #[derive(Debug)]
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = TabOverviewNotificationHandlerIvars]
    struct TabOverviewNotificationHandler;

    impl TabOverviewNotificationHandler {
        #[unsafe(method(neovideWindowDidBecomeKey:))]
        fn window_did_become_key(&self, notification: &NSNotification) {
            if !TAB_OVERVIEW_ACTIVE.with(|active| active.get()) {
                return;
            }

            let Some(object) = notification.object() else {
                return;
            };
            let window: Retained<NSWindow> = object
                .downcast()
                .expect("notification object was not an NSWindow");
            let window_ref: &NSWindow = window.as_ref();

            let identifier = window_ref.tabbingIdentifier();
            let identifier_ref: &NSString = identifier.as_ref();
            if identifier_ref != ns_string!(NEOVIDE_TABBING_IDENTIFIER) {
                log::trace!(
                    "WindowDidBecomeKey ignored (tab id = {})",
                    identifier_ref.to_string()
                );
                return;
            }
            SUPPRESS_UNTIL_NEXT_KEY_EVENT.with(|cell| cell.set(false));

            let ptr_value = window_identifier(window_ref);
            let previous_host = ACTIVE_HOST_WINDOW.with(|cell| {
                let previous = cell.get();
                cell.set(ptr_value);
                previous
            });
            if previous_host != 0 && previous_host != ptr_value {
                log::trace!(
                    "WindowDidBecomeKey host switched from {:?} to {:?}",
                    previous_host as *const (),
                    window_identifier(window_ref)
                );
            }
            let already_pending = PENDING_DETACH_WINDOW.with(|ptr| ptr.get() == ptr_value);
            if already_pending {
                log::trace!(
                    "WindowDidBecomeKey skipping duplicate scheduling (window ptr = {:?})",
                    window_identifier(window_ref)
                );
                return;
            }
            PENDING_DETACH_WINDOW.with(|ptr| ptr.set(ptr_value));

            log::trace!(
                "WindowDidBecomeKey scheduling detach (window ptr = {:?})",
                window_identifier(window_ref)
            );
            unsafe {
                self.schedule_detach(window);
            }
        }

        #[unsafe(method(neovidePerformDetach:))]
        fn perform_detach(&self, timer: &NSTimer) {
            let Some(user_info) = timer.userInfo() else {
                return;
            };
            let window: Retained<NSWindow> = user_info
                .downcast()
                .expect("timer userInfo was not an NSWindow");
            let ptr_value = window_identifier(window.as_ref());
            let host_ptr = ACTIVE_HOST_WINDOW.with(|cell| cell.get());
            if host_ptr != 0 && host_ptr != ptr_value {
                log::trace!(
                    "Detach timer ignoring stale window ptr = {:?} (active host = {:?})",
                    window_identifier(window.as_ref()),
                    host_ptr
                );
                return;
            }
            PENDING_DETACH_WINDOW.with(|ptr| ptr.set(0));
            log::trace!(
                "Detach timer fired for window ptr = {:?}",
                window_identifier(window.as_ref())
            );
            MacosWindowFeature::detach_tabs_after_overview(window.as_ref());
        }
    }
);

impl TabOverviewNotificationHandler {
    fn register(mtm: MainThreadMarker) -> Retained<TabOverviewNotificationHandler> {
        let handler: Retained<TabOverviewNotificationHandler> =
            unsafe { msg_send![mtm.alloc(), init] };
        let center = NSNotificationCenter::defaultCenter();
        unsafe {
            center.addObserver_selector_name_object(
                &handler,
                sel!(neovideWindowDidBecomeKey:),
                Some(NSWindowDidBecomeKeyNotification),
                None,
            );
        }
        log::trace!("Registered NSWindowDidBecomeKey observer");
        handler
    }

    unsafe fn schedule_detach(&self, window: Retained<NSWindow>) {
        log::trace!(
            "Scheduling detach timer for window ptr = {:?}",
            window_identifier(window.as_ref())
        );
        let _: Retained<NSTimer> =
            NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                0.0,
                self,
                sel!(neovidePerformDetach:),
                Some(window.as_ref()),
                false,
            );
    }
}

#[derive(Clone, Debug)]
struct WindowMenuDelegateIvars {}

define_class!(
    #[derive(Debug)]
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = WindowMenuDelegateIvars]
    struct WindowMenuDelegate;

    impl WindowMenuDelegate {
        #[unsafe(method(menuNeedsUpdate:))]
        fn menu_needs_update(&self, menu: &NSMenu) {
            Menu::remove_system_show_all_tabs(menu);
        }
    }
);

unsafe impl NSObjectProtocol for WindowMenuDelegate {}
unsafe impl NSMenuDelegate for WindowMenuDelegate {}

impl WindowMenuDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<WindowMenuDelegate> {
        unsafe { msg_send![Self::alloc(mtm), init] }
    }
}

#[derive(Debug)]
struct Menu {
    quit_handler: Retained<QuitHandler>,
    new_window_handler: Retained<NewWindowHandler>,
    tab_overview_handler: Retained<TabOverviewHandler>,
    _tab_overview_observer: Retained<TabOverviewNotificationHandler>,
    window_menu_delegate: Retained<WindowMenuDelegate>,
}

impl Menu {
    fn new(mtm: MainThreadMarker) -> Self {
        let menu = Menu {
            quit_handler: QuitHandler::new(mtm),
            new_window_handler: NewWindowHandler::new(mtm),
            tab_overview_handler: TabOverviewHandler::new(mtm),
            _tab_overview_observer: TabOverviewNotificationHandler::register(mtm),
            window_menu_delegate: WindowMenuDelegate::new(mtm),
        };
        menu.add_menus(mtm);
        menu
    }

    fn add_app_menu(&self, mtm: MainThreadMarker) -> Retained<NSMenu> {
        unsafe {
            let app_menu = NSMenu::new(mtm);
            let process_name = NSProcessInfo::processInfo().processName();
            let about_item = NSMenuItem::new(mtm);
            about_item.setTitle(&ns_string!("About ").stringByAppendingString(&process_name));
            about_item.setAction(Some(sel!(orderFrontStandardAboutPanel:)));
            app_menu.addItem(&about_item);

            let services_item = NSMenuItem::new(mtm);
            let services_menu = NSMenu::new(mtm);
            services_item.setTitle(ns_string!("Services"));
            services_item.setSubmenu(Some(&services_menu));
            app_menu.addItem(&services_item);

            let sep = NSMenuItem::separatorItem(mtm);
            app_menu.addItem(&sep);

            // application window operations
            let hide_item = NSMenuItem::new(mtm);
            hide_item.setTitle(&ns_string!("Hide ").stringByAppendingString(&process_name));
            hide_item.setKeyEquivalent(ns_string!("h"));
            hide_item.setAction(Some(sel!(hide:)));
            app_menu.addItem(&hide_item);

            let hide_others_item = NSMenuItem::new(mtm);
            hide_others_item.setTitle(ns_string!("Hide Others"));
            hide_others_item.setKeyEquivalent(ns_string!("h"));
            hide_others_item.setKeyEquivalentModifierMask(
                NSEventModifierFlags::Option | NSEventModifierFlags::Command,
            );
            hide_others_item.setAction(Some(sel!(hideOtherApplications:)));
            app_menu.addItem(&hide_others_item);

            let show_all_item = NSMenuItem::new(mtm);
            show_all_item.setTitle(ns_string!("Show All"));
            show_all_item.setAction(Some(sel!(unhideAllApplications:)));

            // quit
            let sep = NSMenuItem::separatorItem(mtm);
            app_menu.addItem(&sep);

            let quit_item = NSMenuItem::new(mtm);
            quit_item.setTitle(&ns_string!("Quit ").stringByAppendingString(&process_name));
            quit_item.setKeyEquivalent(ns_string!("q"));
            quit_item.setAction(Some(sel!(quit:)));
            quit_item.setTarget(Some(&self.quit_handler));
            app_menu.addItem(&quit_item);

            app_menu
        }
    }

    fn add_menus(&self, mtm: MainThreadMarker) {
        let app = NSApplication::sharedApplication(mtm);

        let main_menu = NSMenu::new(mtm);

        let app_menu = self.add_app_menu(mtm);
        let app_menu_item = NSMenuItem::new(mtm);
        app_menu_item.setSubmenu(Some(&app_menu));
        if let Some(services_menu) = app_menu.itemWithTitle(ns_string!("Services")) {
            app.setServicesMenu(services_menu.submenu().as_deref());
        }
        main_menu.addItem(&app_menu_item);

        let win_menu = self.add_window_menu(mtm);
        let win_menu_item = NSMenuItem::new(mtm);
        win_menu_item.setSubmenu(Some(&win_menu));
        main_menu.addItem(&win_menu_item);
        app.setWindowsMenu(Some(&win_menu));
        Self::remove_system_show_all_tabs(&win_menu);
        app.setMainMenu(Some(&main_menu));
    }

    fn add_window_menu(&self, mtm: MainThreadMarker) -> Retained<NSMenu> {
        unsafe {
            let menu = NSMenu::new(mtm);
            menu.setTitle(ns_string!("Window"));
            let delegate: &ProtocolObject<dyn NSMenuDelegate> =
                ProtocolObject::from_ref::<WindowMenuDelegate>(self.window_menu_delegate.as_ref());
            menu.setDelegate(Some(delegate));

            let full_screen_item = NSMenuItem::new(mtm);
            full_screen_item.setTitle(ns_string!("Enter Full Screen"));
            full_screen_item.setKeyEquivalent(ns_string!("f"));
            full_screen_item.setAction(Some(sel!(toggleFullScreen:)));
            full_screen_item.setKeyEquivalentModifierMask(
                NSEventModifierFlags::Control | NSEventModifierFlags::Command,
            );
            menu.addItem(&full_screen_item);

            let create_new_window = NSMenuItem::new(mtm);
            create_new_window.setTitle(ns_string!("New Window"));
            create_new_window.setKeyEquivalent(ns_string!("n"));
            create_new_window.setAction(Some(sel!(neovideCreateWindow:)));
            create_new_window.setTarget(Some(&self.new_window_handler));
            menu.addItem(&create_new_window);

            let show_all_tabs_item = NSMenuItem::new(mtm);
            show_all_tabs_item.setTitle(ns_string!("Editors"));
            show_all_tabs_item.setKeyEquivalent(ns_string!("e"));
            show_all_tabs_item.setKeyEquivalentModifierMask(
                NSEventModifierFlags::Command | NSEventModifierFlags::Shift,
            );
            show_all_tabs_item.setAction(Some(sel!(neovideShowAllTabs:)));
            show_all_tabs_item.setTarget(Some(&self.tab_overview_handler));
            menu.addItem(&show_all_tabs_item);

            let min_item = NSMenuItem::new(mtm);
            min_item.setTitle(ns_string!("Minimize"));
            min_item.setKeyEquivalent(ns_string!("m"));
            min_item.setAction(Some(sel!(performMiniaturize:)));
            menu.addItem(&min_item);
            menu
        }
    }

    fn remove_system_show_all_tabs(menu: &NSMenu) {
        let mut idx = menu.numberOfItems();
        while idx > 0 {
            idx -= 1;
            if let Some(item) = menu.itemAtIndex(idx) {
                let title = item.title();
                let title_ref: &NSString = title.as_ref();
                if title_ref != ns_string!("Show All Tabs") {
                    continue;
                }
                let action = item.action();
                if action.map_or(true, |sel| sel != sel!(neovideShowAllTabs:)) {
                    menu.removeItemAtIndex(idx);
                }
            }
        }
    }
}

pub fn trigger_tab_overview() {
    if let Some(mtm) = MainThreadMarker::new() {
        let app = NSApplication::sharedApplication(mtm);
        if let Some(window) = app.keyWindow() {
            MacosWindowFeature::begin_tab_overview(&window);
        }
    }
}

pub fn register_file_handler() {
    // See signature at
    // https://developer.apple.com/documentation/appkit/nsapplicationdelegate/application(_:openfiles:)?language=objc
    unsafe extern "C-unwind" fn handle_open_files(
        _this: &mut AnyObject,
        _sel: objc2::runtime::Sel,
        _sender: &objc2::runtime::AnyObject,
        filenames: &NSArray<NSString>,
    ) {
        autoreleasepool(|_| {
            // NOTE: Once Neovide is open and the user drops a new file, it opens as expected.
            // TODO: For files to be visualized upon being dropped when Neovide is not started,
            // a valid Neovim handler must be initialized. This requirement explains why files
            // might not appear if the application is not already opened or if the Neovim handler
            // has not been initialized. Without an active handler, the very first file drop event
            // cannot be processed, and consequently, the file will not be displayed.
            if let Some(handler) = HANDLER_REGISTRY.lock().unwrap().clone() {
                log::info!("Received files: {:?}", filenames);
                for filename in filenames.iter() {
                    send_ui(ParallelCommand::FileDrop(filename.to_string()), &handler);
                }
            }
        });
    }

    let mtm = MainThreadMarker::new().expect("File handler must be registered on main thread.");

    unsafe {
        let app = NSApplication::sharedApplication(mtm);
        let delegate = app.delegate().unwrap();

        // Find out class of the NSApplicationDelegate
        let class: &AnyClass = AnyObject::class(delegate.as_ref());

        // register subclass of whatever was in delegate
        let mut my_class = ClassBuilder::new(c"NeovideApplicationDelegate", class).unwrap();
        my_class.add_method(
            sel!(application:openFiles:),
            handle_open_files as unsafe extern "C-unwind" fn(_, _, _, _) -> _,
        );
        let class = my_class.register();

        // this should be safe as:
        //  * our class is a subclass
        //  * no new ivars
        //  * overridden methods are compatible with old (we implement protocol method)
        AnyObject::set_class(delegate.as_ref(), class);
    }

    // Prevent AppKit from interpreting our command line.
    let keys = &[ns_string!("NSTreatUnknownArgumentsAsOpen")];
    // API requires `AnyObject[]` not `NSString[]`.
    let objects = &[ns_string!("NO") as &AnyObject];
    let dict = NSDictionary::from_slices(keys, objects);
    unsafe {
        NSUserDefaults::standardUserDefaults().registerDefaults(&dict);
    }
}
pub fn window_identifier(window: &NSWindow) -> usize {
    window as *const _ as usize
}

pub fn record_host_window(window: &NSWindow) {
    LAST_HOST_WINDOW.with(|cell| cell.set(window_identifier(window)));
}

pub fn get_last_host_window() -> usize {
    LAST_HOST_WINDOW.with(|cell| cell.get())
}

pub fn hide_application() {
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
