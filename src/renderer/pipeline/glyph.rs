use super::Camera;
use crate::renderer::QuadVertex;
use bytemuck::{cast_slice, Pod, Zeroable};
use std::mem::size_of;
use std::ops::Range;
use wgpu::*;
use crate::renderer::fonts::atlas::{Atlas, TextureFormat};

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
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("Glyph Shader"),
        source: ShaderSource::Wgsl(include_str!("../shaders/glyph.wgsl").into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("Glyph Pipeline Layout"),
        bind_group_layouts: &[&camera.bind_group_layout, &glyph_bind_group_layout],
        push_constant_ranges: &[],
    });

    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("Glyph Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: "vs_main",
            buffers: &[QuadVertex::desc(), GlyphFragment::desc()],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs_main",
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_config.format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
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
    })
}

pub struct Glyphs {
    fragment_buffer: Buffer,
    pipeline: RenderPipeline,
    pub bind_group_layout: BindGroupLayout,
    bind_group: Option<BindGroup>,
}

impl Glyphs {
    pub fn new(device: &Device, surface_config: &SurfaceConfiguration, camera: &Camera) -> Self {
        let fragment_buffer = create_fragment_buffer(&device, 16 * 1024);
        let bind_group_layout =
            device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                entries: &[
                    BindGroupLayoutEntry {
                        binding: 0,
                        visibility: ShaderStages::FRAGMENT,
                        ty: BindingType::Texture {
                            multisampled: false,
                            view_dimension: TextureViewDimension::D2,
                            sample_type: TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    BindGroupLayoutEntry {
                        binding: 1,
                        visibility: ShaderStages::FRAGMENT,
                        ty: BindingType::Sampler(SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
                label: Some("Glyph bind group layout"),
            });
        let pipeline = create_pipeline(&device, &surface_config, &camera, &bind_group_layout);


        Self {
            fragment_buffer,
            pipeline,
            bind_group_layout,
            bind_group: None
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

    pub fn update_bind_group(&mut self, atlas: &Atlas, device: &Device) {
        let r8_textuers = &atlas.textures[TextureFormat::R8];
        if let Some(texture) = r8_textuers.first() {
            let texture_view = texture.texture.create_view(&TextureViewDescriptor::default());

            let sampler = device.create_sampler(&SamplerDescriptor {
                address_mode_u: AddressMode::ClampToEdge,
                address_mode_v: AddressMode::ClampToEdge,
                address_mode_w: AddressMode::ClampToEdge,
                mag_filter: FilterMode::Linear,
                min_filter: FilterMode::Nearest,
                mipmap_filter: FilterMode::Nearest,
                ..Default::default()
            });

            let bind_group = device.create_bind_group(
                &BindGroupDescriptor {
                    layout: &self.bind_group_layout,
                    entries: &[
                        BindGroupEntry {
                            binding: 0,
                            resource: BindingResource::TextureView(&texture_view),
                        },
                        BindGroupEntry {
                            binding: 1,
                            resource: BindingResource::Sampler(&sampler),
                        }
                    ],
                    label: Some("Glyph bind group"),
                }
            );
            self.bind_group = Some(bind_group);
        } else {
            self.bind_group = None;
        }
    }
}
