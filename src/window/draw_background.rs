use crate::{settings::SETTINGS, window::WindowSettings};

use cocoa::appkit::{NSColor, NSWindow};
use cocoa::base::{id, nil};
use csscolorparser::Color;
use objc::{rc::autoreleasepool, runtime::YES};
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use winit::window::Window;

pub fn draw_background(window: &Window) {
    if let Ok(color) = &SETTINGS
        .get::<WindowSettings>()
        .background_color
        .parse::<Color>()
    {
        autoreleasepool(|| unsafe {
            let [red, green, blue, alpha] = color.to_array();
            if let RawWindowHandle::AppKit(handle) = window.raw_window_handle() {
                let ns_window: id = handle.ns_window as id;
                let ns_background =
                    NSColor::colorWithSRGBRed_green_blue_alpha_(nil, red, green, blue, alpha);
                ns_window.setBackgroundColor_(ns_background);
                ns_window.setTitlebarAppearsTransparent_(YES);
            }
        });
    };
}
