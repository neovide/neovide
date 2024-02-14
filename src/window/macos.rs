use icrate::{
    AppKit::{
        NSColor, NSEvent, NSView, NSViewMinYMargin, NSViewWidthSizable, NSWindow,
        NSWindowStyleMaskFullScreen, NSWindowStyleMaskTitled,
    },
    Foundation::{MainThreadMarker, NSPoint, NSRect, NSSize},
};
use objc2::{declare_class, msg_send_id, mutability::InteriorMutable, rc::Id, ClassType};

use csscolorparser::Color;
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use winit::window::Window;

use crate::{
    cmd_line::CmdLineSettings, error_msg, frame::Frame, renderer::WindowedContext,
    settings::SETTINGS,
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

        let window_settings = SETTINGS.get::<WindowSettings>();
        macos_window_feature.set_background(
            window_settings.transparency,
            window_settings.show_border,
            window_settings.background_color,
            true,
        );

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

    /// Print a deprecation warning for `neovide_background_color`
    pub fn display_deprecation_warning(&self) {
        error_msg!(concat!(
        "neovide_background_color has now been deprecated. ",
        "Use neovide_transparency instead if you want to get a transparent window titlebar. ",
        "Please check https://neovide.dev/configuration.html#background-color-deprecated-currently-macos-only for more information.",
    ));
    }

    #[deprecated(
        since = "0.12.2",
        note = "This function will be removed in the future."
    )]
    fn handle_legacy_background(
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

    fn handle_background(&self, transparency: f32, show_border: bool) {
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

    /// Sets background color, opacity and shadow properties of a window.
    ///
    /// # Arguments
    ///
    /// * `transparency` - `g:neovide_transparency` value between 0.0 (transparent) and 1.0 (opaque)
    /// * `show_border` - Only if `transparency >= 1.0`, this decides if a grey border should be shown
    /// * `background_color` - Deprecated `g:neovide_background_color` value. Overrides `transparency` and `show_border` if present.
    /// * `ignore_deprecation_warning` - Do not print deprecation warning if `background_color` is present
    pub fn set_background(
        &self,
        transparency: f32,
        show_border: bool,
        background_color: String,
        ignore_deprecation_warning: bool,
    ) {
        match background_color.parse::<Color>() {
            Ok(color) => {
                self.handle_legacy_background(color, show_border, ignore_deprecation_warning)
            }
            _ => self.handle_background(transparency, show_border),
        }
    }
}
