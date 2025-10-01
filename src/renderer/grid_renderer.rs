use std::{
    sync::Arc,
    ops::Range,
};

use log::trace;
use skia_safe::{colors, dash_path_effect, BlendMode, Canvas, Color, Paint, Path, HSV};

use crate::{
    editor::{Colors, Style, UnderlineStyle},
    profiling::tracy_zone,
    renderer::{
        box_drawing::{self},
        CachingShaper, RendererSettings,
    },
    settings::*,
    units::{
        to_skia_point, to_skia_rect, GridPos, GridScale, GridSize, PixelPos, PixelRect, PixelVec,
    },
    window::WindowSettings,
};

use super::{box_drawing::BoxDrawingSettings, fonts::font_options::FontOptions};

pub struct GridRenderer {
    pub shaper: CachingShaper,
    pub default_style: Arc<Style>,
    pub em_size: f32,
    pub grid_scale: GridScale,
    pub box_char_renderer: box_drawing::Renderer,
    pub is_ready: bool,

    settings: Arc<Settings>,
}

/// Struct with named fields to be returned from draw_background
pub struct BackgroundInfo {
    pub custom_color: bool,
    // This should probably be used
    #[allow(unused)]
    pub transparent: bool,
}

impl GridRenderer {
    pub fn new(scale_factor: f64, settings: Arc<Settings>) -> Self {
        let mut shaper = CachingShaper::new(scale_factor as f32);
        let default_style = Arc::new(Style::new(Colors::new(
            Some(colors::WHITE),
            Some(colors::BLACK),
            Some(colors::GREY),
        )));
        let em_size = shaper.current_size();
        let font_dimensions = shaper.font_base_dimensions();
        let grid_scale = GridScale::new(font_dimensions);
        let cell_size = GridSize::new(1, 1) * grid_scale;

        GridRenderer {
            shaper,
            default_style,
            em_size,
            grid_scale,
            box_char_renderer: box_drawing::Renderer::new(
                cell_size,
                em_size,
                BoxDrawingSettings::default(),
            ),
            is_ready: false,

            settings,
        }
    }

    pub fn font_names(&self) -> Vec<String> {
        self.shaper.font_names()
    }

    pub fn handle_scale_factor_update(&mut self, scale_factor: f64) {
        self.shaper.update_scale_factor(scale_factor as f32);
        self.update_font_dimensions();
    }

    pub fn update_font(&mut self, guifont_setting: &str) {
        self.shaper.update_font(guifont_setting);
        self.update_font_dimensions();
    }

    pub fn update_font_options(&mut self, options: FontOptions) {
        self.shaper.update_font_options(options);
        self.update_font_dimensions();
    }

    pub fn update_linespace(&mut self, linespace_setting: f32) {
        self.shaper.update_linespace(linespace_setting);
        self.update_font_dimensions();
    }

    pub fn handle_box_drawing_update(&mut self, settings: BoxDrawingSettings) {
        self.box_char_renderer.update_settings(settings);
    }

    fn update_font_dimensions(&mut self) {
        self.em_size = self.shaper.current_size();
        self.grid_scale = GridScale::new(self.shaper.font_base_dimensions());
        let new_cell_size = GridSize::new(1, 1) * self.grid_scale;
        self.box_char_renderer
            .update_dimensions(new_cell_size, self.em_size);
        self.is_ready = true;
        trace!("Updated font dimensions: {:?}", self.grid_scale);
    }

    fn compute_text_region(&self, cells: &Range<u32>) -> PixelRect<f32> {
        let grid_position = GridPos::new(cells.start, 0);
        let pos = grid_position * self.grid_scale;
        let size = GridSize::new(cells.len() as i32, 1) * self.grid_scale;
        PixelRect::from_origin_and_size(pos, size)
    }

    pub fn get_default_background_color(&self) -> Color {
        self.default_style.colors.background.unwrap().to_color()
    }

    pub fn get_default_background(&self, opacity: f32) -> Color {
        log::info!("blend {}", self.default_style.blend);
        let alpha = opacity * (100 - self.default_style.blend) as f32 / 100.0;
        self.get_default_background_color()
            .with_a((alpha * 255.0) as u8)
    }

    /// Draws a single background cell with the same style
    pub fn draw_background(
        &mut self,
        canvas: &Canvas,
        cells: &Range<u32>,
        style: &Option<Arc<Style>>,
        opacity: f32,
    ) -> BackgroundInfo {
        tracy_zone!("draw_background");
        let debug = self.settings.get::<RendererSettings>().debug_renderer;
        if style.is_none() && !debug {
            return BackgroundInfo {
                custom_color: false,
                transparent: self.default_style.blend > 0 || opacity < 1.0,
            };
        }
        let region = self.compute_text_region(cells);
        let style = style.as_ref().unwrap_or(&self.default_style);
        let style_background = style.background(&self.default_style.colors).to_color();

        let mut paint = Paint::default();
        paint.set_anti_alias(false);
        paint.set_blend_mode(BlendMode::Src);

        if debug {
            let random_hsv: HSV = (rand::random::<f32>() * 360.0, 0.3, 0.3).into();
            let random_color = random_hsv.to_color(255);
            paint.set_color(random_color);
        } else {
            paint.set_color(style_background);
        }

        let is_default_background = style_background == self.get_default_background_color();
        let normal_opacity = self.settings.get::<WindowSettings>().normal_opacity;

        let alpha = if normal_opacity < 1.0 && is_default_background {
            normal_opacity
        } else if style.blend > 0 {
            ((100 - style.blend) as f32 / 100.0) * opacity
        } else {
            opacity
        };
        paint.set_alpha_f(alpha);

        let custom_color = paint.color4f() != self.default_style.colors.background.unwrap();
        if custom_color {
            canvas.draw_rect(to_skia_rect(&region), &paint);
        }

        BackgroundInfo {
            custom_color,
            transparent: alpha < 1.0,
        }
    }

    /// Draws some foreground text.
    /// Returns true if any text was actually drawn.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_foreground(
        &mut self,
        text_canvas: &Canvas,
        boxchar_canvas: &Canvas,
        text: &str,
        cells: &Range<u32>,
        style: &Option<Arc<Style>>,
        window_position: PixelPos<f32>,
    ) -> (bool, bool) {
        tracy_zone!("draw_foreground");
        let region = self.compute_text_region(cells);

        let style = style.as_ref().unwrap_or(&self.default_style);
        let mut text_drawn = false;

        if let Some(underline_style) = style.underline {
            let stroke_size = self.shaper.stroke_size();
            // Measure the underline offset from the baseline position snapped to a whole pixel
            let baseline_position = self.shaper.baseline_offset().round();
            // The underline should be at least 1 pixel below the baseline
            let underline_position =
                baseline_position - self.shaper.underline_offset().min(-1.).round();
            let p1 = PixelPos::new(region.min.x, underline_position);
            let p2 = PixelPos::new(region.max.x, underline_position);

            self.draw_underline(text_canvas, style, underline_style, stroke_size, p1, p2);
            text_drawn = true;
        }

        if self.box_char_renderer.draw_glyph(
            text,
            boxchar_canvas,
            region,
            style.foreground(&self.default_style.colors).to_color(),
            window_position,
        ) {
            return (text_drawn, true);
        } else if !text.is_empty() {
            let mut paint = Paint::default();
            paint.set_anti_alias(false);
            paint.set_blend_mode(BlendMode::SrcOver);
            text_canvas.save();

            // We don't want to clip text in the x position, only the y so we add a buffer of 1
            // character on either side of the region so that we clip vertically but not horizontally.
            let wider_cells = cells.start.saturating_sub(1)..cells.end + 1;
            let clip_region = self.compute_text_region(&wider_cells);

            if self.settings.get::<RendererSettings>().debug_renderer {
                let random_hsv: HSV = (rand::random::<f32>() * 360.0, 1.0, 1.0).into();
                let random_color = random_hsv.to_color(255);
                paint.set_color(random_color);
            } else {
                paint.set_color(style.foreground(&self.default_style.colors).to_color());
            }
            paint.set_anti_alias(false);
            if self.settings.get::<RendererSettings>().debug_renderer {
                let random_hsv: HSV = (rand::random::<f32>() * 360.0, 1.0, 1.0).into();
                let random_color = random_hsv.to_color(255);
                paint.set_color(random_color);
            } else {
                paint.set_color(style.foreground(&self.default_style.colors).to_color());
            }
            paint.set_anti_alias(false);
            text_canvas.clip_rect(to_skia_rect(&clip_region), None, Some(false));

            let mut paint = Paint::default();
            paint.set_anti_alias(false);
            paint.set_blend_mode(BlendMode::SrcOver);

            if self.settings.get::<RendererSettings>().debug_renderer {
                let random_hsv: HSV = (rand::random::<f32>() * 360.0, 1.0, 1.0).into();
                let random_color = random_hsv.to_color(255);
                paint.set_color(random_color);
            } else {
                paint.set_color(style.foreground(&self.default_style.colors).to_color());
            }
            paint.set_anti_alias(false);
            // There's a lot of overhead for empty blobs in Skia, for some reason they never hit the
            // cache, so trim all the spaces
            let trimmed = text.trim_start();
            let leading_space_bytes = text.len() - trimmed.len();
            let leading_spaces = text[..leading_space_bytes].chars().count();
            let trimmed = trimmed.trim_end();
            let adjustment = PixelVec::new(
                leading_spaces as f32 * self.grid_scale.width(),
                self.shaper.baseline_offset(),
            );

            for blob in self.shaper.shape_cached(trimmed, style.into()).iter() {
                tracy_zone!("draw_text_blob");
                text_canvas.draw_text_blob(blob, to_skia_point(region.min + adjustment), &paint);
                text_drawn = true;
            }
            if style.strikethrough {
                let line_position = region.center().y;
                paint.set_color(style.special(&self.default_style.colors).to_color());
                text_canvas.draw_line(
                    (region.min.x, line_position),
                    (region.max.x, line_position),
                    &paint,
                );
                text_drawn = true;
            }
            text_canvas.restore();
        }
        (text_drawn, false)
    }

    fn draw_underline(
        &self,
        canvas: &Canvas,
        style: &Arc<Style>,
        underline_style: UnderlineStyle,
        stroke_size: f32,
        p1: PixelPos<f32>,
        p2: PixelPos<f32>,
    ) {
        tracy_zone!("draw_underline");
        canvas.save();

        let mut underline_paint = Paint::default();
        underline_paint.set_anti_alias(false);
        underline_paint.set_blend_mode(BlendMode::SrcOver);
        let underline_stroke_scale = self
            .settings
            .get::<RendererSettings>()
            .underline_stroke_scale;
        // at least 1 and in whole pixels
        let stroke_width = (stroke_size * underline_stroke_scale).max(1.).round();

        // offset y by width / 2 to align the *top* of the underline with p1 and p2
        let offset = stroke_width / 2.;
        let p1 = (p1.x, p1.y + offset);
        let p2 = (p2.x, p2.y + offset);

        underline_paint
            .set_color(style.special(&self.default_style.colors).to_color())
            .set_stroke_width(stroke_width);

        match underline_style {
            UnderlineStyle::Underline => {
                underline_paint.set_path_effect(None);
                canvas.draw_line(p1, p2, &underline_paint);
            }
            UnderlineStyle::UnderDouble => {
                underline_paint.set_path_effect(None);
                canvas.draw_line(p1, p2, &underline_paint);
                let p1 = (p1.0, p1.1 + 2. * stroke_width);
                let p2 = (p2.0, p2.1 + 2. * stroke_width);
                canvas.draw_line(p1, p2, &underline_paint);
            }
            UnderlineStyle::UnderCurl => {
                let p1 = (p1.0, p1.1 + stroke_width);
                let p2 = (p2.0, p2.1 + stroke_width);
                underline_paint
                    .set_path_effect(None)
                    .set_anti_alias(true)
                    .set_style(skia_safe::paint::Style::Stroke);
                let mut path = Path::default();
                path.move_to(p1);
                let mut sin = -2. * stroke_width;
                let dx = self.grid_scale.width() / 2.;
                let count = ((p2.0 - p1.0) / dx).round();
                let dy = (p2.1 - p1.1) / count;
                for _ in 0..(count as i32) {
                    sin *= -1.;
                    path.r_quad_to((dx / 2., sin), (dx, dy));
                }
                canvas.draw_path(&path, &underline_paint);
            }
            UnderlineStyle::UnderDash => {
                underline_paint.set_path_effect(dash_path_effect::new(
                    &[6.0 * stroke_width, 2.0 * stroke_width],
                    0.0,
                ));
                canvas.draw_line(p1, p2, &underline_paint);
            }
            UnderlineStyle::UnderDot => {
                underline_paint.set_path_effect(dash_path_effect::new(
                    &[1.0 * stroke_width, 1.0 * stroke_width],
                    0.0,
                ));
                canvas.draw_line(p1, p2, &underline_paint);
            }
        }

        canvas.restore();
    }
}
