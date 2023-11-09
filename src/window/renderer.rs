use std::num::NonZeroU32;
use std::{convert::TryInto, ffi::CString};

use crate::renderer::WindowedContext;
use gl::types::*;
use glutin::prelude::GlConfig;
use skia_safe::gpu::backend_render_targets::make_gl;
use skia_safe::gpu::surfaces::wrap_backend_render_target;
use skia_safe::{
    gpu::{gl::FramebufferInfo, DirectContext, SurfaceOrigin},
    Canvas, ColorType, Surface,
};

fn create_surface(
    windowed_context: &WindowedContext,
    gr_context: &mut DirectContext,
    fb_info: FramebufferInfo,
) -> Surface {
    let pixel_format = windowed_context.get_config();
    let size = windowed_context.get_render_target_size();
    let backend_render_target = make_gl(
        size.into(),
        Some(pixel_format.num_samples().into()),
        pixel_format.stencil_size().into(),
        fb_info,
    );
    windowed_context.resize(
        NonZeroU32::new(size.width).unwrap(),
        NonZeroU32::new(size.height).unwrap(),
    );
    wrap_backend_render_target(
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
    pub gr_context: DirectContext,
    fb_info: FramebufferInfo,
    surface: Surface,
}

impl SkiaRenderer {
    pub fn new(windowed_context: &WindowedContext) -> SkiaRenderer {
        gl::load_with(|s| windowed_context.get_proc_address(CString::new(s).unwrap().as_c_str()));

        let interface = skia_safe::gpu::gl::Interface::new_load_with(|name| {
            if name == "eglGetCurrentDisplay" {
                return std::ptr::null();
            }
            windowed_context.get_proc_address(CString::new(name).unwrap().as_c_str())
        })
        .expect("Could not create interface");

        let mut gr_context = skia_safe::gpu::DirectContext::new_gl(Some(interface), None)
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
        let surface = create_surface(windowed_context, &mut gr_context, fb_info);

        SkiaRenderer {
            gr_context,
            fb_info,
            surface,
        }
    }

    pub fn canvas(&mut self) -> &Canvas {
        self.surface.canvas()
    }

    pub fn resize(&mut self, windowed_context: &WindowedContext) {
        self.surface = create_surface(windowed_context, &mut self.gr_context, self.fb_info);
    }
}
