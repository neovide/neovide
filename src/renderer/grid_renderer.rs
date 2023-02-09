use std::sync::Arc;

use glutin::dpi::PhysicalSize;
use log::trace;
use skia_safe::{
    colors, dash_path_effect, BlendMode, Canvas, Color, Paint, Path, Point, Rect, HSV,
};

use crate::{
    dimensions::Dimensions,
    editor::{Colors, Style, UnderlineStyle},
    renderer::{CachingShaper, RendererSettings},
    settings::*,
    window::WindowSettings,
};

pub struct GridRenderer {
    pub shaper: CachingShaper,
    pub paint: Paint,
    pub default_style: Arc<Style>,
    pub em_size: f32,
    pub font_dimensions: Dimensions,
    pub scale_factor: f64,
    pub is_ready: bool,
}

impl GridRenderer {
    pub fn new(scale_factor: f64) -> Self {
        let mut shaper = CachingShaper::new(scale_factor as f32);
        let mut paint = Paint::new(colors::WHITE, None);
        paint.set_anti_alias(false);
        let default_style = Arc::new(Style::new(Colors::new(
            Some(colors::WHITE),
            Some(colors::BLACK),
            Some(colors::GREY),
        )));
        let em_size = shaper.current_size();
        let font_dimensions: Dimensions = shaper.font_base_dimensions().into();

        GridRenderer {
            shaper,
            paint,
            default_style,
            em_size,
            font_dimensions,
            scale_factor,
            is_ready: false,
        }
    }

    pub fn font_names(&self) -> Vec<String> {
        self.shaper.font_names()
    }

    /// Convert PhysicalSize to grid size.
    pub fn convert_physical_to_grid(&self, physical: PhysicalSize<u32>) -> Dimensions {
        Dimensions::from(physical) / self.font_dimensions
    }

    /// Convert grid size to PhysicalSize.
    pub fn convert_grid_to_physical(&self, grid: Dimensions) -> PhysicalSize<u32> {
        (grid * self.font_dimensions).into()
    }

    pub fn handle_scale_factor_update(&mut self, scale_factor: f64) {
        self.shaper.update_scale_factor(scale_factor as f32);
        self.update_font_dimensions();
    }

    pub fn update_font(&mut self, guifont_setting: &str) {
        self.shaper.update_font(guifont_setting);
        self.update_font_dimensions();
    }

    pub fn update_linespace(&mut self, linespace_setting: i64) {
        self.shaper.update_linespace(linespace_setting);
        self.update_font_dimensions();
    }

    fn update_font_dimensions(&mut self) {
        self.em_size = self.shaper.current_size();
        self.font_dimensions = self.shaper.font_base_dimensions().into();
        self.is_ready = true;
        trace!("Updated font dimensions: {:?}", self.font_dimensions,);
    }

    fn compute_text_region(&self, grid_position: (u64, u64), cell_width: u64) -> Rect {
        let (x, y) = grid_position * self.font_dimensions;
        let width = cell_width * self.font_dimensions.width;
        let height = self.font_dimensions.height;
        Rect::new(x as f32, y as f32, (x + width) as f32, (y + height) as f32)
    }

    pub fn get_default_background(&self) -> Color {
        self.default_style.colors.background.unwrap().to_color()
    }

    pub fn draw_background(
        &mut self,
        canvas: &mut Canvas,
        grid_position: (u64, u64),
        cell_width: u64,
        style: &Option<Arc<Style>>,
        is_floating: bool,
    ) {
        self.paint.set_blend_mode(BlendMode::Src);

        let region = self.compute_text_region(grid_position, cell_width);
        let style = style.as_ref().unwrap_or(&self.default_style);

        if SETTINGS.get::<RendererSettings>().debug_renderer {
            let random_hsv: HSV = (rand::random::<f32>() * 360.0, 0.3, 0.3).into();
            let random_color = random_hsv.to_color(255);
            self.paint.set_color(random_color);
        } else {
            self.paint
                .set_color(style.background(&self.default_style.colors).to_color());
        }

        if is_floating {
            self.paint
                .set_alpha((255.0 * ((100 - style.blend) as f32 / 100.0)) as u8);
        } else if (SETTINGS.get::<WindowSettings>().transparency - 1.0).abs() > f32::EPSILON
            // Only make background color transparent
            && self.paint.color() == self.get_default_background()
        {
            self.paint.set_alpha(0);
        }
        canvas.draw_rect(region, &self.paint);
    }

    pub fn draw_foreground(
        &mut self,
        canvas: &mut Canvas,
        text: String,
        grid_position: (u64, u64),
        cell_width: u64,
        style: &Option<Arc<Style>>,
    ) {
        let (x, y) = grid_position * self.font_dimensions;
        let width = cell_width * self.font_dimensions.width;

        let style = style.as_ref().unwrap_or(&self.default_style);

        // We don't want to clip text in the x position, only the y so we add a buffer of 1
        // character on either side of the region so that we clip vertically but not horizontally.
        let (grid_x, grid_y) = grid_position;
        let clip_position = (grid_x.saturating_sub(1), grid_y);
        let region = self.compute_text_region(clip_position, cell_width + 2);

        if let Some(underline_style) = style.underline {
            let line_position = self.shaper.underline_position();
            let p1 = (
                x as f32,
                (y - line_position + self.font_dimensions.height) as f32,
            );
            let p2 = (
                (x + width) as f32,
                (y - line_position + self.font_dimensions.height) as f32,
            );

            self.draw_underline(canvas, style, underline_style, p1.into(), p2.into())
        }

        canvas.save();
        canvas.clip_rect(region, None, Some(false));

        let y_adjustment = self.shaper.y_adjustment();

        if SETTINGS.get::<RendererSettings>().debug_renderer {
            let random_hsv: HSV = (rand::random::<f32>() * 360.0, 1.0, 1.0).into();
            let random_color = random_hsv.to_color(255);
            self.paint.set_color(random_color);
        } else {
            self.paint
                .set_color(style.foreground(&self.default_style.colors).to_color());
        }
        self.paint.set_anti_alias(false);

        for blob in self
            .shaper
            .shape_cached(text, style.bold, style.italic)
            .iter()
        {
            canvas.draw_text_blob(blob, (x as f32, (y + y_adjustment) as f32), &self.paint);
        }

        if style.strikethrough {
            let line_position = region.center_y();
            self.paint
                .set_color(style.special(&self.default_style.colors).to_color());
            canvas.draw_line(
                (x as f32, line_position),
                ((x + width) as f32, line_position),
                &self.paint,
            );
        }

        canvas.restore();
    }

    fn draw_underline(
        &self,
        canvas: &mut Canvas,
        style: &Arc<Style>,
        underline_style: UnderlineStyle,
        p1: Point,
        p2: Point,
    ) {
        canvas.save();

        let mut underline_paint = self.paint.clone();
        let auto_scaling = SETTINGS
            .get::<RendererSettings>()
            .underline_automatic_scaling;
        // Arbitrary value under which we simply round the line thickness to 1. Anything else
        // results in ugly aliasing artifacts.
        let stroke_width = if self.shaper.current_size() < 15. || !auto_scaling {
            underline_paint.set_anti_alias(false);
            1.0
        } else {
            underline_paint.set_anti_alias(true);
            self.shaper.current_size() / 10.
        };

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
                let p1 = (p1.x, p1.y - 2.);
                let p2 = (p2.x, p2.y - 2.);
                canvas.draw_line(p1, p2, &underline_paint);
            }
            UnderlineStyle::UnderCurl => {
                let p1 = (p1.x, p1.y - 3. + stroke_width);
                let p2 = (p2.x, p2.y - 3. + stroke_width);
                underline_paint
                    .set_path_effect(None)
                    .set_anti_alias(true)
                    .set_style(skia_safe::paint::Style::Stroke);
                let mut path = Path::default();
                path.move_to(p1);
                let mut i = p1.0;
                let mut sin = -2. * stroke_width;
                let increment = self.font_dimensions.width as f32 / 2.;
                while i < p2.0 {
                    sin *= -1.;
                    i += increment;
                    path.quad_to((i - (increment / 2.), p1.1 + sin), (i, p1.1));
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
