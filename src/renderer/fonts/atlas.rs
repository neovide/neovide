use crate::renderer::pipeline::Glyphs;
use enum_map::{enum_map, Enum, EnumMap};
use euclid::default::{Point2D, Rect, Size2D};
use std::num::NonZeroU32;
use webrender_api::ImageFormat;
use wgpu::{
    AddressMode, BindGroupDescriptor, BindGroupEntry, BindingResource, BufferAddress, Device,
    Extent3d, FilterMode, Origin3d, Queue, SamplerDescriptor, Texture, TextureDescriptor,
    TextureDimension, TextureUsages, TextureViewDescriptor,
};
use wr_glyph_rasterizer::RasterizedGlyph;

#[derive(Debug, Copy, Clone, Enum)]
pub enum TextureFormat {
    R8,
    R16,
    BGRA8,
    RGBAF32,
    RG8,
    RG16,
    RGBAI32,
    RGBA8,
}

impl From<ImageFormat> for TextureFormat {
    fn from(format: ImageFormat) -> TextureFormat {
        match format {
            ImageFormat::R8 => TextureFormat::R8,
            ImageFormat::R16 => TextureFormat::R16,
            ImageFormat::BGRA8 => TextureFormat::BGRA8,
            ImageFormat::RGBAF32 => TextureFormat::RGBAF32,
            ImageFormat::RG8 => TextureFormat::RG8,
            ImageFormat::RG16 => TextureFormat::RG16,
            ImageFormat::RGBAI32 => TextureFormat::RGBAI32,
            ImageFormat::RGBA8 => TextureFormat::RGBA8,
        }
    }
}

impl TextureFormat {
    fn bytes_per_pixel(&self) -> u32 {
        self.to_image_format().bytes_per_pixel() as u32
    }

    fn to_image_format(&self) -> ImageFormat {
        match self {
            TextureFormat::R8 => ImageFormat::R8,
            TextureFormat::R16 => ImageFormat::R16,
            TextureFormat::BGRA8 => ImageFormat::BGRA8,
            TextureFormat::RGBAF32 => ImageFormat::RGBAF32,
            TextureFormat::RG8 => ImageFormat::RG8,
            TextureFormat::RG16 => ImageFormat::RG16,
            TextureFormat::RGBAI32 => ImageFormat::RGBAI32,
            TextureFormat::RGBA8 => ImageFormat::RGBA8,
        }
    }

    fn to_wgpu(&self) -> wgpu::TextureFormat {
        match self {
            TextureFormat::R8 => wgpu::TextureFormat::R8Unorm,
            TextureFormat::R16 => wgpu::TextureFormat::R16Unorm,
            TextureFormat::BGRA8 => wgpu::TextureFormat::Bgra8Unorm,
            TextureFormat::RGBAF32 => wgpu::TextureFormat::Rgba32Float,
            TextureFormat::RG8 => wgpu::TextureFormat::Rg8Unorm,
            TextureFormat::RG16 => wgpu::TextureFormat::Rg16Unorm,
            TextureFormat::RGBAI32 => wgpu::TextureFormat::Rgba32Sint,
            TextureFormat::RGBA8 => wgpu::TextureFormat::Rgba8Unorm,
        }
    }
}

pub struct AtlasCoordinate {
    pub rect: Rect<f32>,
    pub texture_id: u32,
}

pub struct AtlasTexture {
    pub texture: Texture,
    texture_size: Extent3d,
    cpu_buffer: Vec<u8>,
    bytes_per_pixel: u32,
    current_pos: Point2D<u32>,
    current_row_height: u32,
    upload_pos: Origin3d,
    dirty: bool,
}

impl AtlasTexture {
    pub fn new(device: &Device, texture_format: TextureFormat) -> Self {
        let width = 1024;
        let height = 1024;
        let texture_size = Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&TextureDescriptor {
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: texture_format.to_wgpu(),
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            label: Some("Glyph Atlas"),
        });
        let bytes_per_pixel = texture_format.bytes_per_pixel();
        let buffer_size = (width * height) * bytes_per_pixel;
        let mut cpu_buffer = Vec::with_capacity(buffer_size as usize);
        unsafe {
            cpu_buffer.set_len(buffer_size as usize);
        }

        Self {
            texture,
            texture_size,
            cpu_buffer,
            bytes_per_pixel,
            current_pos: Point2D::zero(),
            current_row_height: 0,
            upload_pos: Origin3d::ZERO,
            dirty: false,
        }
    }

    fn add_glyph(&mut self, glyph: &RasterizedGlyph) -> AtlasCoordinate {
        let width = glyph.width as u32;
        let height = glyph.height as u32;
        if self.current_pos.x + width > self.texture_size.width {
            self.current_pos.y += self.current_row_height;
            self.current_row_height = 0;
        };
        let rect = Rect::new(self.current_pos, Size2D::new(width, height));
        let rect = rect.to_f32().scale(
            1.0 / self.texture_size.width as f32,
            1.0 / self.texture_size.height as f32,
        );

        let row_start_byte = (self.current_pos.x * self.bytes_per_pixel) as usize;
        let row_end_byte = ((self.current_pos.x + width) * self.bytes_per_pixel) as usize;
        let dst_rows = self
            .cpu_buffer
            .chunks_mut((self.texture_size.width * self.bytes_per_pixel) as usize)
            .map(|row| &mut row[row_start_byte..row_end_byte]);
        let src_rows = glyph.bytes.chunks((width * self.bytes_per_pixel) as usize);
        for (src_row, dst_row) in src_rows.zip(dst_rows) {
            dst_row.copy_from_slice(src_row);
        }
        self.current_pos.x += width;
        self.current_row_height = self.current_row_height.max(height);
        self.dirty = true;

        AtlasCoordinate {
            rect,
            texture_id: 0,
        }
    }

    fn upload(&mut self, queue: &Queue) {
        if !self.dirty {
            return;
        }

        let start_byte = self.upload_pos.y * self.bytes_per_pixel as u32;
        let copy_extent = Extent3d {
            width: self.texture_size.width,
            height: (self.current_pos.y + self.current_row_height) as u32 - self.upload_pos.y,
            depth_or_array_layers: 1,
        };

        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: self.upload_pos,
                aspect: wgpu::TextureAspect::All,
            },
            &self.cpu_buffer
                [0..(copy_extent.height * copy_extent.width * self.bytes_per_pixel) as usize],
            wgpu::ImageDataLayout {
                offset: (self.upload_pos.y * self.bytes_per_pixel) as BufferAddress,
                bytes_per_row: NonZeroU32::new(
                    self.bytes_per_pixel as u32 * self.texture_size.width,
                ),
                rows_per_image: None,
            },
            copy_extent,
        );
        // Always make sure that we re-upload the current row
        self.upload_pos.y += copy_extent.height - self.current_row_height as u32;
        self.dirty = false;
    }
}

pub struct Atlas {
    pub textures: EnumMap<TextureFormat, Vec<AtlasTexture>>,
}

impl Atlas {
    pub fn new() -> Self {
        let textures = EnumMap::default();
        Self { textures }
    }

    pub fn add_glyph(&mut self, device: &Device, glyph: &RasterizedGlyph) -> AtlasCoordinate {
        let can_use_rgb8 = true;
        let texture_format = glyph.format.image_format(can_use_rgb8).into();
        let textures = &mut self.textures[texture_format];
        if textures.is_empty() {
            textures.push(AtlasTexture::new(device, texture_format));
        }
        let mut texture = textures.last_mut().unwrap();
        texture.add_glyph(glyph)
    }

    pub fn upload(&mut self, queue: &Queue) {
        for texture in self.textures.values_mut().flat_map(|v| v.iter_mut()) {
            texture.upload(queue)
        }
    }
}
