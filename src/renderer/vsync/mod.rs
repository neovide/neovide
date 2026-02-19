#[cfg(target_os = "macos")]
mod vsync_macos_display_link;
mod vsync_timer;
#[cfg(target_os = "windows")]
mod vsync_win_dwm;
#[cfg(target_os = "windows")]
mod vsync_win_swap_chain;

use std::sync::Arc;

use winit::{event_loop::EventLoopProxy, window::Window};

use crate::{
    renderer::SkiaRenderer,
    settings::Settings,
    window::{EventPayload, WindowSettings},
};
use vsync_timer::VSyncTimer;

#[cfg(target_os = "windows")]
pub use vsync_win_dwm::VSyncWinDwm;
#[cfg(target_os = "windows")]
pub use vsync_win_swap_chain::VSyncWinSwapChain;

#[cfg(target_os = "macos")]
pub use vsync_macos_display_link::VSyncMacosDisplayLink;

#[allow(dead_code)]
pub enum VSync {
    Opengl(),
    WinitThrottling(),
    Timer(VSyncTimer),
    #[cfg(target_os = "windows")]
    WindowsDwm(VSyncWinDwm),
    #[cfg(target_os = "windows")]
    WindowsSwapChain(VSyncWinSwapChain),
    #[cfg(target_os = "macos")]
    MacosDisplayLink(VSyncMacosDisplayLink),
    #[cfg(target_os = "macos")]
    MacosMetal(),
}

impl VSync {
    pub fn new(
        vsync_enabled: bool,
        renderer: &dyn SkiaRenderer,
        proxy: EventLoopProxy<EventPayload>,
        settings: Arc<Settings>,
    ) -> Self {
        if vsync_enabled {
            renderer.create_vsync(proxy)
        } else {
            VSync::Timer(VSyncTimer::new(settings))
        }
    }

    pub fn wait_for_vsync(&mut self) {
        match self {
            VSync::Timer(vsync) => vsync.wait_for_vsync(),
            #[cfg(target_os = "windows")]
            VSync::WindowsDwm(vsync) => vsync.wait_for_vsync(),
            #[cfg(target_os = "windows")]
            VSync::WindowsSwapChain(vsync) => vsync.wait_for_vsync(),
            #[cfg(target_os = "macos")]
            VSync::MacosDisplayLink(vsync) => vsync.wait_for_vsync(),
            _ => {}
        }
    }

    pub fn uses_winit_throttling(&self) -> bool {
        #[cfg(target_os = "windows")]
        return matches!(
            self,
            VSync::WinitThrottling() | VSync::WindowsDwm(..) | VSync::WindowsSwapChain(..)
        );

        #[cfg(target_os = "macos")]
        return matches!(self, VSync::WinitThrottling() | VSync::MacosDisplayLink(..));

        #[cfg(target_os = "linux")]
        return matches!(self, VSync::WinitThrottling());
    }

    pub fn update(&mut self, #[allow(unused_variables)] window: &Window) {
        match self {
            #[cfg(target_os = "macos")]
            VSync::MacosDisplayLink(vsync) => vsync.update(window),
            _ => {}
        }
    }

    pub fn get_refresh_rate(&self, window: &Window, settings: &Settings) -> f32 {
        let settings_refresh_rate = 1.0 / settings.get::<WindowSettings>().refresh_rate as f32;

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
        match self {
            VSync::WinitThrottling(..) => window.request_redraw(),
            #[cfg(target_os = "windows")]
            VSync::WindowsDwm(vsync) => vsync.request_redraw(),
            #[cfg(target_os = "windows")]
            VSync::WindowsSwapChain(vsync) => vsync.request_redraw(),
            #[cfg(target_os = "macos")]
            VSync::MacosDisplayLink(vsync) => vsync.request_redraw(),
            _ => {}
        }
    }
}
