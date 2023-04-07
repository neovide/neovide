use super::fonts::atlas::Atlas;
use super::pipeline::{Background, BackgroundFragment, Camera, GlyphFragment, Glyphs};
use bytemuck::{cast_slice, Pod, Zeroable};
use csscolorparser::Color;
use euclid::default::Size2D;
use hotwatch::Hotwatch;
use pollster::FutureExt as _;
use std::mem::size_of;
use std::ops::Range;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    *,
};
use winit::window::Window;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct QuadVertex {
    position: [f32; 2],
}

impl QuadVertex {
    const ATTRIBS: [VertexAttribute; 1] = vertex_attr_array![0 => Float32x2];

    pub fn desc<'a>() -> VertexBufferLayout<'a> {
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

pub struct WGpuRenderer {
    surface: Surface,
    pub device: Device,
    pub queue: Queue,
    surface_config: SurfaceConfiguration,
    size: winit::dpi::PhysicalSize<u32>,
    quad_vertex_buffer: Buffer,
    quad_index_buffer: Buffer,
    camera: Camera,
    background: Background,
    glyphs: Glyphs,
    validation_errors_shown: Arc<AtomicBool>,
    hotwatcher: Hotwatch,
}

pub struct MainRenderPass<'a> {
    render_pass: RenderPass<'a>,
    background: &'a Background,
    glyphs: &'a Glyphs,
    queue: &'a Queue,
    device: &'a Device,
    quad_vertex_buffer: &'a Buffer,
    quad_index_buffer: &'a Buffer,
}

impl WGpuRenderer {
    pub fn new(window: &Window) -> Self {
        async {
            let size = window.inner_size();

            // The instance is a handle to our GPU
            // Backends::all => Vulkan + Metal + DX12 + Browser WebGPU
            let instance = Instance::new(Backends::DX12);

            // # Safety
            // The surface needs to live as long as the window that created it.
            // TODO: Maybe move the window ownership here
            let surface = unsafe { instance.create_surface(&window) };

            let adapter = instance
                .request_adapter(&RequestAdapterOptions {
                    power_preference: PowerPreference::default(),
                    compatible_surface: Some(&surface),
                    force_fallback_adapter: false,
                })
                .await
                .unwrap();

            let (device, queue) = adapter
                .request_device(
                    &wgpu::DeviceDescriptor {
                        features: Features::empty(),
                        // WebGL doesn't support all of wgpu's features, so if
                        // we're building for the web we'll have to disable some.
                        limits: if cfg!(target_arch = "wasm32") {
                            Limits::downlevel_webgl2_defaults()
                        } else {
                            Limits::default()
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
                .filter(|f| f.describe().srgb == false)
                .next()
                .unwrap_or(formats[0]);
            let surface_config = SurfaceConfiguration {
                usage: TextureUsages::RENDER_ATTACHMENT,
                format: surface_format,
                width: size.width,
                height: size.height,
                present_mode: present_modes[0],
                alpha_mode: alpha_modes[0],
            };
            surface.configure(&device, &surface_config);

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
            let quad_vertex_buffer = device.create_buffer_init(&BufferInitDescriptor {
                label: Some("Quad Vertex Buffer"),
                contents: cast_slice(QUAD_VERTICES),
                usage: BufferUsages::VERTEX,
            });

            let validation_errors_shown = Arc::new(AtomicBool::new(false));
            // Disable validation errors for now
            // TODO: Only when hot reloading
            {
                let validation_errors_shown = validation_errors_shown.clone();
                device.on_uncaptured_error(move |error: Error| {
                    match error {
                        Error::OutOfMemory { source } => {
                            panic!("Out of memory {source}")
                        }
                        Error::Validation {
                            source,
                            description,
                        } => {
                            if !validation_errors_shown.load(Ordering::SeqCst) {
                                validation_errors_shown.store(true, Ordering::SeqCst);
                                for line in description.lines() {
                                    #[cfg(target_os = "windows")]
                                    print!("{}\r\n", line);
                                    #[cfg(not(target_os = "windows"))]
                                    println!("{}", line);
                                }
                            }
                        }
                    };
                });
            }

            let mut hotwatcher = Hotwatch::new().expect("The hotwatcher failed to initialze");

            let camera = Camera::new(&device);
            let background = Background::new(&device, &surface_config, &camera);
            let glyphs = Glyphs::new(&device, &surface_config, &camera, &mut hotwatcher);

            Self {
                surface,
                device,
                queue,
                surface_config,
                size,
                quad_vertex_buffer,
                quad_index_buffer,
                camera,
                background,
                glyphs,
                validation_errors_shown,
                hotwatcher,
            }
        }
        .block_on()
    }

    pub fn update_background_fragments(&mut self, fragments: Vec<BackgroundFragment>) {
        self.background.update(&self.device, &self.queue, fragments);
    }

    pub fn update_glyph_fragments(&mut self, fragments: Vec<GlyphFragment>) {
        self.glyphs.update(&self.device, &self.queue, fragments);
    }

    pub fn render<F>(
        &mut self,
        background: &Color,
        size: Size2D<f32>,
        row_height: f32,
        glyph_atlas: &Atlas,
        callback: F,
    ) where
        F: FnOnce(MainRenderPass),
    {
        // TODO: Deal with errors
        let output = self.surface.get_current_texture().unwrap();
        let mut view = output
            .texture
            .create_view(&TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
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
            self.camera.update(&self.queue, size, row_height);
            self.glyphs.update_bind_group(glyph_atlas, &self.device);
            render_pass.set_bind_group(0, &self.camera.bind_group, &[]);
            let reloaded =
                self.glyphs
                    .reload_pipeline(&self.device, &self.surface_config, &self.camera);
            if reloaded {
                self.validation_errors_shown.store(false, Ordering::SeqCst);
            }
            let background = &self.background;
            let glyphs = &self.glyphs;
            let queue = &self.queue;
            let device = &self.device;
            let quad_vertex_buffer = &self.quad_vertex_buffer;
            let quad_index_buffer = &self.quad_index_buffer;
            callback(MainRenderPass {
                render_pass,
                background,
                glyphs,
                queue,
                device,
                quad_vertex_buffer,
                quad_index_buffer,
            });
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
    }

    pub fn resize(&mut self, window: &Window) {
        let new_size = window.inner_size();

        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.surface_config.width = new_size.width;
            self.surface_config.height = new_size.height;
            self.surface.configure(&self.device, &self.surface_config);
        }
    }
}

impl<'a> MainRenderPass<'a> {
    pub fn draw_window(&mut self, background_range: &Range<u64>, glyph_range: &Range<u64>) {
        if background_range.is_empty() {
            return;
        }
        self.render_pass
            .set_vertex_buffer(0, self.quad_vertex_buffer.slice(..));
        self.render_pass
            .set_index_buffer(self.quad_index_buffer.slice(..), IndexFormat::Uint16);

        self.background
            .draw(&mut self.render_pass, background_range);

        if !glyph_range.is_empty() {
            self.glyphs.draw(&mut self.render_pass, glyph_range);
        }
    }
}
