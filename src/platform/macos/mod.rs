use std::cell::RefCell;
use std::ffi::CString;
use std::os::raw::c_void;
use std::str;
use std::sync::OnceLock;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

use objc2::AnyThread;
use objc2::MainThreadOnly;
use objc2::define_class;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::AnyClass;
use objc2::runtime::AnyObject;
use objc2::runtime::ClassBuilder;
use objc2::runtime::ProtocolObject;
use objc2::sel;
use objc2_app_kit::NSApplication;
use objc2_app_kit::NSEvent;
use objc2_app_kit::NSImage;
use objc2_app_kit::NSMenu;
use objc2_app_kit::NSMenuDelegate;
use objc2_app_kit::NSMenuItem;
use objc2_app_kit::NSView;
use objc2_app_kit::NSWindow;
use objc2_app_kit::NSWindowDidBecomeKeyNotification;
use objc2_app_kit::NSWindowDidBecomeMainNotification;
use objc2_app_kit::NSWorkspace;
use objc2_foundation::MainThreadMarker;
use objc2_foundation::NSArray;
use objc2_foundation::NSData;
use objc2_foundation::NSDictionary;
use objc2_foundation::NSNotification;
use objc2_foundation::NSNotificationCenter;
use objc2_foundation::NSObject;
use objc2_foundation::NSObjectProtocol;
use objc2_foundation::NSProcessInfo;
use objc2_foundation::NSString;
use objc2_foundation::NSTimer;
use objc2_foundation::NSURL;
use objc2_foundation::NSUserDefaults;
use objc2_foundation::ns_string;
use raw_window_handle::HasWindowHandle;
use raw_window_handle::RawWindowHandle;
use winit::event_loop::EventLoopProxy;
use winit::window::Window;
use winit::window::WindowId;

use crate::bridge::SerialCommand;
use crate::bridge::require_active_handler;
use crate::bridge::send_or_queue_file_drop;
use crate::bridge::send_ui;
use crate::cmd_line::CmdLineSettings;
use crate::settings::Settings;
use crate::utils::expand_tilde;
use crate::window::EventPayload;
use crate::window::MacShortcutCommand;
use crate::window::MacosWindowFeature;
use crate::window::UserEvent;
use crate::window::macos::tab_navigation::KeyCombo;

thread_local! {
    static APP_MENU: RefCell<Option<Menu>> = const { RefCell::new(None) };
}

pub(crate) static SHOW_NATIVE_TAB_BAR: AtomicBool = AtomicBool::new(false);
pub(crate) static ENABLE_NATIVE_TAB_BAR: AtomicBool = AtomicBool::new(false);
static EVENT_LOOP_PROXY: OnceLock<EventLoopProxy<EventPayload>> = OnceLock::new();

pub(crate) const NEOVIDE_TABBING_IDENTIFIER: &str = "NeovideWindowTabGroup";
const NEOVIDE_WEBSITE_URL: &str = "https://neovide.dev/";
const NEOVIDE_SPONSOR_URL: &str = "https://github.com/sponsors/neovide";
const NEOVIDE_CREATE_ISSUE_URL: &str =
    "https://github.com/neovide/neovide/issues/new?template=bug_report.md";
static DEFAULT_NEOVIDE_ICON_BYTES: &[u8] =
    include_bytes!("../../../extra/osx/Neovide.app/Contents/Resources/Neovide.icns");

fn native_tabs_enabled() -> bool { ENABLE_NATIVE_TAB_BAR.load(Ordering::Relaxed) }

pub(crate) fn should_show_native_tab_bar() -> bool {
    native_tabs_enabled() && SHOW_NATIVE_TAB_BAR.load(Ordering::Relaxed)
}

pub(crate) fn store_event_loop_proxy(proxy: EventLoopProxy<EventPayload>) {
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

pub fn request_activate_window(window_id: WindowId) {
    let Some(proxy) = EVENT_LOOP_PROXY.get() else {
        log::warn!("Window activation requested before event loop proxy became available");
        return;
    };

    if let Err(error) = proxy.send_event(EventPayload::all(UserEvent::ActivateWindow(window_id))) {
        log::error!("Failed to request window activation: {:?}", error);
    }
}

fn refresh_windows_menu_for_ns_window(app: &NSApplication, window: &NSWindow) {
    let title = window.title();
    let title_ref: &NSString = title.as_ref();
    unsafe {
        window.setExcludedFromWindowsMenu(false);
        let _: () = msg_send![app, removeWindowsItem: window];
        let _: () = msg_send![app, addWindowsItem: window, title: title_ref, filename: false];
        let _: () = msg_send![app, updateWindowsItem: window];
    }
}

fn refresh_windows_menu_for_app(app: &NSApplication) {
    let windows = app.windows();
    for window in windows.iter() {
        refresh_windows_menu_for_ns_window(app, window.as_ref());
    }
    app.setWindowsNeedUpdate(true);
}

pub fn native_tab_bar_enabled() -> bool { native_tabs_enabled() }

pub(crate) fn merge_all_windows_if_native_tabs(ns_window: &NSWindow) {
    if should_show_native_tab_bar() {
        ns_window.mergeAllWindows(None);
        MacosWindowFeature::apply_tab_bar_preference(ns_window);
    }
}

#[link(name = "Quartz", kind = "framework")]
unsafe extern "C" {}

pub fn get_ns_window(window: &Window) -> Retained<NSWindow> {
    match window.window_handle().expect("Failed to fetch window handle").as_raw() {
        RawWindowHandle::AppKit(handle) => {
            let ns_view: Retained<NSView> = unsafe {
                Retained::retain(handle.ns_view.as_ptr().cast())
                    .expect("Failed to get NSView instance.")
            };
            ns_view.window().expect("NSView was not installed in a window")
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

pub fn load_neovide_icon(custom_icon_path: Option<&String>) -> Option<Retained<NSImage>> {
    custom_icon_path
        .and_then(|path| {
            let expanded = expand_tilde(path);
            load_icon_from_custom_path(&expanded)
        })
        .or_else(load_icon_from_default_bytes)
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

fn open_external_url(url: &str) {
    let ns_url_string = NSString::from_str(url);
    if let Some(ns_url) = NSURL::URLWithString(&ns_url_string) {
        let workspace = NSWorkspace::sharedWorkspace();
        workspace.openURL(&ns_url);
    } else {
        log::warn!("Failed to open URL from Help menu: {url}");
    }
}

fn apply_menu_item_hotkey(item: &NSMenuItem, raw: &str, setting_name: &str) {
    let Some(shortcut) = KeyCombo::parse(raw) else {
        return;
    };

    let Some(key) = shortcut.to_key() else {
        log::warn!(
            "macOS menu shortcut '{raw}' for {setting_name} uses an unsupported named key; ignoring"
        );
        return;
    };

    item.setKeyEquivalent(key.as_ref());
    item.setKeyEquivalentModifierMask(shortcut.to_modifiers());
}

define_class!(
    #[derive(Debug)]
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    struct QuitHandler;

    impl QuitHandler {
        #[unsafe(method(quit:))]
        fn quit(&self, _event: &NSEvent) {
            let handler = require_active_handler();
            send_ui(SerialCommand::Keyboard("<D-q>".into()), &handler);
        }
    }
);

impl QuitHandler {
    fn new(mtm: MainThreadMarker) -> Retained<Self> { unsafe { msg_send![Self::alloc(mtm), init] } }
}

define_class!(
    #[derive(Debug)]
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    struct HelpMenuHandler;

    impl HelpMenuHandler {
        #[unsafe(method(createIssueReport:))]
        fn create_issue_report(&self, _sender: &AnyObject) {
            open_external_url(NEOVIDE_CREATE_ISSUE_URL);
        }

        #[unsafe(method(openNeovideWebsite:))]
        fn open_neovide_website(&self, _sender: &AnyObject) {
            open_external_url(NEOVIDE_WEBSITE_URL);
        }

        #[unsafe(method(openSponsorPage:))]
        fn open_sponsor_page(&self, _sender: &AnyObject) {
            open_external_url(NEOVIDE_SPONSOR_URL);
        }
    }
);

impl HelpMenuHandler {
    fn new(mtm: MainThreadMarker) -> Retained<Self> { unsafe { msg_send![Self::alloc(mtm), init] } }
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
    fn new(mtm: MainThreadMarker) -> Retained<Self> { unsafe { msg_send![Self::alloc(mtm), init] } }
}

#[derive(Clone, Debug)]
struct EditorSwitcherMenuHandlerIvars {}

define_class!(
    #[derive(Debug)]
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = EditorSwitcherMenuHandlerIvars]
    struct EditorSwitcherMenuHandler;

    impl EditorSwitcherMenuHandler {
        #[unsafe(method(neovideShowEditorSwitcher:))]
        fn show_editor_switcher(&self, _sender: Option<&AnyObject>) {
            request_editor_switcher();
        }
    }
);

impl EditorSwitcherMenuHandler {
    fn new(mtm: MainThreadMarker) -> Retained<EditorSwitcherMenuHandler> {
        unsafe { msg_send![Self::alloc(mtm), init] }
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

        #[unsafe(method(menuWillOpen:))]
        fn menu_will_open(&self, menu: &NSMenu) {
            Menu::remove_system_show_all_tabs(menu);
            unsafe { self.schedule_deferred_prune(menu) };
        }

        #[unsafe(method(neovidePruneWindowMenu:))]
        fn prune_window_menu(&self, timer: &NSTimer) {
            let Some(user_info) = timer.userInfo() else {
                return;
            };
            let menu: Retained<NSMenu> = user_info
                .downcast()
                .expect("timer userInfo was not an NSMenu");
            Menu::remove_system_show_all_tabs(menu.as_ref());
        }
    }
);

unsafe impl NSObjectProtocol for WindowMenuDelegate {}
unsafe impl NSMenuDelegate for WindowMenuDelegate {}

impl WindowMenuDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<WindowMenuDelegate> {
        unsafe { msg_send![Self::alloc(mtm), init] }
    }

    unsafe fn schedule_deferred_prune(&self, menu: &NSMenu) {
        let _: Retained<NSTimer> = unsafe {
            NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                0.0,
                self,
                sel!(neovidePruneWindowMenu:),
                Some(menu),
                false,
            )
        };
    }
}

#[derive(Clone, Debug)]
struct WindowMenuNotificationHandlerIvars {}

define_class!(
    #[derive(Debug)]
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = WindowMenuNotificationHandlerIvars]
    struct WindowMenuNotificationHandler;

    impl WindowMenuNotificationHandler {
        #[unsafe(method(neovideWindowDidBecomeMain:))]
        fn window_did_become_main(&self, notification: &NSNotification) {
            self.schedule_refresh_from_notification(notification);
        }

        #[unsafe(method(neovideWindowDidBecomeKey:))]
        fn window_did_become_key(&self, notification: &NSNotification) {
            self.schedule_refresh_from_notification(notification);
        }

        #[unsafe(method(neovideRefreshWindowMenu:))]
        fn refresh_window_menu(&self, _timer: &NSTimer) {
            if should_show_native_tab_bar() {
                return;
            }

            let Some(mtm) = MainThreadMarker::new() else {
                return;
            };

            let app = NSApplication::sharedApplication(mtm);
            refresh_windows_menu_for_app(app.as_ref());
        }
    }
);

impl WindowMenuNotificationHandler {
    fn register(mtm: MainThreadMarker) -> Retained<Self> {
        let handler: Retained<WindowMenuNotificationHandler> =
            unsafe { msg_send![mtm.alloc(), init] };
        let center = NSNotificationCenter::defaultCenter();
        unsafe {
            center.addObserver_selector_name_object(
                &handler,
                sel!(neovideWindowDidBecomeMain:),
                Some(NSWindowDidBecomeMainNotification),
                None,
            );
            center.addObserver_selector_name_object(
                &handler,
                sel!(neovideWindowDidBecomeKey:),
                Some(NSWindowDidBecomeKeyNotification),
                None,
            );
        }

        log::trace!("Registered NSWindowDidBecomeMain/Key observers for window menu refresh");
        handler
    }

    fn schedule_refresh_from_notification(&self, notification: &NSNotification) {
        if should_show_native_tab_bar() {
            return;
        }

        let Some(object) = notification.object() else {
            return;
        };

        let _: Retained<NSWindow> =
            object.downcast().expect("notification object was not an NSWindow");

        unsafe { self.schedule_refresh() }
    }

    unsafe fn schedule_refresh(&self) {
        let _: Retained<NSTimer> = unsafe {
            NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                0.0,
                self,
                sel!(neovideRefreshWindowMenu:),
                None,
                false,
            )
        };
    }
}
#[derive(Debug)]
struct Menu {
    quit_handler: Retained<QuitHandler>,
    help_menu_handler: Retained<HelpMenuHandler>,
    new_window_handler: Retained<NewWindowHandler>,
    editor_switcher_menu_handler: Retained<EditorSwitcherMenuHandler>,
    _window_menu_observer: Retained<WindowMenuNotificationHandler>,
    window_menu_delegate: Retained<WindowMenuDelegate>,
}

impl Menu {
    fn new(mtm: MainThreadMarker, settings: &Settings) -> Self {
        let menu = Menu {
            quit_handler: QuitHandler::new(mtm),
            help_menu_handler: HelpMenuHandler::new(mtm),
            new_window_handler: NewWindowHandler::new(mtm),
            editor_switcher_menu_handler: EditorSwitcherMenuHandler::new(mtm),
            _window_menu_observer: WindowMenuNotificationHandler::register(mtm),
            window_menu_delegate: WindowMenuDelegate::new(mtm),
        };
        menu.add_menus(mtm, &settings.get::<CmdLineSettings>());
        menu
    }

    fn add_app_menu(&self, mtm: MainThreadMarker, settings: &CmdLineSettings) -> Retained<NSMenu> {
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
            apply_menu_item_hotkey(&hide_item, &settings.system_hide_hotkey, "system_hide_hotkey");
            hide_item.setAction(Some(sel!(hide:)));
            app_menu.addItem(&hide_item);

            let hide_others_item = NSMenuItem::new(mtm);
            hide_others_item.setTitle(ns_string!("Hide Others"));
            apply_menu_item_hotkey(
                &hide_others_item,
                &settings.system_hide_others_hotkey,
                "system_hide_others_hotkey",
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
            apply_menu_item_hotkey(&quit_item, &settings.system_quit_hotkey, "system_quit_hotkey");
            quit_item.setAction(Some(sel!(quit:)));
            quit_item.setTarget(Some(&self.quit_handler));
            app_menu.addItem(&quit_item);

            app_menu
        }
    }

    fn add_menus(&self, mtm: MainThreadMarker, settings: &CmdLineSettings) {
        let app = NSApplication::sharedApplication(mtm);

        let main_menu = NSMenu::new(mtm);

        let app_menu = self.add_app_menu(mtm, settings);
        let app_menu_item = NSMenuItem::new(mtm);
        app_menu_item.setSubmenu(Some(&app_menu));
        if let Some(services_menu) = app_menu.itemWithTitle(ns_string!("Services")) {
            app.setServicesMenu(services_menu.submenu().as_deref());
        }
        main_menu.addItem(&app_menu_item);

        let win_menu = self.add_window_menu(mtm, settings);
        let win_menu_item = NSMenuItem::new(mtm);
        win_menu_item.setSubmenu(Some(&win_menu));
        main_menu.addItem(&win_menu_item);
        app.setWindowsMenu(Some(&win_menu));

        let help_menu = self.add_help_menu(mtm);
        let help_menu_item = NSMenuItem::new(mtm);
        help_menu_item.setSubmenu(Some(&help_menu));
        main_menu.addItem(&help_menu_item);
        app.setHelpMenu(Some(&help_menu));

        Self::remove_system_show_all_tabs(&win_menu);
        app.setMainMenu(Some(&main_menu));
    }

    fn add_window_menu(
        &self,
        mtm: MainThreadMarker,
        settings: &CmdLineSettings,
    ) -> Retained<NSMenu> {
        unsafe {
            let menu = NSMenu::new(mtm);
            menu.setTitle(ns_string!("Window"));
            let delegate: &ProtocolObject<dyn NSMenuDelegate> =
                ProtocolObject::from_ref::<WindowMenuDelegate>(self.window_menu_delegate.as_ref());
            menu.setDelegate(Some(delegate));

            let full_screen_item = NSMenuItem::new(mtm);
            full_screen_item.setTitle(ns_string!("Enter Full Screen"));
            apply_menu_item_hotkey(
                &full_screen_item,
                &settings.system_fullscreen_hotkey,
                "system_fullscreen_hotkey",
            );
            full_screen_item.setAction(Some(sel!(toggleFullScreen:)));
            menu.addItem(&full_screen_item);

            let create_new_window = NSMenuItem::new(mtm);
            create_new_window.setTitle(ns_string!("New Window"));
            apply_menu_item_hotkey(
                &create_new_window,
                &settings.system_new_window_hotkey,
                "system_new_window_hotkey",
            );
            create_new_window.setAction(Some(sel!(neovideCreateWindow:)));
            create_new_window.setTarget(Some(&self.new_window_handler));
            menu.addItem(&create_new_window);

            let show_all_tabs_item = NSMenuItem::new(mtm);
            show_all_tabs_item.setTitle(ns_string!("Editors"));
            apply_menu_item_hotkey(
                &show_all_tabs_item,
                &settings.system_show_all_tabs_hotkey,
                "system_show_all_tabs_hotkey",
            );
            show_all_tabs_item.setAction(Some(sel!(neovideShowEditorSwitcher:)));
            show_all_tabs_item.setTarget(Some(&self.editor_switcher_menu_handler));
            menu.addItem(&show_all_tabs_item);

            let min_item = NSMenuItem::new(mtm);
            min_item.setTitle(ns_string!("Minimize"));
            apply_menu_item_hotkey(
                &min_item,
                &settings.system_minimize_hotkey,
                "system_minimize_hotkey",
            );
            min_item.setAction(Some(sel!(performMiniaturize:)));
            menu.addItem(&min_item);
            menu
        }
    }

    fn add_help_menu(&self, mtm: MainThreadMarker) -> Retained<NSMenu> {
        unsafe {
            let menu = NSMenu::new(mtm);
            menu.setTitle(ns_string!("Help"));

            let create_issue_item = NSMenuItem::new(mtm);
            create_issue_item.setTitle(ns_string!("Create Issue Report"));
            create_issue_item.setAction(Some(sel!(createIssueReport:)));
            create_issue_item.setTarget(Some(&self.help_menu_handler));
            menu.addItem(&create_issue_item);

            let website_item = NSMenuItem::new(mtm);
            website_item.setTitle(ns_string!("Documentation"));
            website_item.setAction(Some(sel!(openNeovideWebsite:)));
            website_item.setTarget(Some(&self.help_menu_handler));
            menu.addItem(&website_item);

            let sponsor_item = NSMenuItem::new(mtm);
            sponsor_item.setTitle(ns_string!("Sponsor"));
            sponsor_item.setAction(Some(sel!(openSponsorPage:)));
            sponsor_item.setTarget(Some(&self.help_menu_handler));
            menu.addItem(&sponsor_item);

            menu
        }
    }

    fn remove_system_show_all_tabs(menu: &NSMenu) {
        let show_native_tabs = should_show_native_tab_bar();
        let item_count = menu.numberOfItems();
        let mut indices_to_remove = Vec::new();
        for idx in 0..item_count {
            let Some(item) = menu.itemAtIndex(idx) else {
                continue;
            };

            let title = item.title();
            let title_ref: &NSString = title.as_ref();
            let action = item.action();

            let is_system_tab_action = action
                .is_some_and(|sel| sel == sel!(mergeAllWindows:) || sel == sel!(toggleTabBar:));

            let is_system_tab_title = title_ref == ns_string!("Merge All Windows")
                || title_ref == ns_string!("Show Tab Bar")
                || title_ref == ns_string!("Hide Tab Bar");

            let should_remove_tab_item =
                !show_native_tabs && (is_system_tab_action || is_system_tab_title);

            let is_show_all_tabs_title = title_ref == ns_string!("Show All Tabs");

            let is_neovide_show_all_tabs_action =
                action.is_some_and(|sel| sel == sel!(neovideShowEditorSwitcher:));

            let should_remove_system_show_all_tabs =
                is_show_all_tabs_title && !is_neovide_show_all_tabs_action;

            if should_remove_tab_item || should_remove_system_show_all_tabs {
                indices_to_remove.push(idx);
            }
        }

        for idx in indices_to_remove.into_iter().rev() {
            menu.removeItemAtIndex(idx);
        }
    }
}

pub fn register_file_handler() {
    fn dispatch_file_drops(filenames: &NSArray<NSString>) {
        for filename in filenames.iter() {
            send_or_queue_file_drop(filename.to_string(), None);
        }
    }

    unsafe extern "C-unwind" fn handle_create_window(
        _this: &AnyObject,
        _sel: objc2::runtime::Sel,
        _sender: &AnyObject,
    ) {
        request_new_window();
    }

    // See signature at
    // https://developer.apple.com/documentation/appkit/nsapplicationdelegate/applicationdockmenu(_:)?language=objc
    unsafe extern "C-unwind" fn handle_application_dock_menu(
        this: &AnyObject,
        _sel: objc2::runtime::Sel,
        _sender: &NSApplication,
    ) -> *mut NSMenu {
        let mtm = MainThreadMarker::new().expect("Dock menu must be built on main thread.");
        let menu = NSMenu::new(mtm);
        let create_new_window = NSMenuItem::new(mtm);
        create_new_window.setTitle(ns_string!("New Window"));
        unsafe {
            create_new_window.setAction(Some(sel!(neovideCreateWindow:)));
            create_new_window.setTarget(Some(this));
        }
        menu.addItem(&create_new_window);
        Retained::autorelease_return(menu)
    }

    // See signature at
    // https://developer.apple.com/documentation/appkit/nsapplicationdelegate/application(_:openfiles:)?language=objc
    unsafe extern "C-unwind" fn handle_open_files(
        _this: &mut AnyObject,
        _sel: objc2::runtime::Sel,
        _sender: &objc2::runtime::AnyObject,
        filenames: &NSArray<NSString>,
    ) {
        dispatch_file_drops(filenames);
        MacosWindowFeature::activate_and_focus_existing_window();
    }

    let mtm = MainThreadMarker::new().expect("File handler must be registered on main thread.");

    unsafe {
        let app = NSApplication::sharedApplication(mtm);
        let delegate = app.delegate().unwrap();

        // Find out class of the NSApplicationDelegate
        let class: &AnyClass = AnyObject::class(delegate.as_ref());

        // register subclass of whatever was in delegate
        let class_name = CString::new("NeovideApplicationDelegate").unwrap();
        let mut my_class = ClassBuilder::new(class_name.as_c_str(), class).unwrap();
        my_class.add_method(
            sel!(application:openFiles:),
            handle_open_files as unsafe extern "C-unwind" fn(_, _, _, _) -> _,
        );

        my_class.add_method(
            sel!(applicationDockMenu:),
            handle_application_dock_menu as unsafe extern "C-unwind" fn(_, _, _) -> _,
        );

        my_class.add_method(
            sel!(neovideCreateWindow:),
            handle_create_window as unsafe extern "C-unwind" fn(_, _, _) -> _,
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
