use std::sync::Arc;

use log::trace;
use skulpin::skia_safe::gpu::SurfaceOrigin;
use skulpin::skia_safe::{colors, dash_path_effect, Budgeted, Canvas, Paint, Rect, Surface};
use skulpin::CoordinateSystemHelper;

mod caching_shaper;
pub mod cursor_renderer;
pub mod font_options;

pub use caching_shaper::CachingShaper;
pub use font_options::*;

use crate::editor::{Style, EDITOR};
use cursor_renderer::CursorRenderer;

pub struct Renderer {
    surface: Option<Surface>,
    paint: Paint,
    shaper: CachingShaper,

    pub font_width: f32,
    pub font_height: f32,
    cursor_renderer: CursorRenderer,
}

impl Renderer {
    pub fn new() -> Renderer {
        let surface = None;
        let mut paint = Paint::new(colors::WHITE, None);
        paint.set_anti_alias(false);

        let mut shaper = CachingShaper::new();

        let (font_width, font_height) = shaper.font_base_dimensions();
        let cursor_renderer = CursorRenderer::new();

        Renderer {
            surface,
            paint,
            shaper,
            font_width,
            font_height,
            cursor_renderer,
        }
    }

    fn update_font(&mut self, guifont_setting: &str) -> bool {
        let updated = self.shaper.update_font(guifont_setting);
        if updated {
            let (font_width, font_height) = self.shaper.font_base_dimensions();
            self.font_width = font_width;
            self.font_height = font_height.ceil();
        }
        updated
    }

    fn compute_text_region(&self, grid_pos: (u64, u64), cell_width: u64) -> Rect {
        let (grid_x, grid_y) = grid_pos;
        let x = grid_x as f32 * self.font_width;
        let y = grid_y as f32 * self.font_height;
        let width = cell_width as f32 * self.font_width as f32;
        let height = self.font_height as f32;
        Rect::new(x, y, x + width, y + height)
    }

    fn draw_background(
        &mut self,
        canvas: &mut Canvas,
        grid_pos: (u64, u64),
        cell_width: u64,
        style: &Option<Arc<Style>>,
        default_style: &Arc<Style>,
    ) {
        let region = self.compute_text_region(grid_pos, cell_width);
        let style = style.as_ref().unwrap_or(default_style);

        self.paint
            .set_color(style.background(&default_style.colors).to_color());
        canvas.draw_rect(region, &self.paint);
    }

    fn draw_foreground(
        &mut self,
        canvas: &mut Canvas,
        text: &str,
        grid_pos: (u64, u64),
        cell_width: u64,
        style: &Option<Arc<Style>>,
        default_style: &Arc<Style>,
    ) {
        let (grid_x, grid_y) = grid_pos;
        let x = grid_x as f32 * self.font_width;
        let y = grid_y as f32 * self.font_height;
        let width = cell_width as f32 * self.font_width;

        let style = style.as_ref().unwrap_or(default_style);

        canvas.save();

        let region = self.compute_text_region(grid_pos, cell_width);

        canvas.clip_rect(region, None, Some(false));

        if style.underline || style.undercurl {
            let line_position = self.shaper.underline_position();
            let stroke_width = self.shaper.options.size / 10.0;
            self.paint
                .set_color(style.special(&default_style.colors).to_color());
            self.paint.set_stroke_width(stroke_width);

            if style.undercurl {
                self.paint.set_path_effect(dash_path_effect::new(
                    &[stroke_width * 2.0, stroke_width * 2.0],
                    0.0,
                ));
            } else {
                self.paint.set_path_effect(None);
            }

            canvas.draw_line(
                (x, y - line_position + self.font_height),
                (x + width, y - line_position + self.font_height),
                &self.paint,
            );
        }

        self.paint
            .set_color(style.foreground(&default_style.colors).to_color());
        let text = text.trim_end();
        if !text.is_empty() {
            for blob in self
                .shaper
                .shape_cached(text, style.bold, style.italic)
                .iter()
            {
                canvas.draw_text_blob(blob, (x, y), &self.paint);
            }
        }

        if style.strikethrough {
            let line_position = region.center_y();
            self.paint
                .set_color(style.special(&default_style.colors).to_color());
            canvas.draw_line((x, line_position), (x + width, line_position), &self.paint);
        }

        canvas.restore();
    }

    pub fn draw(
        &mut self,
        gpu_canvas: &mut Canvas,
        coordinate_system_helper: &CoordinateSystemHelper,
        dt: f32,
    ) -> bool {
        trace!("Rendering");

        let ((draw_commands, should_clear), default_style, cursor, guifont_setting) = {
            let mut editor = EDITOR.lock();
            (
                editor.build_draw_commands(),
                editor.default_style.clone(),
                editor.cursor.clone(),
                editor.guifont.clone(),
            )
        };

        let font_changed = guifont_setting
            .map(|guifont| self.update_font(&guifont))
            .unwrap_or(false);

        if should_clear {
            self.surface = None;
        }

        let mut surface = self.surface.take().unwrap_or_else(|| {
            let mut context = gpu_canvas.gpu_context().unwrap();
            let budgeted = Budgeted::YES;
            let image_info = gpu_canvas.image_info();
            let surface_origin = SurfaceOrigin::TopLeft;
            let mut surface = Surface::new_render_target(
                &mut context,
                budgeted,
                &image_info,
                None,
                surface_origin,
                None,
                None,
            )
            .expect("Could not create surface");
            let canvas = surface.canvas();
            canvas.clear(default_style.colors.background.clone().unwrap().to_color());
            surface
        });

        let mut canvas = surface.canvas();
        coordinate_system_helper.use_logical_coordinates(&mut canvas);

        for command in draw_commands.iter() {
            self.draw_background(
                &mut canvas,
                command.grid_position,
                command.cell_width,
                &command.style,
                &default_style,
            );
        }

        for command in draw_commands.iter() {
            self.draw_foreground(
                &mut canvas,
                &command.text,
                command.grid_position,
                command.cell_width,
                &command.style,
                &default_style,
            );
        }

        let image = surface.image_snapshot();
        let window_size = coordinate_system_helper.window_logical_size();
        let image_destination = Rect::new(
            0.0,
            0.0,
            window_size.width as f32,
            window_size.height as f32,
        );

        gpu_canvas.draw_image_rect(image, None, &image_destination, &self.paint);

        self.surface = Some(surface);
        self.cursor_renderer.draw(
            cursor,
            &default_style.colors,
            (self.font_width, self.font_height),
            &mut self.shaper,
            gpu_canvas,
            dt,
        );

        font_changed
    }
}
