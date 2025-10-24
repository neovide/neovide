use winit::{
    event_loop::ActiveEventLoop,
    window::{Icon, Window, WindowAttributes},
};

use crate::{
    cmd_line::CmdLineSettings,
    renderer::{build_window_config, WindowConfig},
    settings::{load_last_window_settings, PersistentWindowSettings, Settings},
    window::{load_icon, UserEvent},
};

#[derive(Clone, Debug, PartialEq)]
pub enum WindowCommand {
    TitleChanged(String),
    SetMouseEnabled(bool),
    ListAvailableFonts,
    FocusWindow,
    Minimize,
    RegisterRightClick,
    UnregisterRightClick,
}

impl From<WindowCommand> for UserEvent {
    fn from(value: WindowCommand) -> Self {
        UserEvent::WindowCommand(value)
    }
}

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
        .with_cursor(mouse_cursor_icon.into())
        .with_maximized(maximized)
        .with_transparent(true)
        .with_visible(false);

    window_attributes = window_attributes
        .with_window_icon(Some(icon.clone()))
        .with_taskbar_icon(Some(icon));

    if !cmd_line_settings.opengl {
        window_attributes =
            winit::platform::windows::WindowAttributesExtWindows::with_no_redirection_bitmap(
                window_attributes,
                true,
            );
    }

    let frame_decoration = cmd_line_settings.frame;

    window_attributes =
        window_attributes.with_decorations(frame_decoration == crate::frame::Frame::Full);

    if let Some(previous_position) = previous_position {
        window_attributes = window_attributes.with_position(previous_position);
    }

    build_window_config(window_attributes, event_loop, settings)
}
