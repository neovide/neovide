mod vsync_timer;

use std::env;

use vsync_timer::VSyncTimer;

use crate::{profiling::tracy_zone, settings::SETTINGS, window::WindowSettings};
use winit::window::Window;

#[allow(dead_code)]
pub enum VSync {
    WinitThrottling(),
    Timer(VSyncTimer),
}

impl VSync {
    pub fn new(vsync_enabled: bool) -> Self {
        if env::var("WAYLAND_DISPLAY").is_ok() && vsync_enabled {
            VSync::WinitThrottling()
        } else {
            VSync::Timer(VSyncTimer::new())
        }
    }

    pub fn wait_for_vsync(&mut self) {
        tracy_zone!("wait for vsync");
        if let VSync::Timer(vsync) = self {
            vsync.wait_for_vsync();
        }
    }

    pub fn uses_winit_throttling(&self) -> bool {
        matches!(self, VSync::WinitThrottling())
    }

    pub fn update(&mut self, #[allow(unused_variables)] window: &Window) {}

    pub fn get_refresh_rate(&self, window: &Window) -> f32 {
        let settings_refresh_rate = 1.0 / SETTINGS.get::<WindowSettings>().refresh_rate as f32;

        match self {
            VSync::Timer(_) => settings_refresh_rate,
            _ => {
                let monitor = window.current_monitor();
                monitor
                    .and_then(|monitor| monitor.refresh_rate_millihertz())
                    .map(|rate| 1000.0 / rate as f32)
                    .unwrap_or_else(|| settings_refresh_rate)
                    // We don't really want to support less than 10 FPS
                    .min(0.1)
            }
        }
    }

    pub fn request_redraw(&mut self, window: &Window) {
        if let VSync::WinitThrottling(..) = self {
            window.request_redraw();
        }
    }
}
