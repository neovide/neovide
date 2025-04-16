use crate::units::{to_skia_rect, GridPos, GridScale, GridSize, PixelRect, PixelSize};
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use serde::Deserialize;
use skia_safe::ISize;
use skia_safe::{
    canvas::SrcRectConstraint, AlphaType, Canvas, ColorSpace, ColorType, Data, Image, ImageInfo,
    Paint,
};
use std::{collections::HashMap, ops::Range};

use super::kitty_image::ImageFormat;
use super::{KittyImage, Transmit};

pub struct ImageRenderer {
    loaded_images: HashMap<u64, Image>,
    visible_images: Vec<(u64, ImageRenderOpts)>,
    in_progress_image: Option<Transmit>,
}

#[derive(Clone)]
pub struct ImageFragment {
    pub dst_col: u32,
    pub src_row: u32,
    pub src_range: Range<u32>,
    pub id: u64,
}

#[derive(Clone, Debug, PartialEq, Default, Deserialize)]
// Units are pixels
pub struct Crop {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

impl From<&Crop> for PixelRect<f32> {
    fn from(val: &Crop) -> Self {
        PixelRect::from_origin_and_size(
            (val.x as f32, val.y as f32).into(),
            (val.width as f32, val.height as f32).into(),
        )
    }
}

#[derive(Clone, Debug, PartialEq, Default, Deserialize)]
// Units are cells
pub struct Pos {
    row: i32,
    col: i32,
}

impl From<&Pos> for GridPos<i32> {
    fn from(val: &Pos) -> Self {
        GridPos::new(val.col, val.row)
    }
}

#[derive(Clone, Debug, PartialEq, Default, Deserialize)]
// Units are cells
pub struct Size {
    width: i32,
    height: i32,
}

impl From<&Size> for GridSize<i32> {
    fn from(val: &Size) -> Self {
        GridSize::new(val.width, val.height)
    }
}

#[derive(Clone, Debug, PartialEq, Default, Deserialize)]
pub struct ImageRenderOpts {
    pub crop: Option<Crop>,
    pub pos: Option<Pos>,
    pub size: Option<Size>,
}

impl ImageRenderer {
    pub fn new() -> Self {
        Self {
            loaded_images: HashMap::new(),
            visible_images: Vec::new(),
            in_progress_image: None,
        }
    }

    pub fn upload_image(&mut self, id: u64, data: &String) {
        log::info!("upload image");
        let image_data = STANDARD_NO_PAD.decode(data).unwrap();
        // TODO: don't copy
        let image_data = Data::new_copy(&image_data);
        let image = Image::from_encoded(image_data).unwrap();
        log::info!("Image loaded {:?}", image);
        self.loaded_images.insert(id, image);
    }

    pub fn kitty_image(&mut self, opts: KittyImage) {
        match opts {
            KittyImage::Transmit(opts) => {
                let opts = if let Some(in_progress) = &mut self.in_progress_image {
                    in_progress.data += &opts.data;
                    if opts.more_chunks {
                        return;
                    }
                    in_progress
                } else if opts.more_chunks {
                    self.in_progress_image = Some(opts);
                    return;
                } else {
                    &opts
                };

                let image_data = STANDARD_NO_PAD.decode(&opts.data).unwrap();
                // TODO: don't copy
                let image_data = Data::new_copy(&image_data);
                let dimensions = ISize::new(opts.width as i32, opts.height as i32);
                let image = match opts.format {
                    ImageFormat::Png => Image::from_encoded(image_data).unwrap(),
                    ImageFormat::Rgb => {
                        let image_info = ImageInfo::new(
                            dimensions,
                            ColorType::RGB888x,
                            AlphaType::Opaque,
                            Some(ColorSpace::new_srgb()),
                        );
                        skia_safe::images::raster_from_data(
                            &image_info,
                            image_data,
                            dimensions.width as usize * 3,
                        )
                        .unwrap()
                    }
                    ImageFormat::Rgba => {
                        let image_info = ImageInfo::new(
                            dimensions,
                            ColorType::RGBA8888,
                            AlphaType::Premul,
                            Some(ColorSpace::new_srgb()),
                        );
                        skia_safe::images::raster_from_data(
                            &image_info,
                            image_data,
                            dimensions.width as usize * 4,
                        )
                        .unwrap()
                    }
                };
                self.loaded_images.insert(opts.id.into(), image);
                self.in_progress_image = None;
            }
            KittyImage::Delete => {}
            KittyImage::Display(_opts) => {}
        }
    }

    pub fn show_image(&mut self, id: u64, opts: ImageRenderOpts) {
        self.visible_images.push((id, opts));
    }

    pub fn draw_frame(&self, canvas: &Canvas, grid_scale: GridScale) {
        for (id, opts) in &self.visible_images {
            if let Some(image) = self.loaded_images.get(id) {
                let pos = opts
                    .pos
                    .as_ref()
                    .map_or(GridPos::default(), |pos| pos.into())
                    * grid_scale;
                let size = opts.size.as_ref().map_or_else(
                    || {
                        let image_dimensons = image.dimensions();
                        PixelSize::new(image_dimensons.width as f32, image_dimensons.height as f32)
                    },
                    |size| GridSize::from(size) * grid_scale,
                );
                let dst = PixelRect::from_origin_and_size(pos, size);
                let crop = opts.crop.as_ref().map(|crop| (to_skia_rect(&crop.into())));
                let src = crop.as_ref().map(|crop| (crop, SrcRectConstraint::Strict));
                let paint = Paint::default();
                canvas.draw_image_rect(image, src, to_skia_rect(&dst), &paint);
            }
        }
    }
}
