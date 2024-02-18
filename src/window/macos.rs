use icrate::{
    AppKit::{
        NSApplication, NSColor, NSEvent, NSEventModifierFlagCommand, NSEventModifierFlagOption,
        NSMenu, NSMenuItem, NSView, NSViewMinYMargin, NSViewWidthSizable, NSWindow,
        NSWindowStyleMaskFullScreen, NSWindowStyleMaskTitled,
    },
    Foundation::{MainThreadMarker, NSObject, NSPoint, NSProcessInfo, NSRect, NSSize, NSString},
};
use objc2::{declare_class, msg_send_id, mutability::InteriorMutable, rc::Id, sel, ClassType};

use csscolorparser::Color;
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use winit::event::{Event, WindowEvent};
use winit::window::Window;

use crate::bridge::{send_ui, ParallelCommand};
use crate::{
    cmd_line::CmdLineSettings, error_msg, frame::Frame, renderer::WindowedContext,
    settings::SETTINGS, window::UserEvent,
};

use super::WindowSettings;

declare_class!(
    // A view to simulate the double-click-to-zoom effect for `--frame transparency`.
    struct TitlebarClickHandler;

    unsafe impl ClassType for TitlebarClickHandler {
        type Super = NSView;
        type Mutability = InteriorMutable;
        const NAME: &'static str = "TitlebarClickHandler";
    }

    unsafe impl TitlebarClickHandler {
        #[method(mouseDown:)]
        unsafe fn mouse_down(&self, event: &NSEvent) {
            if event.clickCount() == 2 {
                self.window().unwrap().zoom(Some(self));
            }
        }
    }
);

impl TitlebarClickHandler {
    pub fn new(_mtm: MainThreadMarker) -> Id<TitlebarClickHandler> {
        unsafe { msg_send_id![Self::alloc(), init] }
    }
}

lazy_static! {
    // This height is in dpi-independent length, convert it to pixel length by multiplying it with scale factor.
    static ref TITLEBAR_HEIGHT: f64 = MacosWindowFeature::titlebar_height();
}

pub struct MacosWindowFeature {
    ns_window: Id<NSWindow>,
    titlebar_click_handler: Option<Id<TitlebarClickHandler>>,
    // Extra titlebar height in --frame transparency. 0 in other cases.
    extra_titlebar_height_in_pixel: u32,
    is_fullscreen: bool,
}

impl MacosWindowFeature {
    pub fn from_winit_window(window: &Window, mtm: MainThreadMarker) -> MacosWindowFeature {
        let ns_window = match window.raw_window_handle() {
            RawWindowHandle::AppKit(handle) => unsafe {
                Id::retain(handle.ns_window as *mut NSWindow).unwrap()
            },
            _ => panic!("Not an appkit window."),
        };

        if let Ok(color) = &SETTINGS
            .get::<WindowSettings>()
            .background_color
            .parse::<Color>()
        {
            error_msg!(concat!(
                "neovide_background_color has now been deprecated. ",
                "Use neovide_transparency instead if you want to get a transparent window titlebar. ",
                "Please check https://neovide.dev/configuration.html#background-color-deprecated-currently-macos-only for more information.",
            ));

            unsafe {
                let [red, green, blue, alpha] = color.to_array();
                let ns_background =
                    NSColor::colorWithSRGBRed_green_blue_alpha(red, green, blue, alpha);
                ns_window.setBackgroundColor(Some(&ns_background));
            }
        };

        let mut extra_titlebar_height_in_pixel: u32 = 0;

        let frame = SETTINGS.get::<CmdLineSettings>().frame;
        let titlebar_click_handler: Option<Id<TitlebarClickHandler>> = match frame {
            Frame::Transparent => unsafe {
                let titlebar_click_handler = TitlebarClickHandler::new(mtm);

                // Add the titlebar_click_handler into the view of window.
                let content_view = ns_window.contentView().unwrap();
                content_view.addSubview(&titlebar_click_handler);

                // Set the initial size of titlebar_click_handler.
                let content_view_size = content_view.frame().size;
                titlebar_click_handler.setFrame(NSRect::new(
                    NSPoint::new(0., content_view_size.height - *TITLEBAR_HEIGHT),
                    NSSize::new(content_view_size.width, *TITLEBAR_HEIGHT),
                ));

                // Setup auto layout for titlebar_click_handler.
                titlebar_click_handler.setAutoresizingMask(NSViewWidthSizable | NSViewMinYMargin);
                titlebar_click_handler.setTranslatesAutoresizingMaskIntoConstraints(true);

                extra_titlebar_height_in_pixel =
                    Self::titlebar_height_in_pixel(window.scale_factor());

                Some(titlebar_click_handler)
            },
            _ => None,
        };

        let is_fullscreen = unsafe { ns_window.styleMask() } & NSWindowStyleMaskFullScreen != 0;

        MacosWindowFeature {
            ns_window,
            titlebar_click_handler,
            extra_titlebar_height_in_pixel,
            is_fullscreen,
        }
    }

    // Used to calculate the value of TITLEBAR_HEIGHT, aka, titlebar height in dpi-independent length.
    fn titlebar_height() -> f64 {
        // Do a test to calculate this.
        unsafe {
            let mock_content_rect = NSRect::new(NSPoint::new(100., 100.), NSSize::new(100., 100.));
            let frame_rect = NSWindow::frameRectForContentRect_styleMask(
                mock_content_rect,
                NSWindowStyleMaskTitled,
            );
            frame_rect.size.height - mock_content_rect.size.height
        }
    }

    fn titlebar_height_in_pixel(scale_factor: f64) -> u32 {
        (*TITLEBAR_HEIGHT * scale_factor) as u32
    }

    pub fn handle_scale_factor_update(&mut self, scale_factor: f64) {
        // If 0, there needs no extra height.
        if self.extra_titlebar_height_in_pixel != 0 {
            self.extra_titlebar_height_in_pixel = Self::titlebar_height_in_pixel(scale_factor);
        }
    }

    fn set_titlebar_click_handler_visible(&self, visible: bool) {
        if let Some(titlebar_click_handler) = &self.titlebar_click_handler {
            unsafe {
                titlebar_click_handler.setHidden(!visible);
            }
        }
    }

    pub fn handle_size_changed(&mut self, _windowed_context: &WindowedContext) {
        let is_fullscreen =
            unsafe { self.ns_window.styleMask() } & NSWindowStyleMaskFullScreen != 0;
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
}

declare_class!(
    struct QuitHandler;

    unsafe impl ClassType for QuitHandler {
        type Super = NSObject;
        type Mutability = InteriorMutable;
        const NAME: &'static str = "QuitHandler";
    }

    unsafe impl QuitHandler {
        #[method(quit:)]
        unsafe fn quit(&self, _event: &NSEvent) {
            send_ui(ParallelCommand::Quit);
        }
    }
);

impl QuitHandler {
    pub fn new(_mtm: MainThreadMarker) -> Id<QuitHandler> {
        unsafe { msg_send_id![Self::alloc(), init] }
    }
}

pub struct Menu {
    menu_added: bool,
    quit_handler: Id<QuitHandler>,
}

impl Menu {
    pub fn new(mtm: MainThreadMarker) -> Self {
        Menu {
            menu_added: false,
            quit_handler: QuitHandler::new(mtm),
        }
    }
    pub fn ensure_menu_added(&mut self, ev: &Event<UserEvent>) {
        if let Event::WindowEvent {
            event: WindowEvent::Focused(_),
            ..
        } = ev
        {
            if !self.menu_added {
                self.add_menus();
                self.menu_added = true;
            }
        }
    }

    fn add_app_menu(&self) -> Id<NSMenu> {
        unsafe {
            let app_menu = NSMenu::new();
            let process_name = NSProcessInfo::processInfo().processName();
            let about_item = NSMenuItem::new();
            about_item
                .setTitle(&NSString::from_str("About ").stringByAppendingString(&process_name));
            about_item.setAction(Some(sel!(orderFrontStandardAboutPanel:)));
            app_menu.addItem(&about_item);

            let services_item = NSMenuItem::new();
            let services_menu = NSMenu::new();
            services_item.setTitle(&NSString::from_str("Services"));
            services_item.setSubmenu(Some(&services_menu));
            app_menu.addItem(&services_item);

            let sep = NSMenuItem::separatorItem();
            app_menu.addItem(&sep);

            // application window operations
            let hide_item = NSMenuItem::new();
            hide_item.setTitle(&NSString::from_str("Hide ").stringByAppendingString(&process_name));
            hide_item.setKeyEquivalent(&NSString::from_str("h"));
            hide_item.setAction(Some(sel!(hide:)));
            app_menu.addItem(&hide_item);

            let hide_others_item = NSMenuItem::new();
            hide_others_item.setTitle(&NSString::from_str("Hide Others"));
            hide_others_item.setKeyEquivalent(&NSString::from_str("h"));
            hide_others_item.setKeyEquivalentModifierMask(
                NSEventModifierFlagOption | NSEventModifierFlagCommand,
            );
            hide_others_item.setAction(Some(sel!(hideOtherApplications:)));
            app_menu.addItem(&hide_others_item);

            let show_all_item = NSMenuItem::new();
            show_all_item.setTitle(&NSString::from_str("Show All"));
            show_all_item.setAction(Some(sel!(unhideAllApplications:)));

            // quit
            let sep = NSMenuItem::separatorItem();
            app_menu.addItem(&sep);

            let quit_item = NSMenuItem::new();
            quit_item.setTitle(&NSString::from_str("Quit ").stringByAppendingString(&process_name));
            quit_item.setKeyEquivalent(&NSString::from_str("q"));
            quit_item.setAction(Some(sel!(quit:)));
            quit_item.setTarget(Some(&self.quit_handler));
            app_menu.addItem(&quit_item);

            app_menu
        }
    }

    fn add_menus(&self) {
        let app = unsafe { NSApplication::sharedApplication() };

        let main_menu = unsafe { NSMenu::new() };

        unsafe {
            let app_menu = self.add_app_menu();
            let app_menu_item = NSMenuItem::new();
            app_menu_item.setSubmenu(Some(&app_menu));
            if let Some(services_menu) = app_menu.itemWithTitle(&NSString::from_str("Services")) {
                app.setServicesMenu(services_menu.submenu().as_deref());
            }
            main_menu.addItem(&app_menu_item);

            let win_menu = self.add_window_menu();
            let win_menu_item = NSMenuItem::new();
            win_menu_item.setSubmenu(Some(&win_menu));
            main_menu.addItem(&win_menu_item);
            app.setWindowsMenu(Some(&win_menu));
        }

        unsafe { app.setMainMenu(Some(&main_menu)) };
    }

    fn add_window_menu(&self) -> Id<NSMenu> {
        let menu_title = NSString::from_str("Window");
        unsafe {
            let menu = NSMenu::new();
            menu.setTitle(&menu_title);

            let min_item = NSMenuItem::new();
            min_item.setTitle(&NSString::from_str("Minimize"));
            min_item.setKeyEquivalent(&NSString::from_str("m"));
            min_item.setAction(Some(sel!(performMiniaturize:)));
            menu.addItem(&min_item);
            menu
        }
    }
}
