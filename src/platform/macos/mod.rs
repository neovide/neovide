pub mod settings;
pub mod keyboard;

use std::sync::Arc;
use std::{os::raw::c_void, str};

use objc2::{
    define_class, msg_send,
    rc::Retained,
    runtime::{AnyClass, AnyObject, ClassBuilder},
    sel, AnyThread, MainThreadOnly,
};
use objc2_app_kit::{
    NSApplication, NSAutoresizingMaskOptions, NSColor, NSEvent, NSEventModifierFlags, NSImage,
    NSMenu, NSMenuItem, NSView, NSWindow, NSWindowStyleMask, NSWindowTabbingMode,
};
use objc2_foundation::{
    ns_string, MainThreadMarker, NSArray, NSData, NSDictionary, NSObject, NSPoint, NSProcessInfo,
    NSRect, NSSize, NSString, NSUserDefaults,
};

use csscolorparser::Color;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

use crate::{
    bridge::{send_ui, ParallelCommand, SerialCommand},
    settings::Settings,
};
use crate::{cmd_line::CmdLineSettings, error_msg, frame::Frame};

use crate::window::{WindowSettings, WindowSettingsChanged};

static NEOVIDE_ICON_PATH: &[u8] =
    include_bytes!("../../../extra/osx/Neovide.app/Contents/Resources/Neovide.icns");

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

fn load_neovide_icon() -> Option<Retained<NSImage>> {
    unsafe {
        let data = NSData::dataWithBytes_length(
            NEOVIDE_ICON_PATH.as_ptr() as *mut c_void,
            NEOVIDE_ICON_PATH.len(),
        );

        let icon_image: Option<Retained<NSImage>> =
            NSImage::initWithData(NSImage::alloc(), data.as_ref());

        icon_image
    }
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
}

impl MacosWindowFeature {
    pub fn from_winit_window(window: &Window, settings: Arc<Settings>) -> Self {
        let mtm =
            MainThreadMarker::new().expect("MacosWindowFeature must be created in main thread.");

        let system_titlebar_height = Self::system_titlebar_height(mtm);

        let ns_window = get_ns_window(window);

        // Disallow tabbing mode to prevent the window from being tabbed.
        ns_window.setTabbingMode(NSWindowTabbingMode::Disallowed);

        let mut extra_titlebar_height_in_pixel: u32 = 0;

        let frame = settings.get::<CmdLineSettings>().frame;
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

        let macos_window_feature = MacosWindowFeature {
            ns_window,
            system_titlebar_height,
            titlebar_click_handler,
            extra_titlebar_height_in_pixel,
            is_fullscreen,
            menu: None,
            settings: settings.clone(),
        };

        macos_window_feature.update_background(true);

        macos_window_feature
    }

    // Used to calculate the value of TITLEBAR_HEIGHT, aka, titlebar height in dpi-independent length.
    fn system_titlebar_height(mtm: MainThreadMarker) -> f64 {
        // Do a test to calculate this.
        let mock_content_rect = NSRect::new(NSPoint::new(100., 100.), NSSize::new(100., 100.));
        let frame_rect = {
            NSWindow::frameRectForContentRect_styleMask(
                mock_content_rect,
                NSWindowStyleMask::Titled,
                mtm,
            )
        };
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
    pub fn extra_titlebar_height_in_pixels(&self) -> u32 {
        if self.is_fullscreen {
            0
        } else {
            self.extra_titlebar_height_in_pixel
        }
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

    /// Create the application menu and grab initial focus.
    pub fn ensure_app_initialized(&mut self) {
        let mtm = MainThreadMarker::new().expect("Menu must be created on the main thread");
        if self.menu.is_none() {
            self.menu = Some(Menu::new(mtm));
            let app = NSApplication::sharedApplication(mtm);
            #[allow(deprecated)]
            app.activateIgnoringOtherApps(true);

            // Make sure the icon is loaded when launched from terminal
            let icon = load_neovide_icon();
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
            send_ui(SerialCommand::Keyboard("<D-q>".into()));
        }
    }
);

impl QuitHandler {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        unsafe { msg_send![Self::alloc(mtm), init] }
    }
}

#[derive(Debug)]
struct Menu {
    quit_handler: Retained<QuitHandler>,
}

impl Menu {
    fn new(mtm: MainThreadMarker) -> Self {
        let menu = Menu {
            quit_handler: QuitHandler::new(mtm),
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
        app.setMainMenu(Some(&main_menu));
    }

    fn add_window_menu(&self, mtm: MainThreadMarker) -> Retained<NSMenu> {
        unsafe {
            let menu = NSMenu::new(mtm);
            menu.setTitle(ns_string!("Window"));

            let full_screen_item = NSMenuItem::new(mtm);
            full_screen_item.setTitle(ns_string!("Enter Full Screen"));
            full_screen_item.setKeyEquivalent(ns_string!("f"));
            full_screen_item.setAction(Some(sel!(toggleFullScreen:)));
            full_screen_item.setKeyEquivalentModifierMask(
                NSEventModifierFlags::Control | NSEventModifierFlags::Command,
            );
            menu.addItem(&full_screen_item);

            let min_item = NSMenuItem::new(mtm);
            min_item.setTitle(ns_string!("Minimize"));
            min_item.setKeyEquivalent(ns_string!("m"));
            min_item.setAction(Some(sel!(performMiniaturize:)));
            menu.addItem(&min_item);
            menu
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
        for filename in filenames.iter() {
            send_ui(ParallelCommand::FileDrop(filename.to_string()));
        }
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
