use winit::{event_loop::ActiveEventLoop, window::WindowAttributes};

use crate::{
    cmd_line::CmdLineSettings,
    renderer::{opengl, WindowConfig, WindowConfigType},
    settings::Settings,
};

pub fn build_window_config(
    window_attributes: WindowAttributes,
    event_loop: &ActiveEventLoop,
    settings: &Settings,
) -> WindowConfig {
    let cmd_line_settings = settings.get::<CmdLineSettings>();
    if cmd_line_settings.opengl {
        opengl::build_window(window_attributes, event_loop)
    } else {
        let window = event_loop.create_window(window_attributes).unwrap();
        let config = WindowConfigType::Direct3D;
        WindowConfig { window, config }
    }
}
