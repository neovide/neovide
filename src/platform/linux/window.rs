use std::env;
use winit::{event_loop::ActiveEventLoop, window::Window};

use crate::{
    cmd_line::CmdLineSettings,
    renderer::{build_window_config, WindowConfig},
    settings::{load_last_window_settings, PersistentWindowSettings, Settings},
    window::load_icon,
};

pub fn create_window(
    event_loop: &ActiveEventLoop,
    maximized: bool,
    title: &str,
    settings: &Settings,
) -> WindowConfig {
    let icon = load_icon();

    let cmd_line_settings = settings.get::<CmdLineSettings>();

    let window_settings = load_last_window_settings().ok();

    let previous_position = match window_settings {
        Some(PersistentWindowSettings::Windowed { position, .. }) => Some(position),
        _ => None,
    };

    let mouse_cursor_icon = cmd_line_settings.mouse_cursor_icon;

    let mut window_attributes = Window::default_attributes()
        .with_title(title)
        .with_cursor(winit::window::Cursor::Icon(
            mouse_cursor_icon.to_string().parse().unwrap(),
        ))
        .with_maximized(maximized)
        .with_transparent(true)
        .with_visible(false);

    window_attributes = window_attributes.with_window_icon(Some(icon));

    let frame_decoration = cmd_line_settings.frame;

    window_attributes =
        window_attributes.with_decorations(frame_decoration == crate::frame::Frame::Full);

    if let Some(previous_position) = previous_position {
        window_attributes = window_attributes.with_position(previous_position);
    }

    let window_attributes = {
        let window_attributes = if let Some(token) =
            winit::platform::startup_notify::EventLoopExtStartupNotify::read_token_from_env(
                event_loop,
            ) {
            winit::platform::startup_notify::reset_activation_token_env();
            winit::platform::startup_notify::WindowAttributesExtStartupNotify::with_activation_token(
                window_attributes,
                token,
            )
        } else {
            window_attributes
        };

        if env::var("WAYLAND_DISPLAY").is_ok() {
            let app_id = &cmd_line_settings.wayland_app_id;
            winit::platform::wayland::WindowAttributesExtWayland::with_name(
                window_attributes,
                app_id.clone(),
                "neovide",
            )
        } else {
            let class = &cmd_line_settings.x11_wm_class;
            let instance = &cmd_line_settings.x11_wm_class_instance;
            winit::platform::x11::WindowAttributesExtX11::with_name(
                window_attributes,
                class,
                instance,
            )
        }
    };

    build_window_config(window_attributes, event_loop, settings)
}
