use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use skulpin::CoordinateSystemHelper;
use skulpin::skia_safe::{Canvas, Paint, Surface, Budgeted, Rect, Typeface, Font, FontStyle, colors};
use skulpin::skia_safe::gpu::SurfaceOrigin;

mod caching_shaper;

use caching_shaper::CachingShaper;

use crate::editor::{Editor, CursorShape, Style, Colors};

const FONT_NAME: &str = "Delugia Nerd Font";
const FONT_SIZE: f32 = 14.0;

struct Fonts {
    pub name: String,
    pub size: f32,
    pub normal: Font,
    pub bold: Font,
    pub italic: Font,
    pub bold_italic: Font
}

impl Fonts {
    fn new(name: &str, size: f32) -> Fonts {
        Fonts {
            name: name.to_string(),
            size,
            normal: Font::from_typeface(
                Typeface::new(name, FontStyle::normal()).expect("Could not load normal font file"),
                size),
            bold: Font::from_typeface(
                Typeface::new(name, FontStyle::bold()).expect("Could not load bold font file"),
                size),
            italic: Font::from_typeface(
                Typeface::new(name, FontStyle::italic()).expect("Could not load italic font file"),
                size),
            bold_italic: Font::from_typeface(
                Typeface::new(name, FontStyle::bold_italic()).expect("Could not load bold italic font file"),
                size)
        }
    }

    fn get(&self, style: &Style) -> &Font {
        match (style.bold, style.italic) {
            (false, false) => &self.normal,
            (true, false) => &self.bold,
            (false, true)  => &self.italic,
            (true, true) => &self.bold_italic
        }
    }
}

struct FontLookup {
    pub name: String,
    pub base_size: f32,
    pub loaded_fonts: HashMap<u16, Fonts>
}

impl FontLookup {
    pub fn new(name: &str, base_size: f32) -> FontLookup {
        let lookup = FontLookup {
            name: name.to_string(),
            base_size,
            loaded_fonts: HashMap::new()
        };

        lookup.size(1);
        lookup.size(2);
        lookup.size(3);

        lookup
    }

    fn size(&mut self, size_multiplier: u16) -> &Fonts {
        match self.loaded_fonts.get(&size_multiplier) {
            Some(fonts) => fonts,
            None => {
                let fonts = Fonts::new(
                    &self.name, 
                    self.base_size * size_multiplier as f32);
                self.loaded_fonts.insert(size_multiplier, fonts); 
                self.loaded_fonts.get(&size_multiplier).unwrap()
            }
        }
    }
}

pub struct Renderer {
    editor: Arc<Mutex<Editor>>,

    surface: Option<Surface>,
    paint: Paint,
    fonts_lookup: FontLookup,
    shaper: CachingShaper,

    pub font_width: f32,
    pub font_height: f32,
    cursor_pos: (f32, f32),
}

impl Renderer {
    pub fn new(editor: Arc<Mutex<Editor>>) -> Renderer {
        let surface = None;
        let mut paint = Paint::new(colors::WHITE, None);
        paint.set_anti_alias(false);
        let fonts_lookup = FontLookup::new(FONT_NAME, FONT_SIZE);
        let shaper = CachingShaper::new();

        let base_fonts = fonts_lookup.size(1);
        let (_, bounds) = base_fonts.normal.measure_str("_", Some(&paint));
        let font_width = bounds.width();
        let (_, metrics) = base_fonts.normal.metrics();
        let font_height = metrics.descent - metrics.ascent;
        let cursor_pos = (0.0, 0.0);

        Renderer { editor, surface, paint, fonts_lookup, shaper, font_width, font_height, cursor_pos }
    }

    fn draw_background(&mut self, canvas: &mut Canvas, text: &str, grid_pos: (u64, u64), size: u16, style: &Option<Style>, default_colors: &Colors) {
        let (grid_x, grid_y) = grid_pos;
        let x = grid_x as f32 * self.font_width;
        let y = grid_y as f32 * self.font_height;
        let width = text.chars().count() as f32 * self.font_width;
        let height = self.font_height;
        let region = Rect::new(x, y, x + width, y + height);

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

        if style.underline || style.undercurl {
            let (_, metrics) = self.fonts_lookup.size(scale).get(&style).metrics();
            let line_position = metrics.underline_position().unwrap();

            self.paint.set_color(style.special(&default_colors).to_color());
            canvas.draw_line((x, y - line_position + self.font_height), (x + width, y - line_position + self.font_height), &self.paint);
        }

        self.paint.set_color(style.foreground(&default_colors).to_color());
        let text = text.trim_end();
        if text.len() > 0 {
            let blob = self.shaper.shape_cached(text.to_string(), self.fonts.get(&style));
            canvas.draw_text_blob(blob, (x, y), &self.paint);
        }
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
            self.draw_background(&mut canvas, &command.text, command.grid_position.clone(), &command.style, &default_colors);
        }
        for command in draw_commands.iter() {
            self.draw_foreground(&mut canvas, &command.text, command.grid_position.clone(), &command.style, &default_colors);
        }

        let image = surface.image_snapshot();
        let window_size = coordinate_system_helper.window_logical_size();
        let image_destination = Rect::new(0.0, 0.0, window_size.width as f32, window_size.height as f32);
        gpu_canvas.draw_image_rect(image, None, &image_destination, &self.paint);

        self.surface = Some(surface);

        let (cursor_grid_x, cursor_grid_y) = cursor.position;
        let target_cursor_x = cursor_grid_x as f32 * self.font_width;
        let target_cursor_y = cursor_grid_y as f32 * self.font_height;
        let (previous_cursor_x, previous_cursor_y) = self.cursor_pos;

        let delta_cursor_x = target_cursor_x - previous_cursor_x;
        let delta_cursor_y = target_cursor_y - previous_cursor_y;

        let cursor_x = delta_cursor_x * 0.5 + previous_cursor_x;
        let cursor_y = delta_cursor_y * 0.5 + previous_cursor_y;
        self.cursor_pos = (cursor_x, cursor_y);
        if cursor.enabled {
            let cursor_width = match cursor.shape {
                CursorShape::Vertical => self.font_width / 8.0,
                CursorShape::Horizontal | CursorShape::Block => self.font_width
            };
            let cursor_height = match cursor.shape {
                CursorShape::Horizontal => self.font_width / 8.0,
                CursorShape::Vertical | CursorShape::Block => self.font_height
            };
            let cursor_region = Rect::new(cursor_x, cursor_y, cursor_x + cursor_width, cursor_y + cursor_height);
            self.paint.set_color(cursor.background(&default_colors).to_color());
            gpu_canvas.draw_rect(cursor_region, &self.paint);

            if let CursorShape::Block = cursor.shape {
                self.paint.set_color(cursor.foreground(&default_colors).to_color());
                let editor = self.editor.lock().unwrap();
                let character = editor.grid[cursor_grid_y as usize][cursor_grid_x as usize].clone()
                    .map(|(character, _)| character)
                    .unwrap_or(' ');
                gpu_canvas.draw_text_blob(
                    self.shaper.shape_cached(character.to_string(), &self.fonts.normal), 
                    (cursor_x, cursor_y), &self.paint);
            }
        }

        draw_commands.len() > 0 || delta_cursor_x.abs() > 0.001 || delta_cursor_y.abs() > 0.001
    }
}
