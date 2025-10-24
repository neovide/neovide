use winit::{
    event_loop::EventLoop,
    platform::macos::{EventLoopBuilderExtMacOS, WindowAttributesExtMacOS},
    window::{Cursor, Window},
};

use crate::{
    cmd_line::CmdLineSettings,
    frame::Frame,
    platform::macos::register_file_handler,
    renderer::{build_window_config, WindowConfig},
    settings::{load_last_window_settings, PersistentWindowSettings, Settings},
    window::UserEvent,
};

pub fn create_event_loop() -> EventLoop<UserEvent> {
    let mut builder = EventLoop::with_user_event();
    builder.with_default_menu(false);
    let event_loop = builder.build().expect("Failed to create winit event loop");
    register_file_handler();
    event_loop
}

pub fn create_window(
    event_loop: &winit::event_loop::ActiveEventLoop,
    maximized: bool,
    title: &str,
    settings: &Settings,
) -> WindowConfig {
    let cmd_line_settings = settings.get::<CmdLineSettings>();

    let window_settings = load_last_window_settings().ok();

    let previous_position = match window_settings {
        Some(PersistentWindowSettings::Windowed { position, .. }) => Some(position),
        _ => None,
    };

    let frame_decoration = cmd_line_settings.frame;

    let title_hidden = cmd_line_settings.title_hidden;

    let mouse_cursor_icon = cmd_line_settings.mouse_cursor_icon;

    let mut window_attributes = Window::default_attributes()
        .with_title(title)
        .with_cursor(Cursor::Icon(mouse_cursor_icon.parse()))
        .with_maximized(maximized)
        .with_transparent(true)
        .with_visible(false);

    window_attributes = match frame_decoration {
        Frame::Full => window_attributes,
        Frame::None => window_attributes.with_decorations(false),
        Frame::Buttonless => window_attributes
            .with_title_hidden(title_hidden)
            .with_titlebar_buttons_hidden(true)
            .with_titlebar_transparent(true)
            .with_fullsize_content_view(true),
        Frame::Transparent => window_attributes
            .with_title_hidden(title_hidden)
            .with_titlebar_transparent(true)
            .with_fullsize_content_view(true),
    };

    if let Some(previous_position) = previous_position {
        window_attributes = window_attributes.with_position(previous_position);
    }

    window_attributes = window_attributes.with_accepts_first_mouse(false);

    let window_config = build_window_config(window_attributes, event_loop, settings);

    if let Some(previous_position) = previous_position {
        window_config.window.set_outer_position(previous_position);
    }

    window_config
}
