use std::sync::{Arc, Mutex};
use skulpin::CoordinateSystemHelper;
use skulpin::skia_safe::{Canvas, Paint, Surface, Budgeted, Rect, colors};
use skulpin::skia_safe::gpu::SurfaceOrigin;

mod caching_shaper;
mod cursor_renderer;
mod fonts;

pub use caching_shaper::CachingShaper;

use cursor_renderer::CursorRenderer;
use fonts::FontLookup;
use crate::editor::{Editor, Style, Colors};

const FONT_NAME: &str = "Delugia Nerd Font";
const FONT_SIZE: f32 = 14.0;

pub struct Renderer {
    editor: Arc<Mutex<Editor>>,

    surface: Option<Surface>,
    paint: Paint,
    fonts_lookup: FontLookup,
    shaper: CachingShaper,

    pub font_width: f32,
    pub font_height: f32,
    cursor_renderer: CursorRenderer,
}

impl Renderer {
    pub fn new(editor: Arc<Mutex<Editor>>) -> Renderer {
        let surface = None;
        let mut paint = Paint::new(colors::WHITE, None);
        paint.set_anti_alias(false);
        
        let mut fonts_lookup = FontLookup::new(FONT_NAME, FONT_SIZE);
        let shaper = CachingShaper::new();

        let base_fonts = fonts_lookup.size(1);
        let (_, bounds) = base_fonts.normal.measure_str("_", Some(&paint));
        let font_width = bounds.width();
        let (_, metrics) = base_fonts.normal.metrics();
        let font_height = metrics.descent - metrics.ascent;
        let cursor_renderer = CursorRenderer::new();

        Renderer { editor, surface, paint, fonts_lookup, shaper, font_width, font_height, cursor_renderer }
    }

    fn compute_text_region(&self, text: &str, grid_pos: (u64, u64), size: u16) -> Rect {
        let (grid_x, grid_y) = grid_pos;
        let x = grid_x as f32 * self.font_width;
        let y = grid_y as f32 * self.font_height;
        let width = text.chars().count() as f32 * self.font_width * size as f32;
        let height = self.font_height * size as f32;
        Rect::new(x, y, x + width, y + height)
    }

    fn draw_background(&mut self, canvas: &mut Canvas, text: &str, grid_pos: (u64, u64), size: u16, style: &Option<Style>, default_colors: &Colors) {
        let region = self.compute_text_region(text, grid_pos, size);
        let style = style.clone().unwrap_or(Style::new(default_colors.clone()));

        self.paint.set_color(style.background(default_colors).to_color());
        canvas.draw_rect(region, &self.paint);
    }

    fn draw_foreground(&mut self, canvas: &mut Canvas, text: &str, grid_pos: (u64, u64), size: u16, style: &Option<Style>, default_colors: &Colors) {
        let (grid_x, grid_y) = grid_pos;
        let x = grid_x as f32 * self.font_width;
        let y = grid_y as f32 * self.font_height;
        let width = text.chars().count() as f32 * self.font_width;

        let style = style.clone().unwrap_or(Style::new(default_colors.clone()));

        canvas.save();

        let region = self.compute_text_region(text, grid_pos, size);

        canvas.clip_rect(region, None, Some(false));

        if style.underline || style.undercurl {
            let (_, metrics) = self.fonts_lookup.size(size).get(&style).metrics();
            let line_position = metrics.underline_position().unwrap();

            self.paint.set_color(style.special(&default_colors).to_color());
            canvas.draw_line((x, y - line_position + self.font_height), (x + width, y - line_position + self.font_height), &self.paint);
        }

        self.paint.set_color(style.foreground(&default_colors).to_color());
        let text = text.trim_end();
        if text.len() > 0 {
            let blob = self.shaper.shape_cached(text.to_string(), size, self.fonts_lookup.size(size).get(&style));
            canvas.draw_text_blob(blob, (x, y), &self.paint);
        }

        canvas.restore();
    }

    pub fn draw(&mut self, gpu_canvas: &mut Canvas, coordinate_system_helper: &CoordinateSystemHelper) -> bool {
        let ((draw_commands, should_clear), default_colors, cursor) = {
            let mut editor = self.editor.lock().unwrap();
            (
                editor.build_draw_commands(), 
                editor.default_colors.clone(), 
                editor.cursor.clone()
            )
        };

        if should_clear {
            self.surface = None;
        }

        let mut surface = self.surface.take().unwrap_or_else(|| {
            let mut context = gpu_canvas.gpu_context().unwrap();
            let budgeted = Budgeted::YES;
            let image_info = gpu_canvas.image_info();
            let surface_origin = SurfaceOrigin::TopLeft;
            let mut surface = Surface::new_render_target(&mut context, budgeted, &image_info, None, surface_origin, None, None).expect("Could not create surface");
            let canvas = surface.canvas();
            canvas.clear(default_colors.background.clone().unwrap().to_color());
            surface
        });

        let mut canvas = surface.canvas();
        coordinate_system_helper.use_logical_coordinates(&mut canvas);

        for command in draw_commands.iter() {
            self.draw_background(&mut canvas, &command.text, command.grid_position.clone(), command.scale, &command.style, &default_colors);
        }
        for command in draw_commands.iter() {
            self.draw_foreground(&mut canvas, &command.text, command.grid_position.clone(), command.scale, &command.style, &default_colors);
        }

        let image = surface.image_snapshot();
        let window_size = coordinate_system_helper.window_logical_size();
        let image_destination = Rect::new(0.0, 0.0, window_size.width as f32, window_size.height as f32);
        gpu_canvas.draw_image_rect(image, None, &image_destination, &self.paint);

        self.surface = Some(surface);

        let cursor_animating = self.cursor_renderer.draw(
            cursor, &default_colors, 
            self.font_width, self.font_height, 
            &mut self.paint, self.editor.clone(),
            &mut self.shaper, &mut self.fonts_lookup,
            gpu_canvas);

        draw_commands.len() > 0 || cursor_animating
    }
}
