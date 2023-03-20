use crate::cmd_line::CmdLineSettings;

use glutin::{ContextBuilder, GlProfile, PossiblyCurrent, WindowedContext};

use winit::{event_loop::EventLoop, window::WindowBuilder};

pub type Context = WindowedContext<glutin::PossiblyCurrent>;

pub fn build_context<TE>(
    cmd_line_settings: &CmdLineSettings,
    winit_window_builder: WindowBuilder,
    event_loop: &EventLoop<TE>,
) -> WindowedContext<PossiblyCurrent> {
    let builder = ContextBuilder::new()
        .with_pixel_format(24, 8)
        .with_stencil_buffer(8)
        .with_gl_profile(GlProfile::Core)
        .with_srgb(cmd_line_settings.srgb)
        .with_vsync(cmd_line_settings.vsync);

    let ctx = match builder
        .clone()
        .build_windowed(winit_window_builder.clone(), event_loop)
    {
        Ok(ctx) => ctx,
        Err(err) => {
            // haven't found any sane way to actually match on the pattern rabbithole CreationError
            // provides, so here goes nothing
            if err.to_string().contains("vsync") {
                builder
                    .with_vsync(false)
                    .build_windowed(winit_window_builder, event_loop)
                    .unwrap()
            } else {
                panic!("{}", err);
            }
        }
    };
    unsafe { ctx.make_current().unwrap() }
}
