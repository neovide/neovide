use super::Camera;
use crate::renderer::fonts::atlas::{Atlas, TextureFormat};
use crate::renderer::QuadVertex;
use bytemuck::{cast_slice, Pod, Zeroable};
use hotwatch::{Event, Hotwatch};
use nvim_rs::create;
use std::borrow::Cow;
use std::fs::read_to_string;
use std::mem::size_of;
use std::ops::Range;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use wgpu::*;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct GlyphFragment {
    pub position: [f32; 2],
    pub width: f32,
    pub color: [f32; 4],
    pub uv: [f32; 4],
    pub texture: u32,
}

impl GlyphFragment {
    const ATTRIBS: [VertexAttribute; 5] = vertex_attr_array![1 => Float32x2, 2 => Float32, 3 => Float32x4, 4 => Float32x4, 5 => Uint32];

    fn desc<'a>() -> VertexBufferLayout<'a> {
        VertexBufferLayout {
            array_stride: size_of::<Self>() as BufferAddress,
            step_mode: VertexStepMode::Instance,
            attributes: &Self::ATTRIBS,
        }
    }
}

pub fn create_fragment_buffer(device: &Device, size: BufferAddress) -> Buffer {
    device.create_buffer(&BufferDescriptor {
        label: Some("Glyph Instance Buffer"),
        size,
        usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_pipeline(
    device: &Device,
    surface_config: &SurfaceConfiguration,
    camera: &Camera,
    glyph_bind_group_layout: &BindGroupLayout,
) -> RenderPipeline {
    let file_path = "src/renderer/shaders/glyph.wgsl";
    let contents = read_to_string(file_path).expect("Should have been able to read the file");

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("Glyph Shader"),
        //source: ShaderSource::Wgsl(include_str!("../shaders/glyph.wgsl").into()),
        source: ShaderSource::Wgsl(Cow::from(contents)),
    });

    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("Glyph Pipeline Layout"),
        bind_group_layouts: &[&camera.bind_group_layout, &glyph_bind_group_layout],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("Glyph Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: "vs_main",
            buffers: &[QuadVertex::desc(), GlyphFragment::desc()],
        },
        fragment: Some(FragmentState {
            module: &shader,
            entry_point: "fs_main",
            targets: &[Some(ColorTargetState {
                format: surface_config.format,
                blend: Some(BlendState::ALPHA_BLENDING),
                write_mask: ColorWrites::ALL,
            })],
        }),
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: FrontFace::Ccw,
            cull_mode: Some(Face::Back),
            polygon_mode: PolygonMode::Fill,
            unclipped_depth: false,
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
    pipeline
}

pub struct Glyphs {
    fragment_buffer: Buffer,
    pipeline: RenderPipeline,
    pub bind_group_layout: BindGroupLayout,
    bind_group: Option<BindGroup>,
    shader_changed: Arc<AtomicBool>,
}

impl Glyphs {
    pub fn new(
        device: &Device,
        surface_config: &SurfaceConfiguration,
        camera: &Camera,
        hotwatch: &mut Hotwatch,
    ) -> Self {
        let fragment_buffer = create_fragment_buffer(&device, 16 * 1024);
        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        multisampled: false,
                        view_dimension: TextureViewDimension::D2,
                        sample_type: TextureSampleType::Float { filterable: false },
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::NonFiltering),
                    count: None,
                },
            ],
            label: Some("Glyph bind group layout"),
        });
        let pipeline = create_pipeline(&device, &surface_config, &camera, &bind_group_layout);

        let shader_changed = Arc::new(AtomicBool::new(false));
        {
            let shader_changed = shader_changed.clone();
            hotwatch
                .watch("src/renderer/shaders/glyph.wgsl", move |event: Event| {
                    if let Event::Write(path) | Event::Create(path) = event {
                        shader_changed.store(true, Ordering::SeqCst)
                    }
                })
                .expect("failed to watch file!");
        }

        Self {
            fragment_buffer,
            pipeline,
            bind_group_layout,
            bind_group: None,
            shader_changed,
        }
    }

    pub fn update(&mut self, device: &Device, queue: &Queue, fragments: Vec<GlyphFragment>) {
        let contents = cast_slice(&fragments);

        let size = contents
            .len()
            .max(16 * 1024)
            .checked_next_power_of_two()
            .unwrap() as BufferAddress;
        if self.fragment_buffer.size() < size {
            self.fragment_buffer = create_fragment_buffer(device, size);
        }
        queue.write_buffer(&self.fragment_buffer, 0, contents);
    }

    pub fn draw<'a>(&'a self, render_pass: &mut RenderPass<'a>, range: &Range<u64>) {
        let stride = GlyphFragment::desc().array_stride;
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(1, self.bind_group.as_ref().unwrap(), &[]);
        render_pass.set_vertex_buffer(1, self.fragment_buffer.slice(..));
        render_pass.draw_indexed(0..6, 0, range.start as u32..range.end as u32);
    }

    pub fn reload_pipeline(
        &mut self,
        device: &Device,
        surface_config: &SurfaceConfiguration,
        camera: &Camera,
    ) -> bool {
        if self.shader_changed.load(Ordering::SeqCst) {
            self.shader_changed.store(false, Ordering::SeqCst);
            self.pipeline =
                create_pipeline(device, surface_config, camera, &self.bind_group_layout);
            true
        } else {
            false
        }
    }

    pub fn update_bind_group(&mut self, atlas: &Atlas, device: &Device) {
        let r8_textuers = &atlas.textures[TextureFormat::R8];
        if let Some(texture) = r8_textuers.first() {
            let texture_view = texture
                .texture
                .create_view(&TextureViewDescriptor::default());

            let sampler = device.create_sampler(&SamplerDescriptor {
                address_mode_u: AddressMode::ClampToEdge,
                address_mode_v: AddressMode::ClampToEdge,
                address_mode_w: AddressMode::ClampToEdge,
                mag_filter: FilterMode::Nearest,
                min_filter: FilterMode::Nearest,
                mipmap_filter: FilterMode::Nearest,
                ..Default::default()
            });

            let bind_group = device.create_bind_group(&BindGroupDescriptor {
                layout: &self.bind_group_layout,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: BindingResource::TextureView(&texture_view),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: BindingResource::Sampler(&sampler),
                    },
                ],
                label: Some("Glyph bind group"),
            });
            self.bind_group = Some(bind_group);
        } else {
            self.bind_group = None;
        }
    }
}
