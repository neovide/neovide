use winit::dpi::PhysicalPosition;
use winit::event_loop::ActiveEventLoop;
use winit::platform::windows::WindowAttributesExtWindows;
use winit::window::Icon;
use winit::window::WindowAttributes;

use crate::cmd_line::CmdLineSettings;
use crate::frame::Frame;

pub(super) fn create_window_attributes(
    _event_loop: &ActiveEventLoop,
    window_attributes: WindowAttributes,
    cmd_line_settings: &CmdLineSettings,
    icon: Icon,
    previous_position: Option<PhysicalPosition<i32>>,
) -> WindowAttributes {
    let window_attributes =
        window_attributes.with_window_icon(Some(icon.clone())).with_taskbar_icon(Some(icon));

    let window_attributes = if !cmd_line_settings.opengl {
        WindowAttributesExtWindows::with_no_redirection_bitmap(window_attributes, true)
    } else {
        window_attributes
    };

    let mut window_attributes =
        window_attributes.with_decorations(cmd_line_settings.frame == Frame::Full);

    if let Some(previous_position) = previous_position {
        window_attributes = window_attributes.with_position(previous_position);
    }

    window_attributes
}
