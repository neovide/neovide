#[path = "scrollbar/mod.rs"]
pub mod scrollbar;
#[path = "tabs/mod.rs"]
mod tabs;

use std::cell::RefCell;
use std::path::Path;
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use glamour::Point2;
use objc2::MainThreadOnly;
use objc2::class;
use objc2::define_class;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::sel;
use objc2_app_kit::NSApplication;
use objc2_app_kit::NSAutoresizingMaskOptions;
use objc2_app_kit::NSColor;
use objc2_app_kit::NSEvent;
use objc2_app_kit::NSFont;
use objc2_app_kit::NSFontAttributeName;
use objc2_app_kit::NSFontDescriptor;
use objc2_app_kit::NSFontWeight;
use objc2_app_kit::NSFontWeightLight;
use objc2_app_kit::NSTextView;
use objc2_app_kit::NSView;
use objc2_app_kit::NSWindow;
use objc2_app_kit::NSWindowOrderingMode;
use objc2_app_kit::NSWindowStyleMask;
use objc2_app_kit::NSWindowTabbingMode;
use objc2_app_kit::NSWindowTitleVisibility;
use objc2_core_foundation::CGFloat;
use objc2_foundation::MainThreadMarker;
use objc2_foundation::NSAttributedString;
use objc2_foundation::NSDictionary;
use objc2_foundation::NSInteger;
use objc2_foundation::NSObject;
use objc2_foundation::NSPoint;
use objc2_foundation::NSRange;
use objc2_foundation::NSRect;
use objc2_foundation::NSSize;
use objc2_foundation::NSString;
use objc2_foundation::NSURL;
use objc2_foundation::ns_string;
use winit::window::Window;

use self::scrollbar::WindowScrollbars;
use self::tabs::WindowSystemTabs;
use self::tabs::supports_system_tabs;
use super::HostTab;
use super::HostTabSize;
use super::tab_group_contains_window;
use crate::bridge::NeovimHandler;
use crate::bridge::SerialCommand;
use crate::bridge::send_ui;
use crate::cmd_line::CmdLineSettings;
use crate::frame::Frame;
use crate::renderer::fonts::font_options::FontOptions;
use crate::settings::Settings;
use crate::units::Pixel;
use crate::units::PixelRect;
use crate::utils::expand_tilde;
use crate::window::ForceClickKind;
use crate::window::WindowSettings;
use crate::window::WindowSettingsChanged;
use crate::window::application::platform::ensure_app_initialized;
use crate::window::macos::appkit::activate_app_and_focus_window;
use crate::window::macos::appkit::get_ns_window;
use crate::window::macos::system_tabs::ENABLE_SYSTEM_TAB_BAR;
use crate::window::macos::system_tabs::SHOW_SYSTEM_TAB_BAR;
use crate::window::macos::system_tabs::should_show_system_tab_bar;

thread_local! {
    static QUICKLOOK_PREVIEW_ITEM: RefCell<Option<Retained<NSURL>>> = const { RefCell::new(None) };
    static QUICKLOOK_CONTROLLER: RefCell<Option<Retained<QuickLookPreviewController>>> =
        const { RefCell::new(None) };
}

const NEOVIDE_TABBING_IDENTIFIER: &str = "NeovideWindowTabGroup";
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
    fn new(mtm: MainThreadMarker) -> Retained<Self> { unsafe { msg_send![Self::alloc(mtm), init] } }
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
    fn new(mtm: MainThreadMarker) -> Retained<Self> { unsafe { msg_send![Self::alloc(mtm), init] } }

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

#[derive(Debug)]
pub struct MacosWindowFeature {
    ns_window: Retained<NSWindow>,
    pub system_titlebar_height: f64,
    titlebar_click_handler: Option<Retained<TitlebarClickHandler>>,
    // Extra titlebar height in --frame transparency. 0 in other cases.
    extra_titlebar_height_in_pixel: u32,
    is_fullscreen: bool,
    settings: Arc<Settings>,
    pub definition_is_active: bool,
    match_paren_indicator_view: Option<Retained<MatchParenIndicatorView>>,
    neovim_handler: NeovimHandler,
    simple_fullscreen: bool,
    has_transparent_titlebar: bool,
    system_tabs: Option<WindowSystemTabs>,
    scrollbars: WindowScrollbars,
}

impl MacosWindowFeature {
    pub(super) fn from_winit_window(
        window: &Window,
        settings: Arc<Settings>,
        neovim_handler: NeovimHandler,
        host_tab: Option<HostTab>,
    ) -> Self {
        let mtm =
            MainThreadMarker::new().expect("MacosWindowFeature must be created in main thread.");

        let system_titlebar_height = Self::system_titlebar_height(mtm);

        let ns_window = get_ns_window(window);
        let cmd = settings.get::<CmdLineSettings>();
        let frame_style = cmd.frame;
        let window_settings = settings.get::<WindowSettings>();
        let simple_fullscreen = window_settings.macos_simple_fullscreen;
        let (allow_system_tab, system_tab_requested) = Self::set_tab_flags(&cmd, simple_fullscreen);

        let host_tab_size = host_tab.as_ref().and_then(|host| host.size);

        // Full frames can reuse the host content height because tab bar attachment
        // does not add a grid inset. Transparent frames measure a live inset from
        // the bar, so equal content heights can still yield different grid rows.
        let pending_host_height = match (frame_style, host_tab_size) {
            (Frame::Full, Some(size)) => Some(size.content_height),
            _ => None,
        };

        let system_tabs = system_tab_requested.then(|| {
            WindowSystemTabs::install(ns_window.clone(), mtm, cmd.title_hidden, pending_host_height)
        });

        // Match the host outer size before AppKit attaches the new tab, while
        // preserving the new window top edge in AppKit frame coordinates.
        if system_tab_requested && let Some(host_size) = &host_tab_size {
            let mut window_frame = ns_window.frame();
            let top_edge = window_frame.origin.y + window_frame.size.height;
            window_frame.size = NSSize::new(host_size.outer.width, host_size.outer.height);
            window_frame.origin.y = top_edge - window_frame.size.height;
            ns_window.setFrame_display(window_frame, false);
        }

        if allow_system_tab {
            Self::configure_system_tabbing(&ns_window);
        } else {
            ns_window.setTabbingMode(NSWindowTabbingMode::Disallowed);
        }

        // A tab request implies that tabbing was allowed before the host join.
        let host_join_blocked = match &host_tab {
            Some(host) if system_tab_requested => {
                !Self::join_host_tab_group(&ns_window, &host.window)
            }
            _ => false,
        };

        if host_join_blocked && let Some(tabs) = &system_tabs {
            tabs.cancel_host_height_restore();
        }

        let mut extra_titlebar_height_in_pixel: u32 = 0;
        let has_transparent_titlebar = matches!(frame_style, Frame::Transparent);
        let titlebar_click_handler: Option<Retained<TitlebarClickHandler>> = if !simple_fullscreen {
            match frame_style {
                Frame::Transparent => {
                    extra_titlebar_height_in_pixel = Self::titlebar_height_in_pixel(
                        system_titlebar_height,
                        window.scale_factor(),
                    );
                    Self::install_titlebar_click_handler(
                        &ns_window,
                        system_titlebar_height,
                        window.scale_factor(),
                        mtm,
                    )
                }
                _ => None,
            }
        } else {
            None
        };

        let is_fullscreen = ns_window.styleMask().contains(NSWindowStyleMask::FullScreen);
        let macos_window_feature = MacosWindowFeature {
            ns_window,
            system_titlebar_height,
            titlebar_click_handler,
            extra_titlebar_height_in_pixel,
            is_fullscreen,
            settings: settings.clone(),
            definition_is_active: false,
            match_paren_indicator_view: None,
            neovim_handler,
            simple_fullscreen,
            has_transparent_titlebar,
            system_tabs,
            scrollbars: WindowScrollbars::default(),
        };

        let mut macos_window_feature = macos_window_feature;
        macos_window_feature.set_title_hidden(cmd.title_hidden);
        macos_window_feature.set_simple_fullscreen_mode(simple_fullscreen);
        macos_window_feature.update_background();

        macos_window_feature
    }

    fn set_tab_flags(cmd_line_settings: &CmdLineSettings, simple_fullscreen: bool) -> (bool, bool) {
        let allow_system_tab = supports_system_tabs(cmd_line_settings.frame) && !simple_fullscreen;
        let system_tab_bar_requested = cmd_line_settings.system_native_tabs && allow_system_tab;
        ENABLE_SYSTEM_TAB_BAR.store(allow_system_tab, Ordering::Relaxed);
        SHOW_SYSTEM_TAB_BAR.store(system_tab_bar_requested, Ordering::Relaxed);
        (allow_system_tab, system_tab_bar_requested)
    }

    fn configure_system_tabbing(ns_window: &NSWindow) {
        ns_window.setTabbingMode(NSWindowTabbingMode::Preferred);
        ns_window.setTabbingIdentifier(ns_string!(NEOVIDE_TABBING_IDENTIFIER));
        Self::apply_tab_bar_preference(ns_window);
    }

    /// Try to join the new window to its chosen host tab group and select it.
    ///
    /// AppKit may attach the window when tabbing starts, so a window already in
    /// the host group skips attachment but still reaches selection and tab bar
    /// updates. Attaching only this window keeps detached tab groups separate.
    ///
    /// A visible overview blocks a needed attachment, so `false` tells the caller
    /// to discard its pending host height. Otherwise `true` means the operation
    /// was not blocked, though AppKit membership and selection are not confirmed.
    fn join_host_tab_group(window: &NSWindow, host: &NSWindow) -> bool {
        match host.tabGroup() {
            Some(group) if tab_group_contains_window(&group.windows(), window) => {}
            Some(group) if group.isOverviewVisible() => {
                log::warn!("Cannot add a system tab while the host tab overview is visible");
                return false;
            }
            _ => Self::attach_to_host_tab_group(window, host),
        }

        // Attachment can create a group, so select from the current group.
        if let Some(group) = host.tabGroup() {
            group.setSelectedWindow(Some(window));
        }

        Self::apply_tab_bar_preference(window);
        true
    }

    fn attach_to_host_tab_group(window: &NSWindow, host: &NSWindow) {
        if host.isMiniaturized() {
            host.deminiaturize(None);
        }

        host.addTabbedWindow_ordered(window, NSWindowOrderingMode::Above);
    }

    fn apply_tab_bar_preference(ns_window: &NSWindow) {
        if let Some(tab_group) = ns_window.tabGroup() {
            // keep first window clean even when native tabs are enabled.
            // once multiple tabs are present, we can show the bar again.
            let should_show = should_show_system_tab_bar() && tab_group.windows().len() > 1;
            if tab_group.isTabBarVisible() != should_show {
                ns_window.toggleTabBar(None);
            }
        }
    }

    pub fn activate_and_focus(&self) {
        activate_app_and_focus_window(&self.ns_window);
        if let Some(tabs) = &self.system_tabs {
            tabs.restore_host_height_after_activation();
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

        send_ui(SerialCommand::ForceClickCommand, &self.neovim_handler);
    }

    fn install_titlebar_click_handler(
        ns_window: &NSWindow,
        system_titlebar_height: f64,
        scale_factor: f64,
        mtm: MainThreadMarker,
    ) -> Option<Retained<TitlebarClickHandler>> {
        let handler = TitlebarClickHandler::new(mtm);

        // Add the titlebar_click_handler into the view of window.
        let content_view = ns_window.contentView().unwrap();
        content_view.addSubview(&handler);

        // Set the initial size of titlebar_click_handler.
        let content_view_size = content_view.frame().size;
        handler.setFrame(NSRect::new(
            NSPoint::new(0., content_view_size.height - system_titlebar_height),
            NSSize::new(content_view_size.width, system_titlebar_height),
        ));

        // Setup auto layout for titlebar_click_handler.
        handler.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewMinYMargin,
        );
        handler.setTranslatesAutoresizingMaskIntoConstraints(true);

        let _ = scale_factor;

        Some(handler)
    }

    /// Coordinate AppKit tabbing with a simple fullscreen transition. The route
    /// calls this before winit enters the mode and after winit leaves it, so
    /// the tab bar is hidden before the frame changes and can return afterward.
    /// Repeated calls leave the window chrome and process-wide tab flags intact.
    pub fn set_simple_fullscreen_mode(&mut self, enabled: bool) {
        if self.simple_fullscreen == enabled {
            return;
        }

        self.simple_fullscreen = enabled;
        let cmd_line_settings = self.settings.get::<CmdLineSettings>();
        let (allow_system_tab, _) = Self::set_tab_flags(&cmd_line_settings, enabled);

        if enabled {
            self.prepare_for_simple_fullscreen();
        } else {
            self.restore_after_simple_fullscreen(allow_system_tab);
        }
    }

    /// Prepare AppKit titlebar controls before winit enters simple fullscreen.
    /// The route calls this while the ordinary frame is still active, removing
    /// its transparent titlebar click target, toolbar and any visible tab bar.
    fn prepare_for_simple_fullscreen(&mut self) {
        if let Some(handler) = self.titlebar_click_handler.take() {
            handler.removeFromSuperview();
        }

        self.ns_window.setTabbingMode(NSWindowTabbingMode::Disallowed);

        if let Some(tab_group) = self.ns_window.tabGroup()
            && tab_group.isTabBarVisible()
        {
            self.ns_window.toggleTabBar(None);
        }

        if let Some(tabs) = &self.system_tabs {
            tabs.set_toolbar_enabled(false);
        }
    }

    /// Restore AppKit titlebar behavior after winit leaves simple fullscreen.
    /// An eligible window has its inline tab owner before tabbing resumes, so
    /// its hook can place the accessory as AppKit attaches it. The transparent
    /// titlebar click target is restored when possible. The toolbar returns,
    /// and a requested tab bar regains its visibility preference without
    /// changing its group membership.
    fn restore_after_simple_fullscreen(&mut self, allow_system_tab: bool) {
        self.ensure_inline_system_tabs();
        if let Some(tabs) = &self.system_tabs {
            tabs.set_toolbar_enabled(true);
        }

        if allow_system_tab {
            Self::configure_system_tabbing(&self.ns_window);
        } else {
            self.ns_window.setTabbingMode(NSWindowTabbingMode::Disallowed);
        }

        if self.has_transparent_titlebar
            && self.titlebar_click_handler.is_none()
            && let Some(mtm) = MainThreadMarker::new()
        {
            self.titlebar_click_handler = Self::install_titlebar_click_handler(
                &self.ns_window,
                self.system_titlebar_height,
                self.ns_window.backingScaleFactor(),
                mtm,
            );
        }

        if should_show_system_tab_bar() {
            Self::apply_tab_bar_preference(&self.ns_window);
        }
    }

    pub fn is_native_fullscreen_enabled(&self) -> bool { self.is_fullscreen }

    /// Capture this host before winit creates another window.
    ///
    /// A native fullscreen host can receive another tab without sharing its
    /// frame size. The route omits the host if the new window enters fullscreen.
    pub fn host_tab_for_new_window(&self) -> Option<HostTab> {
        let cmd = self.settings.get::<CmdLineSettings>();
        if !cmd.system_native_tabs || !supports_system_tabs(cmd.frame) || self.simple_fullscreen {
            return None;
        }

        let size = (!self.is_native_fullscreen_enabled())
            .then(|| HostTabSize::from_ns_window(&self.ns_window));

        Some(HostTab { window: self.ns_window.clone(), size })
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
        let font = unsafe {
            NSFont::monospacedSystemFontOfSize_weight(CGFloat::from(font_size), NSFontWeightLight)
        };

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
        let font_size = if options.size > 0.0 { options.size } else { cell_height_px } as f64;
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

        let font_attribute_name = unsafe { NSFontAttributeName };
        let attributes = NSDictionary::from_slices(&[font_attribute_name], &[font.as_ref()]);
        let attr_text = NSString::from_str(text);
        unsafe { NSAttributedString::new_with_attributes(&attr_text, attributes.as_ref()) }
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
        let is_fullscreen = self.ns_window.styleMask().contains(NSWindowStyleMask::FullScreen);
        if is_fullscreen != self.is_fullscreen {
            self.is_fullscreen = is_fullscreen;
            self.set_titlebar_click_handler_visible(!is_fullscreen);
        }

        self.refresh_inline_system_tabs();

        self.update_scrollbar_frames();
    }

    fn tab_bar_padding_in_pixels(&self) -> u32 {
        if self.system_tabs.is_some() {
            return 0;
        }

        if !should_show_system_tab_bar() {
            return 0;
        }

        let Some(tab_group) = self.ns_window.tabGroup() else {
            return 0;
        };

        if !tab_group.isTabBarVisible() {
            return 0;
        }

        let scale_factor = self.ns_window.backingScaleFactor();
        Self::titlebar_height_in_pixel(self.system_titlebar_height, scale_factor)
    }

    fn transparent_system_tabs(&self) -> Option<&WindowSystemTabs> {
        if !self.has_transparent_titlebar {
            return None;
        }

        self.system_tabs.as_ref()
    }

    pub fn extra_titlebar_height_in_pixels(&self) -> u32 {
        if let Some(tabs) = self.transparent_system_tabs() {
            let titlebar = if self.is_fullscreen { 0 } else { self.extra_titlebar_height_in_pixel };
            return titlebar.max(tabs.chrome_inset_in_pixels().unwrap_or(0));
        }

        if self.is_fullscreen {
            return 0;
        }

        let tab_padding = self.tab_bar_padding_in_pixels();
        tab_padding + self.extra_titlebar_height_in_pixel
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
        let WindowSettings { show_border, opacity, normal_opacity, .. } =
            self.settings.get::<WindowSettings>();
        let opaque = opacity.min(normal_opacity) >= 1.0;
        self.update_ns_background(opaque, show_border);
        if let Some(tabs) = &self.system_tabs {
            tabs.set_translucent_background(!opaque);
        }
    }

    pub fn set_document_state(&self, path: &str, modified: bool) {
        let represented_path = if path.is_empty() || !Path::new(path).exists() { "" } else { path };
        let ns_path = NSString::from_str(represented_path);
        self.ns_window.setRepresentedFilename(&ns_path);
        self.ns_window.setDocumentEdited(modified);
    }

    pub fn set_title_hidden(&self, title_hidden: bool) {
        let frame = self.settings.get::<CmdLineSettings>().frame;
        let transparent = matches!(frame, Frame::Transparent | Frame::Buttonless);
        let transparent_titlebar = title_hidden || transparent;
        let hidden = if title_hidden {
            NSWindowTitleVisibility::Hidden
        } else {
            NSWindowTitleVisibility::Visible
        };

        if let Some(tabs) = &self.system_tabs {
            tabs.set_title_hidden(title_hidden);
        } else {
            self.ns_window.setTitleVisibility(hidden);
        }

        self.ns_window.setTitlebarAppearsTransparent(transparent_titlebar);
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
            WindowSettingsChanged::MacosSimpleFullscreen(enabled) => {
                self.set_simple_fullscreen_mode(enabled);
            }
            WindowSettingsChanged::Theme(_) => {
                self.refresh_scrollbar_appearance();
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
                if !self.simple_fullscreen && should_show_system_tab_bar() {
                    if let Some(tab_group) = self.ns_window.tabGroup() {
                        tab_group.setSelectedWindow(Some(&self.ns_window));
                    }
                    Self::apply_tab_bar_preference(&self.ns_window);
                }
            }
            None => {
                log::warn!(
                    "macOS activation shortcut attempted to activate window outside the main thread"
                );
            }
        }
    }

    pub fn is_key_window(&self) -> bool { self.ns_window.isKeyWindow() }

    pub fn can_navigate_tabs(&self) -> bool {
        if !should_show_system_tab_bar() {
            return false;
        }
        self.ns_window.tabGroup().map(|group| group.windows().len() > 1).unwrap_or(false)
    }

    pub fn select_next_tab(&self) {
        if !self.can_navigate_tabs() {
            return;
        }

        let ns_window_ref: &NSWindow = self.ns_window.as_ref();
        unsafe {
            let _: () = msg_send![ns_window_ref, selectNextTab: None::<&AnyObject>];
        }
    }

    pub fn select_previous_tab(&self) {
        if !self.can_navigate_tabs() {
            return;
        }

        let ns_window_ref: &NSWindow = self.ns_window.as_ref();
        unsafe {
            let _: () = msg_send![ns_window_ref, selectPreviousTab: None::<&AnyObject>];
        }
    }

    /// Create the application menu and grab initial focus.
    pub fn ensure_app_initialized(&mut self) { ensure_app_initialized(self.settings.as_ref()); }
}
