use glamour::Point2;
use icrate::{
    block2::{Block, ConcreteBlock, RcBlock},
    AppKit::{
        self, NSApplication, NSColor, NSEvent, NSEventModifierFlagCommand,
        NSEventModifierFlagControl, NSEventModifierFlagDeviceIndependentFlagsMask,
        NSEventModifierFlagOption, NSEventModifierFlags, NSEventType, NSEventTypePressure, NSFont,
        NSMenu, NSMenuItem, NSTextView, NSView, NSViewMinYMargin, NSViewWidthSizable, NSWindow,
        NSWindowStyleMaskFullScreen, NSWindowStyleMaskTitled, NSWindowTabbingModeDisallowed,
    },
    Foundation::{
        CGFloat, CGPoint, MainThreadMarker, NSArray, NSAttributedString, NSDictionary,
        NSMouseInRect, NSMutableAttributedString, NSMutableDictionary, NSObject, NSPoint,
        NSProcessInfo, NSRange, NSRangePointer, NSRect, NSSize, NSString,
    },
};
use objc2::{
    class, declare_class,
    ffi::id,
    msg_send, msg_send_id,
    mutability::InteriorMutable,
    rc::{Allocated, Id},
    runtime::{AnyClass, AnyObject, Bool, Object, BOOL, NO},
    sel, ClassType,
};

use csscolorparser::Color;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::{dpi::PhysicalPosition, event::DeviceId, window::Window};

use crate::{
    bridge::{send_ui, ParallelCommand},
    units::Pixel,
};
use crate::{cmd_line::CmdLineSettings, error_msg, frame::Frame, settings::SETTINGS};

use super::{WindowSettings, WindowSettingsChanged};

declare_class!(
    // A view to simulate the double-click-to-zoom effect for `--frame transparency`.
    #[derive(Debug)]
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

#[derive(Debug)]
pub struct MacosWindowFeature {
    ns_window: Id<NSWindow>,
    titlebar_click_handler: Option<Id<TitlebarClickHandler>>,
    // Extra titlebar height in --frame transparency. 0 in other cases.
    extra_titlebar_height_in_pixel: u32,
    is_fullscreen: bool,
}

impl MacosWindowFeature {
    pub fn from_winit_window(window: &Window, mtm: MainThreadMarker) -> MacosWindowFeature {
        let ns_window = match window.window_handle().unwrap().as_raw() {
            RawWindowHandle::AppKit(handle) => unsafe {
                let ns_view = handle.ns_view.as_ptr();
                let ns_view: Id<NSView> = Id::retain(ns_view.cast()).unwrap();
                ns_view
                    .window()
                    .expect("view was not installed in a window")
            },
            _ => panic!("Not an appkit window."),
        };
        // Disallow tabbing mode to prevent the window from being tabbed.
        unsafe {
            ns_window.setTabbingMode(NSWindowTabbingModeDisallowed);
        }

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

        let macos_window_feature = MacosWindowFeature {
            ns_window,
            titlebar_click_handler,
            extra_titlebar_height_in_pixel,
            is_fullscreen,
        };

        macos_window_feature.update_background(true);

        macos_window_feature
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

    pub fn handle_size_changed(&mut self) {
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

    /// Print a deprecation warning for `neovide_background_color`
    pub fn display_deprecation_warning(&self) {
        error_msg!(concat!(
            "neovide_background_color has now been deprecated. ",
            "Use neovide_transparency instead if you want to get a transparent window titlebar. ",
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
        unsafe {
            let opaque = alpha >= 1.0;
            let ns_background = NSColor::colorWithSRGBRed_green_blue_alpha(red, green, blue, alpha);
            self.ns_window.setBackgroundColor(Some(&ns_background));
            // If the shadow is enabled and the background color is not transparent, the window will have a grey border
            // Workaround: Disable shadow when `show_border` is false
            self.ns_window.setHasShadow(opaque && show_border);
            // Setting the window to opaque upon creation shows a permanent subtle grey border on the top edge of the window
            self.ns_window.setOpaque(opaque && show_border);
            self.ns_window.invalidateShadow();
        }
    }

    fn update_ns_background(&self, transparency: f32, show_border: bool) {
        unsafe {
            let opaque = transparency >= 1.0;
            // Setting the background color to `NSColor::windowBackgroundColor()`
            // makes the background opaque and draws a grey border around the window
            let ns_background = match opaque && show_border {
                true => NSColor::windowBackgroundColor(),
                false => NSColor::clearColor(),
            };
            self.ns_window.setBackgroundColor(Some(&ns_background));
            self.ns_window.setHasShadow(opaque);
            // Setting the window to opaque upon creation shows a permanent subtle grey border on the top edge of the window
            self.ns_window.setOpaque(opaque && show_border);
            self.ns_window.invalidateShadow();
        }
    }

    /// Update background color, opacity, shadow and blur of a window.
    fn update_background(&self, ignore_deprecation_warning: bool) {
        let WindowSettings {
            background_color,
            show_border,
            transparency,
            ..
        } = SETTINGS.get::<WindowSettings>();
        match background_color.parse::<Color>() {
            Ok(color) => {
                self.update_ns_background_legacy(color, show_border, ignore_deprecation_warning)
            }
            _ => self.update_ns_background(transparency, show_border),
        }
    }

    pub fn handle_settings_changed(&self, changed_setting: WindowSettingsChanged) {
        match changed_setting {
            WindowSettingsChanged::BackgroundColor(background_color) => {
                log::info!("background_color changed to {}", background_color);
                self.update_background(false);
            }
            WindowSettingsChanged::ShowBorder(show_border) => {
                log::info!("show_border changed to {}", show_border);
                self.update_background(true);
            }
            WindowSettingsChanged::Transparency(transparency) => {
                log::info!("transparency changed to {}", transparency);
                self.update_background(true);
            }
            WindowSettingsChanged::WindowBlurred(window_blurred) => {
                log::info!("window_blurred changed to {}", window_blurred);
                self.update_background(true);
            }
            _ => {}
        }
    }

    pub fn handle_touchpad_pressure(&self, text: &str, cursor_position: Point2<Pixel<f32>>) {
        log::info!(
            "Touchpad pressure: text: {}, cursor_position: {:?}",
            text,
            cursor_position
        );

        // if stage == 1 && pressure > 0.5 {
        if cursor_position.y > 0. {
            println!(
                "cursor_position.y: {:?}, cursor_position.x: {:?}",
                cursor_position.y, cursor_position.x
            );

            println!("transleted_point.x: {:?}", cursor_position.x / 2.);
            println!("transleted_point.y: {:?}", cursor_position.y / 2.);

            unsafe {
                let ns_view = self.ns_window.contentView().unwrap();

                // Retrieve the scale factor of the window
                let scale_factor = self.ns_window.backingScaleFactor();
                println!("Scale factor: {}", scale_factor);

                let transleted_point = NSPoint::new(
                    cursor_position.x as f64 / scale_factor,
                    cursor_position.y as f64 / scale_factor,
                );
                println!("transleted_point: {:?}", transleted_point);

                // ns_view.setNeedsDisplay(true);
                // ns_view.display();

                // Create an NSAttributedString with the hardcoded text
                let text = NSString::from_str(text);
                // let attr_string = NSAttributedString::from_nsstring(&text);

                // Create an NSFont with the desired font size
                let font = NSFont::boldSystemFontOfSize(40.0);

                let font_key_any: Id<AnyObject> = Id::cast(font);
                // Create an NSArray with the font attribute
                let fonts: Id<NSArray<AnyObject>> = NSArray::from_vec(vec![font_key_any]);

                // Create an NSString for the key and convert it to AnyObject
                let font_attr_key: Id<NSString> = NSString::from_str("NSFontAttributeName");
                let key_any: Id<AnyObject> = Id::cast(font_attr_key);

                // Create an NSArray with the key
                let keys: Id<NSArray<AnyObject>> = NSArray::from_vec(vec![key_any]);

                // Create an NSDictionary with the font attribute
                let attributes: Id<NSDictionary<NSString, AnyObject>> =
                    NSDictionary::dictionaryWithObjects_forKeys(&fonts, &keys);

                let attr_string_with_font = NSAttributedString::initWithString_attributes(
                    NSAttributedString::alloc(),
                    &text,
                    Some(&attributes),
                );
                // Create an NSRange for the entire length of the string
                let range = NSRange::new(0, text.len());

                let mut mut_attr_string =
                    NSMutableAttributedString::from_attributed_nsstring(&attr_string_with_font);

                // Apply the attributes over the specified range
                // mut_attr_string
                //     .attributesAtIndex_effectiveRange(0, NSRangePointer::from(&mut range));
                // Apply the attributes over the specified range
                mut_attr_string.setAttributes_range(Some(&attributes), range);

                // attr_string.fontAttributesInRange(NSRange::new(10, attr_string.length() + 20));
                ns_view.showDefinitionForAttributedString_atPoint(
                    Some(&mut_attr_string),
                    transleted_point,
                );
            }
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
    pub fn ensure_menu_added(&mut self) {
        if !self.menu_added {
            self.add_menus();
            self.menu_added = true;
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

            let full_screen_item = NSMenuItem::new();
            full_screen_item.setTitle(&NSString::from_str("Enter Full Screen"));
            full_screen_item.setKeyEquivalent(&NSString::from_str("f"));
            full_screen_item.setAction(Some(sel!(toggleFullScreen:)));
            full_screen_item.setKeyEquivalentModifierMask(
                NSEventModifierFlagControl | NSEventModifierFlagCommand,
            );
            menu.addItem(&full_screen_item);

            let min_item = NSMenuItem::new();
            min_item.setTitle(&NSString::from_str("Minimize"));
            min_item.setKeyEquivalent(&NSString::from_str("m"));
            min_item.setAction(Some(sel!(performMiniaturize:)));
            menu.addItem(&min_item);
            menu
        }
    }
}

pub fn register_file_handler() {
    use objc2::rc::autoreleasepool;

    extern "C" fn handle_open_files(
        _this: &mut AnyObject,
        _sel: objc2::runtime::Sel,
        _sender: &objc2::runtime::AnyObject,
        files: &mut icrate::Foundation::NSArray<icrate::Foundation::NSString>,
    ) {
        autoreleasepool(|pool| {
            for file in files.iter() {
                let path = file.as_str(pool).to_owned();
                send_ui(ParallelCommand::FileDrop(path));
            }
        });
    }

    unsafe {
        use objc2::declare::ClassBuilder;
        use objc2::msg_send;

        let app = NSApplication::sharedApplication();
        let delegate = app.delegate().unwrap();

        // Find out class of the NSApplicationDelegate
        let class: &AnyClass = msg_send![&delegate, class];

        // register subclass of whatever was in delegate
        let mut my_class = ClassBuilder::new("NeovideApplicationDelegate", class).unwrap();
        my_class.add_method(
            sel!(application:openFiles:),
            handle_open_files as unsafe extern "C" fn(_, _, _, _) -> _,
        );
        let class = my_class.register();

        // this should be safe as:
        //  * our class is a subclass
        //  * no new ivars
        //  * overriden methods are compatible with old (we implement protocol method)
        let delegate_obj = Id::cast::<AnyObject>(delegate);
        AnyObject::set_class(&delegate_obj, class);
    }
}
