pub mod settings;

use std::{cell::RefCell, ffi::CString, os::raw::c_void, path::Path, ptr, str, sync::Arc};

use glamour::Point2;
use objc2::{
    class, define_class, msg_send,
    rc::Retained,
    runtime::{AnyClass, AnyObject, ClassBuilder},
    sel, AnyThread, MainThreadOnly,
};
use objc2_app_kit::{
    NSApplication, NSAutoresizingMaskOptions, NSColor, NSEvent, NSEventModifierFlags, NSFont,
    NSFontAttributeName, NSFontDescriptor, NSFontWeight, NSFontWeightLight, NSImage, NSMenu,
    NSMenuItem, NSTextView, NSView, NSWindow, NSWindowStyleMask, NSWindowTabbingMode, NSWorkspace,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{
    ns_string, MainThreadMarker, NSArray, NSAttributedString, NSData, NSDictionary, NSInteger,
    NSObject, NSPoint, NSProcessInfo, NSRange, NSRect, NSSize, NSString, NSUserDefaults, NSURL,
};

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

use crate::utils::expand_tilde;
use crate::{
    bridge::{send_ui, ParallelCommand, SerialCommand},
    renderer::fonts::font_options::FontOptions,
    settings::Settings,
};
use crate::{cmd_line::CmdLineSettings, frame::Frame};

use crate::units::{Pixel, PixelRect};
#[cfg(target_os = "macos")]
use crate::window::ForceClickKind;
use crate::window::{WindowSettings, WindowSettingsChanged};

#[link(name = "Quartz", kind = "framework")]
extern "C" {}

static DEFAULT_NEOVIDE_ICON_BYTES: &[u8] =
    include_bytes!("../../../extra/osx/Neovide.app/Contents/Resources/Neovide.icns");

const NEOVIDE_WEBSITE_URL: &str = "https://neovide.dev/";
const NEOVIDE_SPONSOR_URL: &str = "https://github.com/sponsors/neovide";
const NEOVIDE_CREATE_ISSUE_URL: &str =
    "https://github.com/neovide/neovide/issues/new?template=bug_report.md";

thread_local! {
    static QUICKLOOK_PREVIEW_ITEM: RefCell<Option<Retained<NSURL>>> = const { RefCell::new(None) };
    static QUICKLOOK_CONTROLLER: RefCell<Option<Retained<QuickLookPreviewController>>> =
        const { RefCell::new(None) };
}

pub enum TouchpadStage {
    Soft,
    Click,
    ForceClick,
}

impl TouchpadStage {
    pub fn from_stage(stage: i64) -> TouchpadStage {
        match stage {
            0 => TouchpadStage::Soft,
            1 => TouchpadStage::Click,
            2 => TouchpadStage::ForceClick,
            _ => panic!("Invalid touchpad stage"),
        }
    }
}

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

define_class!(
    #[derive(Debug)]
    #[unsafe(super = NSTextView)]
    #[thread_kind = MainThreadOnly]
    struct MatchParenIndicatorView;

    impl MatchParenIndicatorView {
        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            false
        }

        #[unsafe(method(hitTest:))]
        fn hit_test(&self, _point: NSPoint) -> *mut NSView {
            std::ptr::null_mut()
        }
    }
);

impl MatchParenIndicatorView {
    fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
        unsafe { msg_send![Self::alloc(mtm), initWithFrame: frame] }
    }
}

define_class!(
    #[derive(Debug)]
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    struct QuickLookPreviewController;

    impl QuickLookPreviewController {
        #[unsafe(method(numberOfPreviewItemsInPreviewPanel:))]
        fn number_of_preview_items(&self, _panel: *mut AnyObject) -> NSInteger {
            QUICKLOOK_PREVIEW_ITEM.with(|cell| {
                if cell.borrow().is_some() {
                    1
                } else {
                    0
                }
            })
        }

        #[unsafe(method(previewPanel:previewItemAtIndex:))]
        fn preview_item_at_index(
            &self,
            _panel: *mut AnyObject,
            _index: NSInteger,
        ) -> *mut AnyObject {
            QUICKLOOK_PREVIEW_ITEM.with(|cell| {
                cell.borrow()
                    .as_ref()
                    .map(|item| Retained::<NSURL>::as_ptr(item) as *mut AnyObject)
                    .unwrap_or(ptr::null_mut())
            })
        }
    }
);

impl QuickLookPreviewController {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        unsafe { msg_send![Self::alloc(mtm), init] }
    }

    fn shared(mtm: MainThreadMarker) -> Retained<Self> {
        QUICKLOOK_CONTROLLER.with(|cell| {
            if let Some(controller) = cell.borrow().as_ref() {
                return controller.clone();
            }

            let controller = Self::new(mtm);
            *cell.borrow_mut() = Some(controller.clone());
            controller
        })
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
        .and_then(|path| {
            let expanded = expand_tilde(path);
            load_icon_from_custom_path(&expanded)
        })
        .or_else(load_icon_from_default_bytes)
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

#[derive(Debug)]
pub struct MacosWindowFeature {
    ns_window: Retained<NSWindow>,
    pub system_titlebar_height: f64,
    titlebar_click_handler: Option<Retained<TitlebarClickHandler>>,
    // Extra titlebar height in --frame transparency. 0 in other cases.
    extra_titlebar_height_in_pixel: u32,
    is_fullscreen: bool,
    menu: Option<Menu>,
    settings: Arc<Settings>,
    pub definition_is_active: bool,
    match_paren_indicator_view: Option<Retained<MatchParenIndicatorView>>,
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
            definition_is_active: false,
            match_paren_indicator_view: None,
        };

        macos_window_feature.update_background();

        macos_window_feature
    }

    fn activate_app_and_focus_window(window: &NSWindow) {
        let mtm = MainThreadMarker::new().expect("Window activation must be on the main thread.");
        let app = NSApplication::sharedApplication(mtm);
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
        window.makeKeyAndOrderFront(None);
    }

    pub fn activate_and_focus(&self) {
        Self::activate_app_and_focus_window(&self.ns_window);
    }

    fn focus_target_window(app: &NSApplication) -> Option<Retained<NSWindow>> {
        app.mainWindow()
            .or_else(|| app.keyWindow())
            .or_else(|| app.windows().firstObject())
    }

    pub fn activate_and_focus_existing_window() -> bool {
        let Some(mtm) = MainThreadMarker::new() else {
            return false;
        };

        let app = NSApplication::sharedApplication(mtm);
        Self::focus_target_window(&app)
            .map(|window| Self::activate_app_and_focus_window(&window))
            .is_some()
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

    pub fn set_definition_is_active(&mut self, is_active: bool) {
        self.definition_is_active = is_active;
    }

    fn preview_file(&self, entity: &str) -> bool {
        if entity.is_empty() {
            return false;
        }

        let expanded = expand_tilde(entity);
        let path = Path::new(&expanded);
        if !path.exists() {
            return false;
        }

        let Some(mtm) = MainThreadMarker::new() else {
            return false;
        };

        unsafe {
            let ns_path = NSString::from_str(&expanded);
            let url = NSURL::fileURLWithPath(&ns_path);
            self.present_quicklook_item(url, mtm)
        }
    }

    fn preview_url(&self, url: &str) -> bool {
        if url.is_empty() {
            return false;
        }
        let Some(mtm) = MainThreadMarker::new() else {
            return false;
        };

        let ns_url_string = NSString::from_str(url);

        if let Some(ns_url) = NSURL::URLWithString(&ns_url_string) {
            return unsafe { self.present_quicklook_item(ns_url, mtm) };
        }
        false
    }

    unsafe fn present_quicklook_item(&self, url: Retained<NSURL>, mtm: MainThreadMarker) -> bool {
        QUICKLOOK_PREVIEW_ITEM.with(|cell| {
            *cell.borrow_mut() = Some(url);
        });

        let controller = QuickLookPreviewController::shared(mtm);

        let panel: *mut AnyObject = msg_send![class!(QLPreviewPanel), sharedPreviewPanel];
        if panel.is_null() {
            return false;
        }

        let controller_ref: &QuickLookPreviewController = controller.as_ref();

        let _: () = msg_send![panel, setDataSource: controller_ref];
        let _: () = msg_send![panel, setDelegate: controller_ref];
        let _: () = msg_send![panel, reloadData];
        let _: () = msg_send![panel, makeKeyAndOrderFront: controller_ref];

        true
    }

    pub fn handle_force_click_target(
        &mut self,
        entity: &str,
        kind: ForceClickKind,
        point: Point2<Pixel<f32>>,
        guifont: String,
        cell_height_px: f32,
    ) {
        let handled = match kind {
            ForceClickKind::Url => self.preview_url(entity),
            ForceClickKind::File => self.preview_file(entity),
            ForceClickKind::Text => false,
        };

        if handled {
            self.set_definition_is_active(false);
            return;
        }

        self.show_definition_at_point(entity, point, guifont, cell_height_px);
        self.set_definition_is_active(true);
    }

    pub fn handle_touchpad_force_click(&self) {
        if self.definition_is_active {
            return;
        }

        send_ui(SerialCommand::ForceClickCommand);
    }

    pub fn show_definition_at_point(
        &self,
        text: &str,
        point: Point2<Pixel<f32>>,
        guifont: String,
        cell_height_px: f32,
    ) {
        if text.is_empty() {
            return;
        }

        let (font_size, requested_family) = Self::definition_font_request(&guifont, cell_height_px);

        unsafe {
            let ns_view = self.ns_window.contentView().unwrap();
            let translated_point = self.definition_point(point);
            let attr_string =
                Self::definition_attr_string(text, font_size, requested_family.as_deref());

            ns_view.showDefinitionForAttributedString_atPoint(
                Some(attr_string.as_ref()),
                translated_point,
            );
        }
    }

    pub fn show_find_indicator_for_rect(&mut self, rect: PixelRect<f32>, text: Option<&str>) {
        // just being defensive here in case of an invalid state.
        let width = rect.max.x - rect.min.x;
        let height = rect.max.y - rect.min.y;
        if width <= 0.0 || height <= 0.0 {
            return;
        }

        unsafe {
            let ns_view = self.ns_window.contentView().unwrap();
            let scale = self.ns_window.backingScaleFactor();
            let size = NSSize::new(width as f64 / scale, height as f64 / scale);
            let mut origin = NSPoint::new(rect.min.x as f64 / scale, rect.min.y as f64 / scale);

            // future-proof for being defensive here.
            //
            // NSView flipped macOS standard value is false,
            // https://developer.apple.com/documentation/appkit/nsview/isflipped
            //
            // but winit flips it since it uses the upper-left corner as the origin.
            // https://docs.rs/crate/winit-appkit/0.31.0-beta.2/source/src/view.rs#149-153
            if !ns_view.isFlipped() {
                let view_height = ns_view.bounds().size.height;
                origin.y = view_height - origin.y - size.height;
            }

            let ns_rect = NSRect::new(origin, size);
            self.show_match_paren_indicator(ns_view.as_ref(), ns_rect, text)
        }
    }

    unsafe fn show_match_paren_indicator(
        &mut self,
        ns_view: &NSView,
        rect: NSRect,
        text: Option<&str>,
    ) {
        let text = match text {
            Some(text) if !text.is_empty() => text,
            _ => return,
        };

        let indicator_view = self.ensure_match_paren_indicator_view(ns_view, rect);
        indicator_view.setFrame(rect);

        let ns_text = NSString::from_str(text);
        indicator_view.setString(&ns_text);

        let font_size = (rect.size.height * 0.85).max(1.0);
        let font =
            NSFont::monospacedSystemFontOfSize_weight(CGFloat::from(font_size), NSFontWeightLight);

        indicator_view.setFont(Some(font.as_ref()));
        indicator_view.setTextColor(Some(NSColor::textColor().as_ref()));

        let show_range_selector = sel!(showFindIndicatorForRange:);
        let can_show = msg_send![&*indicator_view, respondsToSelector: show_range_selector];
        if can_show {
            let length = text.encode_utf16().count();
            indicator_view.showFindIndicatorForRange(NSRange::new(0, length));
            let clear_color = NSColor::clearColor();
            let _: () = msg_send![
                &*indicator_view,
                performSelector: sel!(setTextColor:),
                withObject: clear_color.as_ref() as *const NSColor,
                afterDelay: 0.35
            ];
        }
    }

    fn ensure_match_paren_indicator_view(
        &mut self,
        ns_view: &NSView,
        rect: NSRect,
    ) -> Retained<MatchParenIndicatorView> {
        if let Some(view) = self.match_paren_indicator_view.as_ref() {
            return view.clone();
        }

        let mtm = MainThreadMarker::new()
            .expect("MatchParen indicator must be created on the main thread.");
        let view = MatchParenIndicatorView::new(mtm, rect);
        self.setup_match_paren_indicator_view(&view);

        ns_view.addSubview(&view);
        self.match_paren_indicator_view = Some(view.clone());

        view
    }

    fn setup_match_paren_indicator_view(&self, view: &MatchParenIndicatorView) {
        view.setEditable(false);
        view.setSelectable(false);
        view.setDrawsBackground(false);
        view.setTextContainerInset(NSSize::new(0.0, 0.0));
        view.setString(ns_string!(""));
        view.setTextColor(Some(NSColor::clearColor().as_ref()));

        if let Some(container) = unsafe { view.textContainer() } {
            container.setLineFragmentPadding(CGFloat::from(0.0));
        }
    }

    fn definition_font_request(guifont: &str, cell_height_px: f32) -> (f64, Option<String>) {
        let options = FontOptions::parse(guifont).unwrap_or_default();
        let font_size = if options.size > 0.0 {
            options.size
        } else {
            cell_height_px
        } as f64;
        let requested_family = options.normal.first().map(|font| font.family.to_string());
        (font_size, requested_family)
    }

    unsafe fn definition_attr_string(
        text: &str,
        font_size: f64,
        requested_family: Option<&str>,
    ) -> Retained<NSAttributedString> {
        let default_font = NSFont::monospacedSystemFontOfSize_weight(
            CGFloat::from(font_size),
            NSFontWeight::from(5),
        );

        let font_name_string = requested_family
            .map(|name| name.to_string())
            .unwrap_or_else(|| NSFont::fontName(default_font.as_ref()).to_string());
        let font_name = NSString::from_str(&font_name_string);
        let font_descriptor = NSFontDescriptor::fontDescriptorWithName_size(&font_name, font_size);

        // prefer the requested font; fall back to the descriptor, then a monospaced default
        // to keep size sane.
        let font = NSFont::fontWithDescriptor_size(&font_descriptor, font_size)
            .or_else(|| NSFont::fontWithName_size(&font_name, font_size))
            .unwrap_or_else(|| default_font.clone());

        let attributes = NSDictionary::from_slices(&[NSFontAttributeName], &[font.as_ref()]);
        let attr_text = NSString::from_str(text);
        NSAttributedString::new_with_attributes(&attr_text, attributes.as_ref())
    }

    unsafe fn definition_point(&self, point: Point2<Pixel<f32>>) -> NSPoint {
        let scale_factor = self.ns_window.backingScaleFactor();
        NSPoint::new(point.x as f64 / scale_factor, point.y as f64 / scale_factor)
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
    fn update_background(&self) {
        let WindowSettings {
            show_border,
            opacity,
            normal_opacity,
            ..
        } = self.settings.get::<WindowSettings>();
        let opaque = opacity.min(normal_opacity) >= 1.0;
        self.update_ns_background(opaque, show_border);
    }

    pub fn handle_settings_changed(&mut self, changed_setting: WindowSettingsChanged) {
        match changed_setting {
            WindowSettingsChanged::ShowBorder(show_border) => {
                log::info!("show_border changed to {show_border}");
                self.update_background();
            }
            WindowSettingsChanged::Opacity(opacity) => {
                log::info!("opacity changed to {opacity}");
                self.update_background();
            }
            WindowSettingsChanged::NormalOpacity(normal_opacity) => {
                log::info!("normal_opacity changed to {normal_opacity}");
                self.update_background();
            }
            WindowSettingsChanged::WindowBlurred(window_blurred) => {
                log::info!("window_blurred changed to {window_blurred}");
                self.update_background();
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
            send_ui(SerialCommand::Keyboard("<D-q>".into()));
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
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        unsafe { msg_send![Self::alloc(mtm), init] }
    }
}

#[derive(Debug)]
struct Menu {
    quit_handler: Retained<QuitHandler>,
    help_menu_handler: Retained<HelpMenuHandler>,
}

impl Menu {
    fn new(mtm: MainThreadMarker) -> Self {
        let menu = Menu {
            quit_handler: QuitHandler::new(mtm),
            help_menu_handler: HelpMenuHandler::new(mtm),
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

        let help_menu = self.add_help_menu(mtm);
        let help_menu_item = NSMenuItem::new(mtm);
        help_menu_item.setSubmenu(Some(&help_menu));
        main_menu.addItem(&help_menu_item);
        app.setHelpMenu(Some(&help_menu));

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
}

pub fn register_file_handler() {
    fn dispatch_file_drops(filenames: &NSArray<NSString>) {
        for filename in filenames.iter() {
            send_ui(ParallelCommand::FileDrop(filename.to_string()));
        }
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
