use std::env;

use winit::dpi::PhysicalPosition;
use winit::event_loop::ActiveEventLoop;
use winit::platform::startup_notify;
use winit::platform::startup_notify::EventLoopExtStartupNotify;
use winit::platform::startup_notify::WindowAttributesExtStartupNotify;
use winit::platform::wayland::WindowAttributesExtWayland;
use winit::platform::x11::WindowAttributesExtX11;
use winit::window::Icon;
use winit::window::WindowAttributes;

use crate::cmd_line::CmdLineSettings;
use crate::frame::Frame;

pub fn create_window_attributes(
    event_loop: &ActiveEventLoop,
    window_attributes: WindowAttributes,
    cmd_line_settings: &CmdLineSettings,
    icon: Icon,
    previous_position: Option<PhysicalPosition<i32>>,
) -> WindowAttributes {
    let mut window_attributes = window_attributes
        .with_window_icon(Some(icon))
        .with_decorations(cmd_line_settings.frame == Frame::Full);

    if let Some(previous_position) = previous_position {
        window_attributes = window_attributes.with_position(previous_position);
    }

    let window_attributes =
        if let Some(token) = EventLoopExtStartupNotify::read_token_from_env(event_loop) {
            startup_notify::reset_activation_token_env();
            WindowAttributesExtStartupNotify::with_activation_token(window_attributes, token)
        } else {
            window_attributes
        };

    if env::var("WAYLAND_DISPLAY").is_ok() {
        let app_id = &cmd_line_settings.wayland_app_id;
        WindowAttributesExtWayland::with_name(window_attributes, app_id.clone(), "neovide")
    } else {
        let class = &cmd_line_settings.x11_wm_class;
        let instance = &cmd_line_settings.x11_wm_class_instance;
        WindowAttributesExtX11::with_name(window_attributes, class, instance)
    }
}
