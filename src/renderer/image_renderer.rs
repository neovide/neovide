use crate::units::{to_skia_rect, GridPos, GridScale, GridSize, PixelRect, PixelSize};
use base64::{
    alphabet,
    engine::{
        general_purpose::{GeneralPurpose, GeneralPurposeConfig},
        DecodePaddingMode,
    },
    Engine,
};
use bytemuck::cast_ref;
use glamour::{Matrix3, Matrix4};
use serde::Deserialize;
use skia_safe::{
    canvas::SrcRectConstraint, matrix::Member, AlphaType, BlendMode, Canvas, ColorSpace, ColorType,
    Data, FilterMode, ISize, Image, ImageInfo, Matrix, MipmapMode, Paint, RSXform, Rect,
    SamplingOptions, M44,
};
use std::{collections::HashMap, ops::Range};

use super::kitty_image::{Display, ImageFormat};
use super::{KittyImage, Transmit};
use crate::units::{GridRect, PixelVec};

/// Don't add padding when encoding, and allow input with or without padding when decoding.
pub const NO_PAD_INDIFFERENT: GeneralPurposeConfig = GeneralPurposeConfig::new()
    .with_encode_padding(false)
    .with_decode_padding_mode(DecodePaddingMode::Indifferent);

/// A [`GeneralPurpose`] engine using the [`alphabet::STANDARD`] base64 alphabet and
/// [`NO_PAD_INDIFFERENT`] config.
pub const STANDARD_NO_PAD_INDIFFERENT: GeneralPurpose =
    GeneralPurpose::new(&alphabet::STANDARD, NO_PAD_INDIFFERENT);

pub struct ImageRenderer {
    loaded_images: HashMap<u64, Image>,
    visible_images: Vec<(u64, ImageRenderOpts)>,
    in_progress_image: Option<Transmit>,
    displayed_images: HashMap<(u32, u32), Display>,
}

#[derive(Clone)]
pub struct ImageFragment {
    pub dst_col: u32,
    pub src_row: u32,
    pub src_range: Range<u32>,
    pub image_id: u32,
    pub placement_id: u32,
}

struct VisibleImage<'a> {
    image: &'a Image,
    xform: Vec<RSXform>,
    tex: Vec<Rect>,
    inv_matrix: Matrix3<f32>,
    skia_matrix: M44,
    image_scale: GridScale,
}

pub struct FragmentRenderer<'a> {
    visible_images: HashMap<(u32, u32), VisibleImage<'a>>,
    renderer: &'a ImageRenderer,
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
            displayed_images: HashMap::new(),
        }
    }

    pub fn upload_image(&mut self, id: u64, data: &String) {
        log::info!("upload image");
        let image_data = STANDARD_NO_PAD_INDIFFERENT.decode(data).unwrap();
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

                let image_data = STANDARD_NO_PAD_INDIFFERENT.decode(&opts.data).unwrap();
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
            KittyImage::Display(opts) => {
                self.displayed_images
                    .insert((opts.id, opts.placement_id), opts);
            }
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

    pub fn begin_draw_image_fragments(&self) -> FragmentRenderer {
        FragmentRenderer::new(self)
    }
}

impl<'a> FragmentRenderer<'a> {
    pub fn new(renderer: &'a ImageRenderer) -> Self {
        Self {
            visible_images: HashMap::new(),
            renderer,
        }
    }

    pub fn draw(&mut self, fragments: &Vec<ImageFragment>, matrix: &Matrix, scale: &GridScale) {
        for fragment in fragments {
            let image = self
                .visible_images
                .entry((fragment.image_id, fragment.placement_id))
                .or_insert_with(|| {
                    // TODO: allow failures somehow
                    let display = self
                        .renderer
                        .displayed_images
                        .get(&(fragment.image_id, fragment.placement_id))
                        .unwrap();
                    // HACK: A bit of a hack use 32 bit ids
                    // Might be final if we drop the support for non-kitty images,
                    // Otherwise we can decide that kitty has some fixed upper bit id
                    let image = self
                        .renderer
                        .loaded_images
                        .get(&(fragment.image_id as u64))
                        .unwrap();
                    let x_scale = (display.columns as f32 * scale.width()) / image.width() as f32;
                    let y_scale = (display.rows as f32 * scale.height()) / image.height() as f32;
                    let matrix = Matrix3::from_scale((x_scale, y_scale).into());
                    let inv_matrix = matrix.inverse();
                    let skia_matrix = Matrix4::<f32>::from_mat3(matrix);
                    let skia_matrix = M44::col_major(cast_ref(skia_matrix.as_ref()));
                    let image_scale = GridScale::new(PixelSize::new(
                        image.width() as f32 / display.columns as f32,
                        image.height() as f32 / display.rows as f32,
                    ));
                    VisibleImage {
                        image,
                        xform: Vec::new(),
                        tex: Vec::new(),
                        skia_matrix,
                        inv_matrix,
                        image_scale,
                    }
                });
            let dest_pos = GridPos::new(fragment.dst_col, 0) * *scale
                + PixelVec::new(matrix[Member::TransX], matrix[Member::TransY]);
            let dest_pos = image.inv_matrix.transform_point2(dest_pos.to_untyped());
            image
                .xform
                .push(RSXform::new(1.0, 0.0, (dest_pos.x, dest_pos.y)));

            let src_min = GridPos::new(fragment.src_range.start, fragment.src_row);
            let src_max = GridPos::new(fragment.src_range.end, fragment.src_row + 1);
            let src_rect = GridRect::new(src_min, src_max) * image.image_scale;
            image.tex.push(to_skia_rect(&src_rect));
        }
    }

    pub fn flush(self, canvas: &Canvas) {
        for image in self.visible_images.values() {
            let paint = Paint::default();
            // Kitty uses Linear filtering, so use that here as well
            // It does not look very good when upscaling some images like logos though
            let sampling_options = SamplingOptions::new(FilterMode::Linear, MipmapMode::Linear);
            canvas.save();
            canvas.set_matrix(&image.skia_matrix);
            canvas.draw_atlas(
                image.image,
                &image.xform,
                &image.tex,
                None,
                BlendMode::Src,
                sampling_options,
                None,
                &paint,
            );
            canvas.restore();
        }
    }
}
