use std::{
    convert::TryInto,
    env::{self, consts::OS},
    ffi::{c_void, CStr, CString},
    num::NonZeroU32,
    rc::Rc,
    sync::Arc,
};

use gl::{types::*, MAX_RENDERBUFFER_SIZE};
use glutin::surface::SwapInterval;
use glutin::{
    config::{Config, ConfigTemplateBuilder},
    context::{ContextAttributesBuilder, GlProfile, PossiblyCurrentContext},
    display::GetGlDisplay,
    prelude::*,
    surface::{Surface, SurfaceAttributesBuilder, WindowSurface},
};
use glutin_winit::DisplayBuilder;
use raw_window_handle::HasWindowHandle;
use skia_safe::{
    canvas::Canvas,
    gpu::{
        backend_render_targets::make_gl, gl::FramebufferInfo, surfaces::wrap_backend_render_target,
        DirectContext, SurfaceOrigin,
    },
    ColorSpace, ColorType, PixelGeometry, SurfaceProps, SurfacePropsFlags,
};
use winit::{
    dpi::PhysicalSize,
    event_loop::{ActiveEventLoop, EventLoopProxy},
    window::{Window, WindowAttributes},
};

#[cfg(target_os = "windows")]
pub use super::vsync::VSyncWinDwm;

#[cfg(target_os = "macos")]
pub use super::vsync::VSyncMacosDisplayLink;

use super::{RendererSettings, SkiaRenderer, VSync, WindowConfig, WindowConfigType};

use crate::{profiling::tracy_gpu_zone, settings::Settings, window::EventPayload};

#[cfg(feature = "gpu_profiling")]
use crate::profiling::{opengl::create_opengl_gpu_context, GpuCtx};

pub struct OpenGLSkiaRenderer {
    // NOTE: The destruction order is important, so don't re-arrange
    // If possible keep it the reverse of the initialization order
    skia_surface: skia_safe::Surface,
    fb_info: FramebufferInfo,
    pub gr_context: DirectContext,
    context: PossiblyCurrentContext,
    window_surface: Surface<WindowSurface>,
    config: Config,
    window: Option<Rc<Window>>,

    settings: Arc<Settings>,
}

fn clamp_render_buffer_size(size: &PhysicalSize<u32>) -> PhysicalSize<u32> {
    PhysicalSize::new(
        size.width.clamp(1, MAX_RENDERBUFFER_SIZE),
        size.height.clamp(1, MAX_RENDERBUFFER_SIZE),
    )
}

fn get_proc_address(surface: &Surface<WindowSurface>, addr: &CStr) -> *const c_void {
    GlDisplay::get_proc_address(&surface.display(), addr)
}

impl OpenGLSkiaRenderer {
    pub fn new(window: WindowConfig, srgb: bool, vsync: bool, settings: Arc<Settings>) -> Self {
        #[allow(irrefutable_let_patterns)] // This can only be something else than OpenGL on Windows
        let config = if let WindowConfigType::OpenGL(config) = window.config {
            config
        } else {
            panic!("Not an opengl window");
        };
        let window = window.window;
        let gl_display = config.display();
        let raw_window_handle = window.window_handle().unwrap().as_raw();

        let size = clamp_render_buffer_size(&window.inner_size());

        let surface_attributes = SurfaceAttributesBuilder::<WindowSurface>::new()
            .with_srgb(Some(srgb))
            .build(
                raw_window_handle,
                NonZeroU32::new(size.width).unwrap(),
                NonZeroU32::new(size.height).unwrap(),
            );
        let window_surface =
            unsafe { gl_display.create_window_surface(&config, &surface_attributes) }
                .expect("Failed to create Windows Surface");

        let context_attributes = ContextAttributesBuilder::new()
            .with_profile(GlProfile::Core)
            .build(Some(raw_window_handle));
        let context = unsafe { gl_display.create_context(&config, &context_attributes) }
            .expect("Failed to create OpenGL context")
            .make_current(&window_surface)
            .unwrap();

        // NOTE: We don't care if these fails, the driver can override the SwapInterval in any case, so it needs to work in all cases
        // The OpenGL VSync is always disabled on Wayland and Windows, since they have their own
        // implementation
        let _ = if vsync && env::var("WAYLAND_DISPLAY").is_err() && OS != "windows" && OS != "macos"
        {
            window_surface
                .set_swap_interval(&context, SwapInterval::Wait(NonZeroU32::new(1).unwrap()))
        } else {
            window_surface.set_swap_interval(&context, SwapInterval::DontWait)
        };

        gl::load_with(|s| get_proc_address(&window_surface, CString::new(s).unwrap().as_c_str()));

        let interface = skia_safe::gpu::gl::Interface::new_load_with(|name| {
            if name == "eglGetCurrentDisplay" {
                return std::ptr::null();
            }
            get_proc_address(&window_surface, CString::new(name).unwrap().as_c_str())
        })
        .expect("Could not create interface");

        let mut gr_context = skia_safe::gpu::direct_contexts::make_gl(interface, None)
            .expect("Could not create direct context");
        let fb_info = {
            let mut fboid: GLint = 0;
            unsafe { gl::GetIntegerv(gl::FRAMEBUFFER_BINDING, &mut fboid) };

            FramebufferInfo {
                fboid: fboid.try_into().expect("Could not create frame buffer id"),
                format: skia_safe::gpu::gl::Format::RGBA8.into(),
                ..Default::default()
            }
        };
        let skia_surface = create_surface(
            &config,
            &window.inner_size(),
            &context,
            &window_surface,
            &mut gr_context,
            &fb_info,
            &settings,
        );

        Self {
            window_surface,
            context,
            window: Some(window),
            config,
            gr_context,
            fb_info,
            skia_surface,

            settings,
        }
    }
}

impl SkiaRenderer for OpenGLSkiaRenderer {
    fn window(&self) -> Rc<Window> {
        Rc::clone(self.window.as_ref().unwrap())
    }

    fn flush(&mut self) {
        {
            tracy_gpu_zone!("skia flush");
            self.gr_context.flush_and_submit();
        }
    }

    fn swap_buffers(&mut self) {
        {
            tracy_gpu_zone!("swap buffers");
            self.window().pre_present_notify();
            let _ = self.window_surface.swap_buffers(&self.context);
        }
    }

    fn canvas(&mut self) -> &Canvas {
        self.skia_surface.canvas()
    }

    fn resize(&mut self) {
        self.skia_surface = create_surface(
            &self.config,
            &self.window().inner_size(),
            &self.context,
            &self.window_surface,
            &mut self.gr_context,
            &self.fb_info,
            &self.settings,
        );
    }

    #[allow(unused_variables)]
    fn create_vsync(&self, proxy: EventLoopProxy<EventPayload>) -> VSync {
        #[cfg(target_os = "linux")]
        if env::var("WAYLAND_DISPLAY").is_ok() {
            VSync::WinitThrottling()
        } else {
            VSync::Opengl()
        }

        #[cfg(target_os = "windows")]
        {
            VSync::WindowsDwm(VSyncWinDwm::new(proxy))
        }

        #[cfg(target_os = "macos")]
        {
            VSync::MacosDisplayLink(VSyncMacosDisplayLink::new(&self.window(), proxy))
        }
    }

    #[cfg(feature = "gpu_profiling")]
    fn tracy_create_gpu_context(&self, name: &str) -> Box<dyn GpuCtx> {
        create_opengl_gpu_context(name)
    }
}

impl Drop for OpenGLSkiaRenderer {
    fn drop(&mut self) {
        match self.window_surface.display() {
            #[cfg(not(target_os = "macos"))]
            glutin::display::Display::Egl(display) => {
                // Ensure that all the windows are dropped, so the destructors for
                // Renderer and contexts ran.
                self.window = None;

                self.gr_context.release_resources_and_abandon();

                // SAFETY: the display is being destroyed after destroying all the
                // windows, thus no attempt to access the EGL state will be made.
                unsafe {
                    display.terminate();
                }
            }
            _ => (),
        }
    }
}

fn gen_config(mut config_iterator: Box<dyn Iterator<Item = Config> + '_>) -> Config {
    config_iterator.next().unwrap()
}

pub fn build_window(
    window_attributes: WindowAttributes,
    event_loop: &ActiveEventLoop,
) -> WindowConfig {
    let template_builder = ConfigTemplateBuilder::new()
        .with_stencil_size(8)
        .with_transparency(true);
    let (window, config) = DisplayBuilder::new()
        .with_window_attributes(Some(window_attributes))
        .build(event_loop, template_builder, gen_config)
        .expect("Failed to create Window");
    let window = window.expect("Could not create Window");
    let config = WindowConfigType::OpenGL(config);
    WindowConfig {
        window: window.into(),
        config,
    }
}

fn create_surface(
    pixel_format: &Config,
    size: &PhysicalSize<u32>,
    context: &PossiblyCurrentContext,
    window_surface: &Surface<WindowSurface>,
    gr_context: &mut DirectContext,
    fb_info: &FramebufferInfo,
    settings: &Settings,
) -> skia_safe::Surface {
    let size = clamp_render_buffer_size(size);
    let backend_render_target = make_gl(
        size.into(),
        Some(pixel_format.num_samples().into()),
        pixel_format.stencil_size().into(),
        *fb_info,
    );

    let width = NonZeroU32::new(size.width).unwrap();
    let height = NonZeroU32::new(size.height).unwrap();
    GlSurface::resize(window_surface, context, width, height);

    let render_settings = settings.get::<RendererSettings>();

    let surface_props = SurfaceProps::new_with_text_properties(
        SurfacePropsFlags::default(),
        PixelGeometry::default(),
        render_settings.text_contrast,
        render_settings.text_gamma,
    );

    // NOTE: It would be much better to render using a linear gamma format, and SRGB backbuffer
    // format But currently the Skia glyph atlas uses a 32-bit linear format texture, so some color
    // precision is lost, and the font colors will be slightly off.
    wrap_backend_render_target(
        gr_context,
        &backend_render_target,
        SurfaceOrigin::BottomLeft,
        ColorType::RGBA8888,
        ColorSpace::new_srgb(),
        Some(surface_props).as_ref(),
    )
    .expect("Could not create skia backend render target")
}
