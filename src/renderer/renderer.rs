use bytemuck::{cast_slice, Pod, Zeroable};
use csscolorparser::Color;
use euclid::default::{Size2D, Transform3D};
use pollster::FutureExt as _;
use std::ops::Range;
use std::mem::size_of;
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    vertex_attr_array, BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingType, Buffer, BufferAddress, BufferBindingType, BufferDescriptor,
    BufferUsages, CommandEncoder, CommandEncoderDescriptor, Device, Face, FrontFace, IndexFormat,
    LoadOp, MaintainBase, MultisampleState, Operations, PipelineLayoutDescriptor, PolygonMode,
    PrimitiveState, PrimitiveTopology, Queue, RenderPass, RenderPassColorAttachment,
    RenderPassDescriptor, RenderPipeline, RenderPipelineDescriptor, ShaderModuleDescriptor,
    ShaderSource, ShaderStages, SurfaceTexture, TextureView, TextureViewDescriptor,
    VertexAttribute, VertexBufferLayout, VertexState, VertexStepMode, COPY_BUFFER_ALIGNMENT,
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
        vertex_attr_array![1 => Float32x2, 2 => Float32, 3 => Float32x4];

    fn desc<'a>() -> VertexBufferLayout<'a> {
        VertexBufferLayout {
            array_stride: size_of::<Self>() as BufferAddress,
            step_mode: VertexStepMode::Instance,
            attributes: &Self::ATTRIBS,
        }
    }
}
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    pub row_height: f32,
    padding: [u32; 3],
}

impl CameraUniform {
    fn new() -> Self {
        Self {
            view_proj: Transform3D::identity().to_arrays().into(),
            row_height: 0.0,
            padding: [0; 3], 
        }
    }

    fn update_view_proj(&mut self, size: Size2D<f32>) {
        self.view_proj = Transform3D::ortho(0.0, size.width, size.height, 0.0, -1.0, 1.0)
            .to_arrays()
            .into();
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
    background_fragment_buffer: Buffer,
    camera_uniform: CameraUniform,
    camera_buffer: Buffer,
    camera_bind_group: BindGroup,
    pub background_pipeline: RenderPipeline,
}

pub struct MainRenderPass<'a> {
    render_pass: RenderPass<'a>,
    background_pipeline: &'a RenderPipeline,
    queue: &'a Queue,
    device: &'a Device,
    quad_vertex_buffer: &'a Buffer,
    quad_index_buffer: &'a Buffer,
    background_fragment_buffer: &'a Buffer,
}

pub fn create_background_fragment_buffer(device: &Device, size: BufferAddress) -> Buffer {
    device.create_buffer(&BufferDescriptor {
        label: Some("Background Instance Buffer"),
        size,
        usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
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
                .filter(|f| f.describe().srgb == false)
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
            let quad_vertex_buffer = device.create_buffer_init(&BufferInitDescriptor {
                label: Some("Quad Vertex Buffer"),
                contents: cast_slice(QUAD_VERTICES),
                usage: BufferUsages::VERTEX,
            });

            let background_fragment_buffer = create_background_fragment_buffer(&device, 16*1024);

            let background_shader = device.create_shader_module(ShaderModuleDescriptor {
                label: Some("Background Shader"),
                source: ShaderSource::Wgsl(include_str!("shaders/background.wgsl").into()),
            });

            let camera_uniform = CameraUniform::new();

            let camera_buffer = device.create_buffer_init(&BufferInitDescriptor {
                label: Some("Camera Buffer"),
                contents: cast_slice(&[camera_uniform]),
                usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            });

            let camera_bind_group_layout =
                device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                    entries: &[BindGroupLayoutEntry {
                        binding: 0,
                        visibility: ShaderStages::VERTEX,
                        ty: BindingType::Buffer {
                            ty: BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                    label: Some("Camera Bind Group Layout"),
                });

            let camera_bind_group = device.create_bind_group(&BindGroupDescriptor {
                layout: &camera_bind_group_layout,
                entries: &[BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                }],
                label: Some("Camera Bind Group"),
            });

            let background_pipeline_layout =
                device.create_pipeline_layout(&PipelineLayoutDescriptor {
                    label: Some("Background Pipeline Layout"),
                    bind_group_layouts: &[&camera_bind_group_layout],
                    push_constant_ranges: &[],
                });

            let background_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
                label: Some("Background Pipeline"),
                layout: Some(&background_pipeline_layout),
                vertex: VertexState {
                    module: &background_shader,
                    entry_point: "vs_main",
                    buffers: &[QuadVertex::desc(),
                    BackgroundFragment::desc()
                    ],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &background_shader,
                    entry_point: "fs_main",
                    targets: &[Some(wgpu::ColorTargetState {
                        format: config.format,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: PrimitiveState {
                    topology: PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: FrontFace::Ccw,
                    cull_mode: Some(Face::Back),
                    // Setting this to anything other than Fill requires Features::NON_FILL_POLYGON_MODE
                    polygon_mode: PolygonMode::Fill,
                    // Requires Features::DEPTH_CLIP_CONTROL
                    unclipped_depth: false,
                    // Requires Features::CONSERVATIVE_RASTERIZATION
                    conservative: false,
                },
                depth_stencil: None,
                multisample: MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview: None,
            });

            Self {
                surface,
                device,
                queue,
                config,
                size,
                quad_vertex_buffer,
                quad_index_buffer,
                background_fragment_buffer,
                camera_uniform,
                camera_buffer,
                camera_bind_group,
                background_pipeline,
            }
        }
        .block_on()
    }

    pub fn update_background_fragments(&mut self, fragments: Vec<BackgroundFragment>) {
        let contents = cast_slice(&fragments);

        let size = contents
            .len()
            .min(16 * 1024)
            .checked_next_power_of_two()
            .unwrap() as BufferAddress;
        if self.background_fragment_buffer.size() < size {
            self.background_fragment_buffer = create_background_fragment_buffer(&self.device, size);
        }
        self.queue.write_buffer(&self.background_fragment_buffer, 0, contents);
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

    pub fn render<F>(&mut self, background: &Color, size: Size2D<f32>, row_height: f32, callback: F)
    where
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
            self.camera_uniform.update_view_proj(size);
            self.camera_uniform.row_height = row_height;
            self.queue
                .write_buffer(&self.camera_buffer, 0, cast_slice(&[self.camera_uniform]));
            render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
            let background_pipeline = &self.background_pipeline;
            let queue = &self.queue;
            let device = &self.device;
            let quad_vertex_buffer = &self.quad_vertex_buffer;
            let quad_index_buffer = &self.quad_index_buffer;
            let background_fragment_buffer = &self.background_fragment_buffer;
            callback(MainRenderPass {
                render_pass,
                background_pipeline,
                queue,
                device,
                quad_vertex_buffer,
                quad_index_buffer,
                background_fragment_buffer,
            });
        }
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
}

impl<'a> MainRenderPass<'a> {
    pub fn draw_background(
        &mut self,
        range: &Range<u64>,
    ) {
        if range.is_empty() {
            return;
        }
        /*
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
        */
        let stride = BackgroundFragment::desc().array_stride;
        //let buffer_range = range.start * stride..range.end *stride;
        self.render_pass.set_pipeline(&self.background_pipeline);
        self.render_pass
            .set_vertex_buffer(0, self.quad_vertex_buffer.slice(..));
        self.render_pass.set_vertex_buffer(1, self.background_fragment_buffer.slice(..));
        self.render_pass
            .set_index_buffer(self.quad_index_buffer.slice(..), IndexFormat::Uint16);
        self.render_pass.draw_indexed(0..6, 0, range.start as u32..range.end as u32);
    }
}
