use glutin::{WindowedContext, PossiblyCurrent};
use crate::{settings::SETTINGS, window::WindowSettings};

#[cfg(target_os = "macos")]
use glutin::platform::macos::WindowExtMacOS;
use cocoa::appkit::{NSWindow, NSColor};
use cocoa::base::{id, nil};
use objc::{
    rc::autoreleasepool,
    runtime::{YES},
};
use csscolorparser::Color;

#[cfg(target_os = "macos")]
pub fn draw_background(window: &WindowedContext<PossiblyCurrent>) {
    if let Ok(color) = &SETTINGS.get::<WindowSettings>().background_color.parse::<Color>() {
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
