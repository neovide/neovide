use std::{rc::Rc, sync::Arc};

use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_app_kit::NSColorSpace;
use objc2_core_foundation::{CGFloat, CGSize};
use objc2_metal::{
    MTLCommandBuffer, MTLCommandQueue, MTLCreateSystemDefaultDevice, MTLDevice, MTLDrawable,
    MTLTexture,
};
use objc2_quartz_core::{CAMetalDrawable, CAMetalLayer};
use skia_safe::{
    gpu::{
        self,
        mtl::{BackendContext, TextureInfo},
        surfaces::wrap_backend_render_target,
        DirectContext, SurfaceOrigin,
    },
    Canvas, ColorSpace, ColorType, PixelGeometry, Surface, SurfaceProps, SurfacePropsFlags,
};
use winit::{event_loop::EventLoopProxy, window::Window};

use crate::{
    platform::macos::get_ns_window,
    profiling::tracy_gpu_zone,
    renderer::{RendererSettings, SkiaRenderer, VSync},
    window::EventPayload,
};

use super::Settings;

struct MetalDrawableSurface {
    pub drawable: Retained<ProtocolObject<dyn CAMetalDrawable>>,
    pub surface: Surface,
}

impl MetalDrawableSurface {
    fn new(
        drawable: Retained<ProtocolObject<dyn CAMetalDrawable>>,
        context: &mut DirectContext,
        settings: &Settings,
    ) -> MetalDrawableSurface {
        tracy_gpu_zone!("MetalDrawableSurface.new");

        let texture = drawable.texture();
        let texture_info = unsafe { TextureInfo::new(Retained::as_ptr(&texture).cast()) };
        let backend_render_target = gpu::backend_render_targets::make_mtl(
            (texture.width() as i32, texture.height() as i32),
            &texture_info,
        );

        let render_settings = settings.get::<RendererSettings>();

        let surface_props = SurfaceProps::new_with_text_properties(
            SurfacePropsFlags::default(),
            PixelGeometry::default(),
            render_settings.text_contrast,
            render_settings.text_gamma,
        );

        let surface = wrap_backend_render_target(
            context,
            &backend_render_target,
            SurfaceOrigin::TopLeft,
            ColorType::BGRA8888,
            ColorSpace::new_srgb(),
            Some(surface_props).as_ref(),
        )
        .expect("Failed to create skia surface with metal drawable.");

        MetalDrawableSurface { drawable, surface }
    }

    fn mtl_drawable(&self) -> &ProtocolObject<dyn MTLDrawable> {
        self.drawable.as_ref()
    }
}

pub struct MetalSkiaRenderer {
    window: Rc<Window>,
    _device: Retained<ProtocolObject<dyn MTLDevice>>,
    command_queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    metal_layer: Retained<CAMetalLayer>,
    _backend: BackendContext,
    context: DirectContext,
    metal_drawable_surface: Option<MetalDrawableSurface>,
    settings: Arc<Settings>,
}

impl MetalSkiaRenderer {
    pub fn new(window: Rc<Window>, srgb: bool, vsync: bool, settings: Arc<Settings>) -> Self {
        log::info!("Initialize MetalSkiaRenderer...");

        let draw_size = window.inner_size();
        let ns_window = get_ns_window(&window);

        ns_window.setColorSpace(Some(
            if srgb {
                NSColorSpace::sRGBColorSpace()
            } else {
                NSColorSpace::deviceRGBColorSpace()
            }
            .as_ref(),
        ));

        let device =
            MTLCreateSystemDefaultDevice().expect("Failed to create Metal system default device.");
        let metal_layer = {
            let metal_layer = CAMetalLayer::new();
            metal_layer.setDevice(Some(&device));
            metal_layer.setPresentsWithTransaction(false);
            metal_layer.setFramebufferOnly(false);
            metal_layer.setDisplaySyncEnabled(vsync);
            metal_layer.setOpaque(false);

            let ns_view = ns_window.contentView().unwrap();
            ns_view.setWantsLayer(true);
            ns_view.setLayer(Some(&metal_layer));

            metal_layer
                .setDrawableSize(CGSize::new(draw_size.width as f64, draw_size.height as f64));
            metal_layer
        };

        let command_queue = device
            .newCommandQueue()
            .expect("Failed to create command queue.");

        let backend = unsafe {
            BackendContext::new(
                Retained::as_ptr(&device).cast(),
                Retained::as_ptr(&command_queue).cast(),
            )
        };

        let context = gpu::direct_contexts::make_metal(&backend, None).unwrap();

        MetalSkiaRenderer {
            window,
            _device: device,
            metal_layer,
            command_queue,
            _backend: backend,
            context,
            metal_drawable_surface: None,
            settings,
        }
    }

    fn move_to_next_frame(&mut self) {
        tracy_gpu_zone!("move_to_next_frame");

        let drawable = {
            self.metal_layer
                .nextDrawable()
                .expect("Failed to get next drawable of metal layer.")
        };

        self.metal_drawable_surface = Some(MetalDrawableSurface::new(
            drawable,
            &mut self.context,
            &self.settings,
        ));
    }
}

impl SkiaRenderer for MetalSkiaRenderer {
    fn window(&self) -> Rc<Window> {
        Rc::clone(&self.window)
    }

    fn flush(&mut self) {
        tracy_gpu_zone!("flush");

        self.context.flush_and_submit();
    }

    fn swap_buffers(&mut self) {
        tracy_gpu_zone!("swap buffers");

        let command_buffer = self
            .command_queue
            .commandBuffer()
            .expect("Failed to create command buffer.");
        command_buffer.presentDrawable(
            self.metal_drawable_surface
                .as_mut()
                .expect("No drawable surface now.")
                .mtl_drawable(),
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
        self.metal_layer.setDrawableSize(CGSize::new(
            window_size.width as CGFloat,
            window_size.height as CGFloat,
        ));

        self.window.request_redraw();
    }

    fn create_vsync(&self, _proxy: EventLoopProxy<EventPayload>) -> VSync {
        VSync::MacosMetal()
    }
}
