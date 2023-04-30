use std::ffi::{c_void, CStr};
use std::num::NonZeroU32;

use crate::cmd_line::CmdLineSettings;

use glutin::{
    config::{Config, ConfigTemplateBuilder},
    context::{ContextAttributesBuilder, GlProfile, PossiblyCurrentContext},
    display::GetGlDisplay,
    prelude::*,
    surface::{Surface, SurfaceAttributesBuilder, WindowSurface},
};
use glutin_winit::DisplayBuilder;
use raw_window_handle::HasRawWindowHandle;
use winit::{
    event_loop::EventLoop,
    window::{Window, WindowBuilder},
};

pub struct Context {
    surface: Surface<WindowSurface>,
    context: PossiblyCurrentContext,
    window: Window,
    config: Config,
}

impl Context {
    pub fn window(&self) -> &Window {
        &self.window
    }
    pub fn resize(&self, width: NonZeroU32, height: NonZeroU32) {
        GlSurface::resize(&self.surface, &self.context, width, height)
    }
    pub fn swap_buffers(&self) -> glutin::error::Result<()> {
        GlSurface::swap_buffers(&self.surface, &self.context)
    }
    pub fn get_proc_address(&self, addr: &CStr) -> *const c_void {
        GlDisplay::get_proc_address(&self.surface.display(), addr)
    }
    pub fn get_config(&self) -> &Config {
        &self.config
    }
}

fn gen_config(mut config_iterator: Box<dyn Iterator<Item = Config> + '_>) -> Config {
    config_iterator.next().unwrap()
}

pub fn build_context<TE>(
    cmd_line_settings: &CmdLineSettings,
    winit_window_builder: WindowBuilder,
    event_loop: &EventLoop<TE>,
) -> Context {
    let template_builder = ConfigTemplateBuilder::new()
        .with_stencil_size(8)
        .with_transparency(true);
    let (window, config) = DisplayBuilder::new()
        .with_window_builder(Some(winit_window_builder))
        .build(event_loop, template_builder, gen_config)
        .unwrap();
    let window = window.expect("Could not create Window");

    let gl_display = config.display();
    let raw_window_handle = window.raw_window_handle();

    let size = window.inner_size();

    let surface_attributes = SurfaceAttributesBuilder::<WindowSurface>::new()
        .with_srgb(Some(cmd_line_settings.srgb))
        .build(
            raw_window_handle,
            NonZeroU32::new(size.width.max(1)).unwrap(),
            NonZeroU32::new(size.height.max(1)).unwrap(),
        );
    let surface =
        unsafe { gl_display.create_window_surface(&config, &surface_attributes) }.unwrap();
    let context_attributes = ContextAttributesBuilder::new()
        .with_profile(GlProfile::Core)
        .build(Some(raw_window_handle));
    let context = unsafe {
        gl_display
            .create_context(&config, &context_attributes)
            .unwrap()
    }
    .make_current(&surface)
    .unwrap();

    Context {
        surface,
        context,
        window,
        config,
    }
}
