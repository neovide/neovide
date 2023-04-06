use enum_map::{enum_map, Enum, EnumMap};
use webrender_api::ImageFormat;
use wgpu::{Device, Extent3d, Texture, TextureDescriptor, TextureDimension, TextureUsages};
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
    fn bytes_per_pixel(&self) -> usize {
        self.to_image_format().bytes_per_pixel() as usize
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
}

struct AtlasTexture {
    //texture: Texture,
    texture_size: Extent3d,
    cpu_buffer: Vec<u8>,
    bytes_per_pixel: usize,
    current_xpos: usize,
    current_ypos: usize,
    current_row_height: usize,
}

impl AtlasTexture {
    pub fn new(texture_format: TextureFormat) -> Self {
        let width = 1024;
        let height = 1024;
        let texture_size = Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        /*
        let texture = device.create_texture(&TextureDescriptor {
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            label: Some("Glyph Atlas"),
        });
        */
        let bytes_per_pixel = texture_format.bytes_per_pixel();
        let buffer_size = (width * height) as usize * bytes_per_pixel;
        let mut cpu_buffer = Vec::with_capacity(buffer_size);
        unsafe {
            cpu_buffer.set_len(buffer_size);
        }

        Self {
            //texture
            texture_size,
            cpu_buffer,
            bytes_per_pixel,
            current_xpos: 0,
            current_ypos: 0,
            current_row_height: 0,
        }
    }

    fn add_glyph(&mut self, glyph: &RasterizedGlyph) {
        let width = glyph.width as usize;
        let height = glyph.height as usize;
        if self.current_xpos + width > self.texture_size.width as usize {
            self.current_ypos += self.current_row_height;
            self.current_row_height = 0;
        }
        let row_start_byte = self.current_xpos * self.bytes_per_pixel;
        let row_end_byte = (self.current_xpos + width) * self.bytes_per_pixel;
        let dst_rows = self
            .cpu_buffer
            .chunks_mut(self.texture_size.width as usize * self.bytes_per_pixel)
            .map(|row| &mut row[row_start_byte..row_end_byte]);
        let src_rows = glyph.bytes.chunks(width * self.bytes_per_pixel);
        for (src_row, dst_row) in src_rows.zip(dst_rows) {
            dst_row.copy_from_slice(src_row);
        }
        self.current_xpos += width;
        self.current_row_height = self.current_row_height.max(height);
    }
}

pub struct Atlas {
    textures: EnumMap<TextureFormat, Vec<AtlasTexture>>,
}

impl Atlas {
    pub fn new() -> Self {
        let textures = EnumMap::default();
        Self { textures }
    }

    pub fn add_glyph(&mut self, glyph: &RasterizedGlyph) {
        let can_use_rgb8 = true;
        let texture_format = glyph.format.image_format(can_use_rgb8).into();
        let textures = &mut self.textures[texture_format];
        if textures.is_empty() {
            textures.push(AtlasTexture::new(texture_format));
        }
        let mut texture = textures.last_mut().unwrap();
        texture.add_glyph(glyph);
    }
}
