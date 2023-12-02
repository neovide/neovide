use icrate::{
    AppKit::{
        NSColor, NSEvent, NSView, NSViewMinYMargin, NSViewWidthSizable, NSWindow,
        NSWindowStyleMaskTitled,
    },
    Foundation::{MainThreadMarker, NSPoint, NSRect, NSSize},
};
use objc2::{declare_class, msg_send_id, mutability::InteriorMutable, rc::Id, ClassType};

use csscolorparser::Color;
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use winit::window::Window;

use crate::{cmd_line::CmdLineSettings, error_msg, frame::Frame, settings::SETTINGS};

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
    // Extra titlebar height in --frame transparency. 0 in other cases.
    extra_titlebar_height_in_pixel: u32,
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
        if frame == Frame::Transparent {
            unsafe {
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
            }
        }

        MacosWindowFeature {
            extra_titlebar_height_in_pixel,
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

    /// Get the extra titlebar height in pixels, so Neovide can do the correct top padding.
    pub fn extra_titlebar_height_in_pixels(&self) -> u32 {
        self.extra_titlebar_height_in_pixel
    }
}
