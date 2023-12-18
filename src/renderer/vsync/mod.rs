#[cfg(target_os = "macos")]
mod macos_display_link;
#[cfg(target_os = "macos")]
mod vsync_macos;
mod vsync_timer;
#[cfg(target_os = "windows")]
mod vsync_win;

use vsync_timer::VSyncTimer;

use crate::{
    renderer::WindowedContext, settings::SETTINGS, window::UserEvent, window::WindowSettings,
};
use winit::event_loop::EventLoopProxy;

#[cfg(target_os = "linux")]
use std::env;

#[cfg(target_os = "windows")]
use vsync_win::VSyncWin;

#[cfg(target_os = "macos")]
use vsync_macos::VSyncMacos;

#[allow(dead_code)]
pub enum VSync {
    Opengl(),
    WinitThrottling(),
    Timer(VSyncTimer),
    #[cfg(target_os = "windows")]
    Windows(VSyncWin),
    #[cfg(target_os = "macos")]
    Macos(VSyncMacos),
}

impl VSync {
    #[allow(unused_variables)]
    pub fn new(
        vsync_enabled: bool,
        context: &WindowedContext,
        proxy: EventLoopProxy<UserEvent>,
    ) -> Self {
        if vsync_enabled {
            #[cfg(target_os = "linux")]
            if env::var("WAYLAND_DISPLAY").is_ok() {
                VSync::WinitThrottling()
            } else {
                VSync::Opengl()
            }

            #[cfg(target_os = "windows")]
            {
                VSync::Windows(VSyncWin::new(proxy))
            }

            #[cfg(target_os = "macos")]
            {
                VSync::Macos(VSyncMacos::new(context, proxy))
            }
        } else {
            VSync::Timer(VSyncTimer::new())
        }
    }

    pub fn wait_for_vsync(&mut self) {
        match self {
            VSync::Timer(vsync) => vsync.wait_for_vsync(),
            #[cfg(target_os = "windows")]
            VSync::Windows(vsync) => vsync.wait_for_vsync(),
            #[cfg(target_os = "macos")]
            VSync::Macos(vsync) => vsync.wait_for_vsync(),
            _ => {}
        }
    }

    pub fn uses_winit_throttling(&self) -> bool {
        #[cfg(target_os = "windows")]
        return matches!(self, VSync::WinitThrottling() | VSync::Windows(..));

        #[cfg(target_os = "macos")]
        return matches!(self, VSync::WinitThrottling() | VSync::Macos(..));

        #[cfg(target_os = "linux")]
        return matches!(self, VSync::WinitThrottling());
    }

    pub fn update(&mut self, #[allow(unused_variables)] context: &WindowedContext) {
        match self {
            #[cfg(target_os = "macos")]
            VSync::Macos(vsync) => vsync.update(context),
            _ => {}
        }
    }

    pub fn get_refresh_rate(&self, context: &WindowedContext) -> f32 {
        let settings_refresh_rate = 1.0 / SETTINGS.get::<WindowSettings>().refresh_rate as f32;

        match self {
            VSync::Timer(_) => settings_refresh_rate,
            _ => {
                let monitor = context.window().current_monitor();
                monitor
                    .and_then(|monitor| monitor.refresh_rate_millihertz())
                    .map(|rate| 1000.0 / rate as f32)
                    .unwrap_or_else(|| settings_refresh_rate)
                    // We don't really want to support less than 10 FPS
                    .min(0.1)
            }
        }
    }

    pub fn request_redraw(&mut self, context: &WindowedContext) {
        match self {
            VSync::WinitThrottling(..) => context.window().request_redraw(),
            #[cfg(target_os = "windows")]
            VSync::Windows(vsync) => vsync.request_redraw(),
            #[cfg(target_os = "macos")]
            VSync::Macos(vsync) => vsync.request_redraw(),
            _ => {}
        }
    }
}
