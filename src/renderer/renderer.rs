use bytemuck::{cast_slice, Pod, Zeroable};
use csscolorparser::Color;
use pollster::FutureExt as _;
use std::convert::TryInto;
use std::mem::size_of;
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    vertex_attr_array, Buffer, BufferAddress, BufferDescriptor, BufferUsages, CommandEncoder,
    CommandEncoderDescriptor, LoadOp, Operations, RenderPassColorAttachment, RenderPassDescriptor,
    SurfaceTexture, TextureViewDescriptor, VertexAttribute, VertexBufferLayout, VertexStepMode,
    COPY_BUFFER_ALIGNMENT,
};
use winit::window::Window;

/*
fn create_surface(
    windowed_context: &WindowedContext,
    gr_context: &mut DirectContext,
    fb_info: FramebufferInfo,
) -> Surface {
    let pixel_format = windowed_context.get_pixel_format();
    let size = windowed_context.window().inner_size();
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
        fb_info,
    );
    windowed_context.resize(size.into());
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
*/

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct QuadVertex {
    position: [f32; 2],
}

impl QuadVertex {
    const ATTRIBS: [VertexAttribute; 1] = vertex_attr_array![0 => Float32x2];

    fn desc<'a>() -> VertexBufferLayout<'a> {
        VertexBufferLayout {
            array_stride: size_of::<Self>() as BufferAddress,
            step_mode: VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

const QUAD_VERTICES: &[QuadVertex] = &[
    QuadVertex {
        position: [0.0, 0.0],
    },
    QuadVertex {
        position: [1.0, 0.0],
    },
    QuadVertex {
        position: [1.0, 1.0],
    },
    QuadVertex {
        position: [0.0, 1.0],
    },
];

const QUAD_INDICES: &[u16] = &[0, 3, 1, 3, 2, 1];

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct BackgroundFragment {
    pub position: [f32; 2],
    pub width: f32,
    pub color: [f32; 4],
}

impl BackgroundFragment {
    const ATTRIBS: [VertexAttribute; 3] =
        vertex_attr_array![0 => Float32x2, 1 => Float32, 2=> Float32x4];

    fn desc<'a>() -> VertexBufferLayout<'a> {
        VertexBufferLayout {
            array_stride: size_of::<Self>() as BufferAddress,
            step_mode: VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

pub struct WGpuRenderer {
    surface: wgpu::Surface,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: winit::dpi::PhysicalSize<u32>,
    quad_vertex_buffer: Buffer,
    quad_index_buffer: Buffer,
    surface_texture: Option<SurfaceTexture>,
    encoder: Option<CommandEncoder>,
}

impl WGpuRenderer {
    pub fn new(window: &Window) -> Self {
        async {
            let size = window.inner_size();

            // The instance is a handle to our GPU
            // Backends::all => Vulkan + Metal + DX12 + Browser WebGPU
            let instance = wgpu::Instance::new(wgpu::Backends::all());

            // # Safety
            // The surface needs to live as long as the window that created it.
            // TODO: Maybe move the window ownership here
            let surface = unsafe { instance.create_surface(&window) };

            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::default(),
                    compatible_surface: Some(&surface),
                    force_fallback_adapter: false,
                })
                .await
                .unwrap();

            let (device, queue) = adapter
                .request_device(
                    &wgpu::DeviceDescriptor {
                        features: wgpu::Features::empty(),
                        // WebGL doesn't support all of wgpu's features, so if
                        // we're building for the web we'll have to disable some.
                        limits: if cfg!(target_arch = "wasm32") {
                            wgpu::Limits::downlevel_webgl2_defaults()
                        } else {
                            wgpu::Limits::default()
                        },
                        label: None,
                    },
                    None, // Trace path
                )
                .await
                .unwrap();

            let present_modes = surface.get_supported_present_modes(&adapter);
            let alpha_modes = surface.get_supported_alpha_modes(&adapter);
            let formats = surface.get_supported_formats(&adapter);

            let surface_format = formats
                .iter()
                .copied()
                .filter(|f| f.describe().srgb)
                .next()
                .unwrap_or(formats[0]);
            let config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: surface_format,
                width: size.width,
                height: size.height,
                present_mode: present_modes[0],
                alpha_mode: alpha_modes[0],
            };
            surface.configure(&device, &config);

            let quad_vertex_buffer = device.create_buffer_init(&BufferInitDescriptor {
                label: Some("Quad Vertex Buffer"),
                contents: cast_slice(QUAD_VERTICES),
                usage: BufferUsages::VERTEX,
            });
            let quad_index_buffer = device.create_buffer_init(&BufferInitDescriptor {
                label: Some("Quad Index Buffer"),
                contents: cast_slice(QUAD_INDICES),
                usage: BufferUsages::INDEX,
            });

            Self {
                surface,
                device,
                queue,
                config,
                size,
                quad_vertex_buffer,
                quad_index_buffer,
                surface_texture: None,
                encoder: None,
            }
        }
        .block_on()
    }
    /*
    pub fn new(windowed_context: &WindowedContext) -> SkiaRenderer {
        gl::load_with(|s| windowed_context.get_proc_address(s));

        let interface = skia_safe::gpu::gl::Interface::new_load_with(|name| {
            if name == "eglGetCurrentDisplay" {
                return std::ptr::null();
            }
            windowed_context.get_proc_address(name)
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
            }
        };
        let surface = create_surface(windowed_context, &mut gr_context, fb_info);

        SkiaRenderer {
            gr_context,
            fb_info,
            surface,
        }
    }
    */

    /*
    pub fn canvas(&mut self) -> &mut Canvas {
        self.surface.canvas()
    }
    */

    pub fn begin_frame(&mut self, background: &Color) {
        // TODO: Deal with errors
        let output = self.surface.get_current_texture().unwrap();
        let view = output
            .texture
            .create_view(&TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });
        {
            let _render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(wgpu::Color {
                            r: background.r,
                            g: background.g,
                            b: background.b,
                            a: background.a,
                        }),
                        store: true,
                    },
                })],
                depth_stencil_attachment: None,
            });
        }
        self.surface_texture = Some(output);
        self.encoder = Some(encoder);
    }

    pub fn end_frame(&mut self) {
        let encoder = self.encoder.take().unwrap();
        let output = self.surface_texture.take().unwrap();
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
    }

    pub fn resize(&mut self, window: &Window) {
        let new_size = window.inner_size();

        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    pub fn draw_background(
        &mut self,
        fragments: Vec<BackgroundFragment>,
        buffer: Option<Buffer>,
    ) -> Buffer {
        let contents = cast_slice(&fragments);

        let size = contents
            .len()
            .min(16 * 1024)
            .checked_next_power_of_two()
            .unwrap() as BufferAddress;

        let buffer = if buffer.is_some() && buffer.as_ref().unwrap().size() >= size {
            buffer.unwrap()
        } else {
            self.device.create_buffer(&BufferDescriptor {
                label: Some("Background Instance Buffer"),
                size,
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        self.queue.write_buffer(&buffer, 0, contents);
        buffer
    }
}
