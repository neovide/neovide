use std::env;
use winit::event_loop::EventLoopProxy;

use crate::{renderer::VSync, window::UserEvent};

#[allow(unused_variables)]
pub fn create_vsync(proxy: EventLoopProxy<UserEvent>) -> VSync {
    if env::var("WAYLAND_DISPLAY").is_ok() {
        VSync::WinitThrottling()
    } else {
        VSync::Opengl()
    }
}
