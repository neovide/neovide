use objc2::MainThreadOnly;
use objc2::define_class;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::runtime::ProtocolObject;
use objc2::sel;
use objc2_app_kit::NSApplication;
use objc2_app_kit::NSEvent;
use objc2_app_kit::NSMenu;
use objc2_app_kit::NSMenuDelegate;
use objc2_app_kit::NSMenuItem;
use objc2_app_kit::NSWindow;
use objc2_app_kit::NSWindowDidBecomeKeyNotification;
use objc2_app_kit::NSWindowDidBecomeMainNotification;
use objc2_app_kit::NSWorkspace;
use objc2_foundation::MainThreadMarker;
use objc2_foundation::NSNotification;
use objc2_foundation::NSNotificationCenter;
use objc2_foundation::NSObject;
use objc2_foundation::NSObjectProtocol;
use objc2_foundation::NSProcessInfo;
use objc2_foundation::NSString;
use objc2_foundation::NSTimer;
use objc2_foundation::NSURL;
use objc2_foundation::ns_string;

use super::request_editor_switcher;
use super::request_new_window;
use crate::bridge::SerialCommand;
use crate::bridge::require_active_handler;
use crate::bridge::send_ui;
use crate::cmd_line::CmdLineSettings;
use crate::settings::Settings;
use crate::window::macos::native_tabs::should_show_native_tab_bar;
use crate::window::macos::shortcut::KeyCombo;

const NEOVIDE_WEBSITE_URL: &str = "https://neovide.dev/";
const NEOVIDE_SPONSOR_URL: &str = "https://github.com/sponsors/neovide";
const NEOVIDE_CREATE_ISSUE_URL: &str =
    "https://github.com/neovide/neovide/issues/new?template=bug_report.md";

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
pub struct Menu {
    quit_handler: Retained<QuitHandler>,
    help_menu_handler: Retained<HelpMenuHandler>,
    new_window_handler: Retained<NewWindowHandler>,
    editor_switcher_menu_handler: Retained<EditorSwitcherMenuHandler>,
    _window_menu_observer: Retained<WindowMenuNotificationHandler>,
    window_menu_delegate: Retained<WindowMenuDelegate>,
}

impl Menu {
    pub fn new(mtm: MainThreadMarker, settings: &Settings) -> Self {
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
