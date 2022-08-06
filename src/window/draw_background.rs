use crate::{settings::SETTINGS, window::WindowSettings};
use glutin::{PossiblyCurrent, WindowedContext};

use cocoa::appkit::{NSColor, NSWindow};
use cocoa::base::{id, nil};
use csscolorparser::Color;
use glutin::platform::macos::WindowExtMacOS;
use objc::{rc::autoreleasepool, runtime::YES};

pub fn draw_background(window: &WindowedContext<PossiblyCurrent>) {
    if let Ok(color) = &SETTINGS
        .get::<WindowSettings>()
        .background_color
        .parse::<Color>()
    {
        autoreleasepool(|| unsafe {
            let [red, green, blue, alpha] = color.to_array();
            let ns_window: id = window.window().ns_window() as id;
            let ns_background = NSColor::colorWithSRGBRed_green_blue_alpha_(
                nil,
                red.into(),
                green.into(),
                blue.into(),
                alpha.into(),
            );
            ns_window.setBackgroundColor_(ns_background);
            ns_window.setTitlebarAppearsTransparent_(YES);
        });
    };
}
