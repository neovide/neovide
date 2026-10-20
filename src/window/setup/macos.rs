use winit::dpi::PhysicalPosition;
use winit::event_loop::ActiveEventLoop;
use winit::event_loop::EventLoopBuilder;
use winit::platform::macos::EventLoopBuilderExtMacOS;
use winit::platform::macos::WindowAttributesExtMacOS;
use winit::window::Icon;
use winit::window::WindowAttributes;

use super::EventPayload;
use crate::cmd_line::CmdLineSettings;
use crate::frame::Frame;
use crate::renderer::WindowConfig;
use crate::window::macos;

pub(super) fn configure_event_loop(builder: &mut EventLoopBuilder<EventPayload>) {
    builder.with_default_menu(false);
}

pub(super) fn register_file_handler() { macos::register_file_handler(); }

pub(super) fn create_window_attributes(
    _event_loop: &ActiveEventLoop,
    window_attributes: WindowAttributes,
    cmd_line_settings: &CmdLineSettings,
    icon: Icon,
    previous_position: Option<PhysicalPosition<i32>>,
) -> WindowAttributes {
    let window_attributes = window_attributes.with_window_icon(Some(icon));
    let title_hidden = cmd_line_settings.title_hidden;
    let mut window_attributes = match cmd_line_settings.frame {
        Frame::Full => window_attributes.with_title_hidden(title_hidden),
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

    window_attributes.with_accepts_first_mouse(false)
}

pub(super) fn apply_previous_position(
    window_config: &WindowConfig,
    previous_position: Option<PhysicalPosition<i32>>,
) {
    if let Some(previous_position) = previous_position {
        window_config.window.set_outer_position(previous_position);
    }
}
