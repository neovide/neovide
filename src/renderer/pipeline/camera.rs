use bytemuck::{cast_slice, Pod, Zeroable};
use euclid::default::{Size2D, Transform3D};
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    *,
};

#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    row_height: f32,
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
}

pub struct Camera {
    uniform: CameraUniform,
    buffer: Buffer,
    pub bind_group_layout: BindGroupLayout,
    pub bind_group: BindGroup,
}

impl Camera {
    pub fn new(device: &Device) -> Self {
        let uniform = CameraUniform::new();

        let buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: cast_slice(&[uniform]),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });
        let uniform = CameraUniform::new();

        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
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

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
            label: Some("Camera Bind Group"),
        });
        Self {
            uniform,
            buffer,
            bind_group_layout,
            bind_group,
        }
    }

    pub fn update(&mut self, queue: &Queue, size: Size2D<f32>, row_height: f32) {
        self.uniform.view_proj = Transform3D::ortho(0.0, size.width, size.height, 0.0, -1.0, 1.0)
            .to_arrays()
            .into();

        self.uniform.row_height = row_height;
        queue.write_buffer(&self.buffer, 0, cast_slice(&[self.uniform]));
    }
}
