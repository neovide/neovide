use std::convert::TryInto;

use gl::types::*;
use glutin::{window::Window, PossiblyCurrent, RawContext};
use skia_safe::{
    gpu::{gl::FramebufferInfo, BackendRenderTarget, DirectContext, SurfaceOrigin},
    Canvas, ColorType, Surface,
};

fn create_surface(
    gl_context: &RawContext<PossiblyCurrent>,
    window: &Window,
    gr_context: &mut DirectContext,
    framebuffer_info: FramebufferInfo,
) -> Surface {
    let pixel_format = gl_context.get_pixel_format();
    let size = window.inner_size();
    let size = (
        size.width.try_into().expect("Could not convert width"),
        size.height.try_into().expect("Could not convert height"),
    );
    let backend_render_target = BackendRenderTarget::new_gl(
        size,
        pixel_format
            .multisampling
            .map(|s| s.try_into().expect("Could not convert multisampling")),
        pixel_format
            .stencil_bits
            .try_into()
            .expect("Could not convert stencil"),
        framebuffer_info,
    );
    gl_context.resize(size.into());
    Surface::from_backend_render_target(
        gr_context,
        &backend_render_target,
        SurfaceOrigin::BottomLeft,
        ColorType::RGBA8888,
        None,
        None,
    )
    .expect("Could not create skia surface")
}

pub struct SkiaRenderer {
    pub skia_context: DirectContext,
    framebuffer_info: FramebufferInfo,
    surface: Surface,
}

impl SkiaRenderer {
    pub fn new(gl_context: &RawContext<PossiblyCurrent>, window: &Window) -> SkiaRenderer {
        gl::load_with(|s| gl_context.get_proc_address(s));

        let interface = skia_safe::gpu::gl::Interface::new_load_with(|name| {
            if name == "eglGetCurrentDisplay" {
                return std::ptr::null();
            }
            gl_context.get_proc_address(name)
        })
        .expect("Could not create interface");

        let mut skia_context = skia_safe::gpu::DirectContext::new_gl(Some(interface), None)
            .expect("Could not create direct context");
        let framebuffer_info = {
            let mut fboid: GLint = 0;
            unsafe { gl::GetIntegerv(gl::FRAMEBUFFER_BINDING, &mut fboid) };

            FramebufferInfo {
                fboid: fboid.try_into().expect("Could not create frame buffer id"),
                format: skia_safe::gpu::gl::Format::RGBA8.into(),
            }
        };
        let surface = create_surface(gl_context, window, &mut skia_context, framebuffer_info);

        SkiaRenderer {
            skia_context,
            framebuffer_info,
            surface,
        }
    }

    pub fn canvas(&mut self) -> &mut Canvas {
        self.surface.canvas()
    }

    pub fn resize(&mut self, gl_context: &RawContext<PossiblyCurrent>, window: &Window) {
        self.surface = create_surface(
            gl_context,
            window,
            &mut self.skia_context,
            self.framebuffer_info,
        );
    }
}
