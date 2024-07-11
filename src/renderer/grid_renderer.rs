use std::sync::Arc;

use log::trace;

use glam::Vec4;
use palette::{named, Hsv, IntoColor, LinSrgba, Srgba, WithAlpha};
use vide::{Layer, Quad, Text};

use crate::{
    editor::{Colors, Style, UnderlineStyle},
    profiling::tracy_zone,
    renderer::RendererSettings,
    settings::*,
    units::{GridPos, GridScale, GridSize, PixelPos, PixelRect, PixelVec},
};

use super::fonts::font_options::FontOptions;

pub struct GridRenderer {
    //pub shaper: CachingShaper,
    pub default_style: Arc<Style>,
    pub em_size: f32,
    pub grid_scale: GridScale,
    pub is_ready: bool,
}

/// Struct with named fields to be returned from draw_background
pub struct BackgroundInfo {
    pub custom_color: bool,
    // This should probably be used
    #[allow(unused)]
    pub transparent: bool,
}

impl GridRenderer {
    pub fn new(scale_factor: f64) -> Self {
        //let mut shaper = CachingShaper::new(scale_factor as f32);
        let default_style = Arc::new(Style::new(Colors::new(
            Some(named::WHITE.with_alpha(255).into()),
            Some(named::BLACK.with_alpha(255).into()),
            Some(named::GREY.with_alpha(255).into()),
        )));
        // let em_size = shaper.current_size();
        // let font_dimensions = shaper.font_base_dimensions();
        let em_size = 13.3333333;
        let font_dimensions = (8.205129, 17.0).into();

        GridRenderer {
            //shaper,
            default_style,
            em_size,
            grid_scale: GridScale(font_dimensions),
            is_ready: false,
        }
    }

    pub fn font_names(&self) -> Vec<String> {
        //self.shaper.font_names()
        Vec::new()
    }

    pub fn handle_scale_factor_update(&mut self, scale_factor: f64) {
        // self.shaper.update_scale_factor(scale_factor as f32);
        self.update_font_dimensions();
    }

    pub fn update_font(&mut self, guifont_setting: &str) {
        // self.shaper.update_font(guifont_setting);
        self.update_font_dimensions();
    }

    pub fn update_font_options(&mut self, options: FontOptions) {
        // self.shaper.update_font_options(options);
        self.update_font_dimensions();
    }

    pub fn update_linespace(&mut self, linespace_setting: f32) {
        // self.shaper.update_linespace(linespace_setting);
        self.update_font_dimensions();
    }

    fn update_font_dimensions(&mut self) {
        // TODO
        //self.em_size = self.shaper.current_size();
        //self.grid_scale = GridScale(self.shaper.font_base_dimensions());
        //self.grid_scale = GridScale((10.0, 10.0).into());
        self.is_ready = true;
        trace!("Updated font dimensions: {:?}", self.grid_scale.0);
    }

    fn compute_text_region(&self, grid_position: GridPos<i32>, cell_width: i32) -> PixelRect<f32> {
        let pos = grid_position.cast() * self.grid_scale;
        let size = GridSize::new(cell_width, 1).cast() * self.grid_scale;
        PixelRect::from_origin_and_size(pos, size)
    }

    pub fn get_default_background(&self) -> Srgba {
        self.default_style.colors.background.unwrap()
    }

    /// Draws a single background cell with the same style
    pub fn draw_background(
        &self,
        grid_position: GridPos<i32>,
        cell_width: i32,
        transform: PixelVec<f32>,
        style: &Option<Arc<Style>>,
        layer: &mut Layer,
    ) -> BackgroundInfo {
        tracy_zone!("draw_background");
        let debug = SETTINGS.get::<RendererSettings>().debug_renderer;
        if style.is_none() && !debug {
            return BackgroundInfo {
                custom_color: false,
                transparent: false,
            };
        }

        let region = self
            .compute_text_region(grid_position, cell_width)
            .translate(transform);
        let style = style.as_ref().unwrap_or(&self.default_style);

        let color: Srgba = if debug {
            Hsv::new(rand::random::<f32>() * 360.0, 0.3, 0.3).into_color()
        } else {
            style.background(&self.default_style.colors)
        }
        .with_alpha(if style.blend > 0 {
            (100 - style.blend) as f32 / 100.0
        } else {
            1.0
        });

        let custom_color = color != self.default_style.colors.background.unwrap();
        if custom_color {
            let top_left: mint::Vector2<_> = region.min.to_vector().into();
            let bottom_right: mint::Vector2<_> = region.max.to_vector().into();
            let color: LinSrgba = color.into_color();
            let quad = Quad::new(
                top_left.into(),
                bottom_right.into(),
                Vec4::from_array(color.into()),
            );
            layer.add_quad(quad);
        }

        BackgroundInfo {
            custom_color,
            transparent: style.blend > 0,
        }
    }

    /// Draws some foreground text.
    /// Returns true if any text was actually drawn.
    pub fn draw_foreground(
        &self,
        text: &str,
        grid_position: GridPos<i32>,
        cell_width: i32,
        transform: PixelVec<f32>,
        style: &Option<Arc<Style>>,
        layer: &mut Layer,
    ) -> bool {
        tracy_zone!("draw_foreground");
        let pos = grid_position.cast() * self.grid_scale + transform;
        let size = GridSize::new(cell_width, 0).cast() * self.grid_scale;
        let width = size.width;

        let style = style.as_ref().unwrap_or(&self.default_style);
        let mut drawn = false;

        // We don't want to clip text in the x position, only the y so we add a buffer of 1
        // character on either side of the region so that we clip vertically but not horizontally.
        let clip_position = (grid_position.x.saturating_sub(1), grid_position.y).into();
        let region = self.compute_text_region(clip_position, cell_width + 2);

        // TODO: Draw underline
        // if let Some(underline_style) = style.underline {
        //     let line_position = self.grid_scale.0.height - self.shaper.underline_position();
        //     let p1 = pos + PixelVec::new(0.0, line_position);
        //     let p2 = pos + PixelVec::new(width, line_position);
        //
        //     self.draw_underline(canvas, style, underline_style, p1, p2);
        //     drawn = true;
        // }

        // canvas.save();
        // canvas.clip_rect(to_skia_rect(&region), None, Some(false));

        let color: Srgba = if SETTINGS.get::<RendererSettings>().debug_renderer {
            Hsv::new(rand::random::<f32>() * 360.0, 1.0, 1.0).into_color()
        } else {
            style.foreground(&self.default_style.colors)
        };

        // There's a lot of overhead for empty blobs in Skia, for some reason they never hit the
        // cache, so trim all the spaces
        let trimmed = text.trim_start();
        let leading_space_bytes = text.len() - trimmed.len();
        let leading_spaces = text[..leading_space_bytes].chars().count();
        let trimmed = trimmed.trim_end();
        // TODO:
        // let y_adjustment = self.shaper.y_adjustment;
        let y_adjustment = self.em_size;
        let adjustment = PixelVec::new(
            leading_spaces as f32 * self.grid_scale.0.width,
            y_adjustment,
        );

        if !trimmed.is_empty() {
            tracy_zone!("draw_text_blob");
            let pos: mint::Vector2<_> = (pos + adjustment).to_vector().into();
            // TODO: The text is not in linear srgba for some reason
            //let color: LinSrgba = color.into_color();
            let text = Text::new(
                trimmed.to_string(),
                pos.into(),
                self.em_size,
                Vec4::from_array(color.into()),
            );
            layer.add_text(text);
            //canvas.draw_text_blob(blob, to_skia_point(pos + adjustment), &paint);
            drawn = true;
        }

        // if style.strikethrough {
        //     let line_position = region.center().y;
        //     paint.set_color(style.special(&self.default_style.colors).to_color());
        //     canvas.draw_line(
        //         (pos.x, line_position),
        //         (pos.x + width, line_position),
        //         &paint,
        //     );
        //     drawn = true;
        // }

        drawn
    }

    fn draw_underline(
        &self,
        style: &Arc<Style>,
        underline_style: UnderlineStyle,
        p1: PixelPos<f32>,
        p2: PixelPos<f32>,
    ) {
        // tracy_zone!("draw_underline");
        // canvas.save();
        //
        // let mut underline_paint = Paint::default();
        // underline_paint.set_anti_alias(false);
        // underline_paint.set_blend_mode(BlendMode::SrcOver);
        // let underline_stroke_scale = SETTINGS.get::<RendererSettings>().underline_stroke_scale;
        // // If the stroke width is less than one, clamp it to one otherwise we get nasty aliasing
        // // issues
        // let stroke_width = (self.shaper.current_size() * underline_stroke_scale / 10.).max(1.);
        //
        // underline_paint
        //     .set_color(style.special(&self.default_style.colors).to_color())
        //     .set_stroke_width(stroke_width);
        //
        // match underline_style {
        //     UnderlineStyle::Underline => {
        //         underline_paint.set_path_effect(None);
        //         canvas.draw_line(to_skia_point(p1), to_skia_point(p2), &underline_paint);
        //     }
        //     UnderlineStyle::UnderDouble => {
        //         underline_paint.set_path_effect(None);
        //         canvas.draw_line(to_skia_point(p1), to_skia_point(p2), &underline_paint);
        //         let p1 = (p1.x, p1.y - 2.);
        //         let p2 = (p2.x, p2.y - 2.);
        //         canvas.draw_line(p1, p2, &underline_paint);
        //     }
        //     UnderlineStyle::UnderCurl => {
        //         let p1 = (p1.x, p1.y - 3. + stroke_width);
        //         let p2 = (p2.x, p2.y - 3. + stroke_width);
        //         underline_paint
        //             .set_path_effect(None)
        //             .set_anti_alias(true)
        //             .set_style(skia_safe::paint::Style::Stroke);
        //         let mut path = Path::default();
        //         path.move_to(p1);
        //         let mut i = p1.0;
        //         let mut sin = -2. * stroke_width;
        //         let increment = self.grid_scale.0.width / 2.;
        //         while i < p2.0 {
        //             sin *= -1.;
        //             i += increment;
        //             path.quad_to((i - (increment / 2.), p1.1 + sin), (i, p1.1));
        //         }
        //         canvas.draw_path(&path, &underline_paint);
        //     }
        //     UnderlineStyle::UnderDash => {
        //         underline_paint.set_path_effect(dash_path_effect::new(
        //             &[6.0 * stroke_width, 2.0 * stroke_width],
        //             0.0,
        //         ));
        //         canvas.draw_line(to_skia_point(p1), to_skia_point(p2), &underline_paint);
        //     }
        //     UnderlineStyle::UnderDot => {
        //         underline_paint.set_path_effect(dash_path_effect::new(
        //             &[1.0 * stroke_width, 1.0 * stroke_width],
        //             0.0,
        //         ));
        //         canvas.draw_line(to_skia_point(p1), to_skia_point(p2), &underline_paint);
        //     }
        // }
        //
        // canvas.restore();
    }
}
