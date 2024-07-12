use core_graphics_types::{base::CGFloat, geometry::CGSize};
use icrate::AppKit::NSWindow;
use metal::{
    foreign_types::{ForeignType, ForeignTypeRef},
    CommandQueue, Device, MTLPixelFormat, MetalDrawable, MetalLayer,
};
use objc2::{msg_send, rc::Id, runtime::AnyObject};
use raw_window_handle::HasRawWindowHandle;
use skia_safe::{
    gpu::{
        mtl::{BackendContext, Handle, TextureInfo},
        surfaces::wrap_backend_render_target,
        BackendRenderTarget, DirectContext, SurfaceOrigin,
    },
    Canvas, ColorType, Surface,
};
use winit::{event_loop::EventLoopProxy, window::Window};

use crate::{profiling::tracy_gpu_zone, window::UserEvent};

use super::{vsync::VSyncMacosDisplayLink, SkiaRenderer, VSync};

struct MetalDrawableSurface {
    pub drawable: MetalDrawable,
    pub surface: Surface,
}

impl MetalDrawableSurface {
    fn new(drawable: MetalDrawable, context: &mut DirectContext) -> MetalDrawableSurface {
        tracy_gpu_zone!("MetalDrawableSurface.new");

        let texture = drawable.texture();
        let texture_info = unsafe { TextureInfo::new(texture.as_ptr() as Handle) };
        let backend_render_target = BackendRenderTarget::new_metal(
            (texture.width() as i32, texture.height() as i32),
            &texture_info,
        );

        let surface = wrap_backend_render_target(
            context,
            &backend_render_target,
            SurfaceOrigin::TopLeft,
            ColorType::BGRA8888,
            None,
            None,
        )
        .expect("Failed to create skia surface with metal drawable.");

        MetalDrawableSurface { drawable, surface }
    }

    fn new_from_next_drawable_of_metal_layer(
        metal_layer: &mut MetalLayer,
        context: &mut DirectContext,
    ) -> MetalDrawableSurface {
        tracy_gpu_zone!("MetalDrawableSurface.new_from_next_drawable_of_metal_layer");

        let drawable = metal_layer
            .next_drawable()
            .expect("Failed to get next drawable of metal layer.")
            .to_owned();

        Self::new(drawable, context)
    }
}

pub struct MetalSkiaRenderer {
    window: Window,
    _device: Device,
    metal_layer: MetalLayer,
    command_queue: CommandQueue,
    _backend: BackendContext,
    context: DirectContext,
    metal_drawable_surface: Option<MetalDrawableSurface>,
}

impl MetalSkiaRenderer {
    pub fn new(window: Window) -> Self {
        log::info!("Initialize MetalSkiaRenderer...");

        let device = Device::system_default().expect("No metal device found.");

        let draw_size = window.inner_size();

        let metal_layer = {
            let metal_layer = MetalLayer::new();
            metal_layer.set_device(&device);
            metal_layer.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
            metal_layer.set_presents_with_transaction(false);
            metal_layer.set_framebuffer_only(false);
            metal_layer.set_display_sync_enabled(false);
            metal_layer.set_opaque(false);

            unsafe {
                let ns_window = match window.raw_window_handle() {
                    raw_window_handle::RawWindowHandle::AppKit(handle) => {
                        Id::retain(handle.ns_window as *mut NSWindow).unwrap()
                    }
                    _ => panic!("Not an AppKit window."),
                };
                let ns_view = ns_window.contentView().unwrap();
                ns_view.setWantsLayer(true);
                let _: () = msg_send![&ns_view, setLayer:(metal_layer.as_ptr() as * mut AnyObject)];
            }

            metal_layer
                .set_drawable_size(CGSize::new(draw_size.width as f64, draw_size.height as f64));
            metal_layer
        };

        let command_queue = device.new_command_queue();

        let backend = unsafe {
            BackendContext::new(
                device.as_ptr() as Handle,
                command_queue.as_ptr() as Handle,
                std::ptr::null(),
            )
        };

        let context = DirectContext::new_metal(&backend, None).unwrap();

        MetalSkiaRenderer {
            window,
            _device: device,
            metal_layer,
            command_queue,
            _backend: backend,
            context,
            metal_drawable_surface: None,
        }
    }

    fn move_to_next_frame(&mut self) {
        tracy_gpu_zone!("move_to_next_frame");

        self.metal_drawable_surface =
            Some(MetalDrawableSurface::new_from_next_drawable_of_metal_layer(
                &mut self.metal_layer,
                &mut self.context,
            ));
    }
}

impl SkiaRenderer for MetalSkiaRenderer {
    fn window(&self) -> &Window {
        &self.window
    }

    fn flush(&mut self) {
        tracy_gpu_zone!("flush");

        self.context.flush_and_submit();
    }

    fn swap_buffers(&mut self) {
        tracy_gpu_zone!("swap buffers");

        let command_buffer = self.command_queue.new_command_buffer();
        command_buffer.present_drawable(
            self.metal_drawable_surface
                .as_mut()
                .expect("Not metal drawable surface now.")
                .drawable
                .as_ref(),
        );
        command_buffer.commit();

        self.metal_drawable_surface = None;
    }

    fn canvas(&mut self) -> &Canvas {
        tracy_gpu_zone!("canvas");

        self.move_to_next_frame();

        self.metal_drawable_surface
            .as_mut()
            .expect("Not metal drawable surface now.")
            .surface
            .canvas()
    }

    fn resize(&mut self) {
        tracy_gpu_zone!("resize");

        let window_size = self.window.inner_size();
        self.metal_layer.set_drawable_size(CGSize::new(
            window_size.width as CGFloat,
            window_size.height as CGFloat,
        ));

        self.window.request_redraw();
    }

    fn create_vsync(&self, proxy: EventLoopProxy<UserEvent>) -> VSync {
        VSync::MacosDisplayLink(VSyncMacosDisplayLink::new(self.window(), proxy))
    }
}
