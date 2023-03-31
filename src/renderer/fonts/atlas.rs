use wgpu::{
    Device, TextureDescriptor, Extent3d, TextureDimension, TextureFormat, TextureUsages, Texture
};

struct Atlas {
    texture: Texture,
}

impl Atlas {
    pub fn new(device: Device) -> Self {
        let texture_size = Extent3d{ 
            width: 1024,
            height: 1024,
            depth_or_array_layers: 1,

        };
        let texture = device.create_texture(
            &TextureDescriptor {
                size: texture_size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: TextureFormat::Rgba8Unorm,
                usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
                label: Some("Glyph Atlas"),
            }
        );
        Self {
            texture
        }
    }
}
