use log::info;

use skulpin::winit::event::WindowEvent;
use skulpin::winit::event_loop::{
    ControlFlow, EventLoopClosed, EventLoopProxy, EventLoopWindowTarget,
};
use skulpin::winit::window::{Icon, Window, WindowBuilder, WindowId};
use skulpin::{
    winit::dpi::LogicalSize, CoordinateSystem, PresentMode, Renderer as SkulpinRenderer,
    RendererBuilder, WinitWindow,
};
use std::collections::HashMap;

#[cfg(feature = "winit")]
pub trait EventProcessor {
    fn process_event(&mut self, e: WindowEvent) -> Option<ControlFlow>;
}

pub trait WindowHandle: EventProcessor {
    fn window(&mut self) -> &Window;
    fn set_window(&mut self, window: Window);
    fn logical_size(&self) -> LogicalSize<u32>;
    fn update(&mut self) -> bool;
    fn should_draw(&self) -> bool;
    fn draw(&mut self, skulpin_renderer: &mut SkulpinRenderer) -> bool;
}

pub struct WindowManager<T: 'static + NoopEvent> {
    windows: HashMap<WindowId, Box<dyn WindowHandle>>,
    renderer: Option<SkulpinRenderer>,
    proxy: EventLoopProxy<T>,
}

impl<T: NoopEvent> WindowManager<T> {
    pub fn new(proxy: EventLoopProxy<T>) -> Self {
        Self {
            windows: HashMap::new(),
            renderer: None,
            proxy,
        }
    }

    pub fn noop(&self) -> Result<(), EventLoopClosed<T>> {
        self.proxy.send_event(T::noop())
    }

    pub fn handle_event(&mut self, id: WindowId, event: WindowEvent) -> Option<ControlFlow> {
        if let Some(handle) = self.windows.get_mut(&id) {
            handle.process_event(event)
        } else {
            None
        }
    }

    fn initialize_renderer(&mut self, window: &Window) {
        let renderer = {
            let winit_window_wrapper = WinitWindow::new(window);
            RendererBuilder::new()
                .prefer_integrated_gpu()
                .use_vulkan_debug_layer(false)
                .present_mode_priority(vec![PresentMode::Immediate])
                .coordinate_system(CoordinateSystem::Logical)
                .build(&winit_window_wrapper)
                .expect("Failed to create renderer")
        };
        self.renderer = Some(renderer);
    }

    pub fn create_window<U: 'static + WindowHandle + Default>(
        &mut self,
        title: &str,
        window_target: &EventLoopWindowTarget<T>,
        icon: Option<Icon>,
    ) {
        let mut handle = Box::new(U::default());
        let logical_size = handle.logical_size();

        let window = WindowBuilder::new()
            .with_title(title)
            .with_inner_size(logical_size)
            .with_window_icon(icon)
            .build(window_target)
            .expect("Failed to create window");
        info!("window created");
        if self.renderer.is_none() {
            self.initialize_renderer(&window);
        }
        let window_id = window.id();
        handle.set_window(window);
        self.windows.insert(window_id, handle);
    }

    pub fn update_all(&mut self) -> bool {
        for handle in self.windows.values_mut() {
            if !handle.update() {
                return false;
            }
        }
        true
    }

    pub fn render_all(&mut self) -> bool {
        let mut renderer = self.renderer.as_mut().unwrap();
        for handle in self.windows.values_mut() {
            if !handle.draw(&mut renderer) {
                return false;
            }
        }
        true
    }
}

pub trait NoopEvent {
    fn noop() -> Self;
}

#[derive(Debug)]
pub enum NeovideEvent {
    // Pause(WindowId),
    Noop,
}

impl NoopEvent for NeovideEvent {
    fn noop() -> Self {
        NeovideEvent::Noop
    }
}
