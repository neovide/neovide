use std::sync::{Arc, Mutex};
use skulpin::skia_safe::{Canvas, Paint, Rect, Typeface, Font, FontStyle, colors};

mod caching_shaper;
mod fps_tracker;
mod font_measurer;

use caching_shaper::CachingShaper;
use fps_tracker::FpsTracker;

use crate::editor::{Editor, CursorType, Style, Colors};

const FONT_NAME: &str = "Delugia Nerd Font";
const FONT_SIZE: f32 = 14.0;

pub struct Renderer {
    editor: Arc<Mutex<Editor>>,

    paint: Paint,
    font: Font,
    shaper: CachingShaper,

    pub font_width: f32,
    pub font_height: f32,
    cursor_pos: (f32, f32),

    fps_tracker: FpsTracker
}

impl Renderer {
    pub fn new(editor: Arc<Mutex<Editor>>) -> Renderer {
        let paint = Paint::new(colors::WHITE, None);
        let typeface = Typeface::new(FONT_NAME, FontStyle::default()).expect("Could not load font file.");
        let font = Font::from_typeface(typeface, FONT_SIZE);
        let shaper = CachingShaper::new();

        let (_, bounds) = font.measure_str("0", Some(&paint));
        let font_width = bounds.width();
        let (_, metrics) = font.metrics();
        let font_height = metrics.descent - metrics.ascent;
        let cursor_pos = (0.0, 0.0);

        let fps_tracker = FpsTracker::new();

        Renderer { editor, paint, font, shaper, font_width, font_height, cursor_pos, fps_tracker }
    }

    fn draw_background(&mut self, canvas: &mut Canvas, text: &str, grid_pos: (u64, u64), style: &Style, default_colors: &Colors) {
        let (grid_x, grid_y) = grid_pos;
        let x = grid_x as f32 * self.font_width;
        let y = grid_y as f32 * self.font_height;
        let width = text.chars().count() as f32 * self.font_width;
        let height = self.font_height;
        let region = Rect::new(x, y, x + width, y + height);
        self.paint.set_color(style.background(default_colors).to_color());
        canvas.draw_rect(region, &self.paint);
    }

    fn draw_foreground(&mut self, canvas: &mut Canvas, text: &str, grid_pos: (u64, u64), style: &Style, default_colors: &Colors, update_cache: bool) {
        let (grid_x, grid_y) = grid_pos;
        let x = grid_x as f32 * self.font_width;
        let y = grid_y as f32 * self.font_height;
        let width = text.chars().count() as f32 * self.font_width;

        if style.underline || style.undercurl {
            let (_, metrics) = self.font.metrics();
            let line_position = metrics.underline_position().unwrap();

            self.paint.set_color(style.special(&default_colors).to_color());
            canvas.draw_line((x, y - line_position + self.font_height), (x + width, y - line_position + self.font_height), &self.paint);
        }

        self.paint.set_color(style.foreground(&default_colors).to_color());
        let text = text.trim_end();
        if text.len() > 0 {
            let reference;
            let blob = if update_cache {
                self.shaper.shape_cached(text.to_string(), &self.font)
            } else {
                reference = self.shaper.shape(text, &self.font);
                &reference
            };
            canvas.draw_text_blob(blob, (x, y), &self.paint);
        }
    }

    fn draw_text(&mut self, canvas: &mut Canvas, text: &str, grid_pos: (u64, u64), style: &Style, default_colors: &Colors, update_cache: bool) {
        self.draw_background(canvas, text, grid_pos, style, default_colors);
        self.draw_foreground(canvas, text, grid_pos, style, default_colors, update_cache);
    }

    pub fn draw(&mut self, canvas: &mut Canvas) {
        let (draw_commands, default_colors, (width, height), cursor_grid_pos, cursor_type, cursor_foreground, cursor_background, cursor_enabled) = {
            let editor = self.editor.lock().unwrap();
            (
                editor.build_draw_commands().clone(), 
                editor.default_colors.clone(), 
                editor.size.clone(),
                editor.cursor_pos.clone(), 
                editor.cursor_type.clone(),
                editor.cursor_foreground(),
                editor.cursor_background(),
                editor.cursor_enabled
            )
        };

        canvas.clear(default_colors.background.clone().unwrap().to_color());

        for command in draw_commands.iter() {
            self.draw_background(canvas, &command.text, command.grid_position.clone(), &command.style, &default_colors);
        }
        for command in draw_commands {
            self.draw_foreground(canvas, &command.text, command.grid_position.clone(), &command.style, &default_colors, true);
        }

        self.fps_tracker.record_frame();
        self.draw_text(canvas, &self.fps_tracker.fps.to_string(), (width - 2, height - 1), &Style::new(default_colors.clone()), &default_colors, false);

        let (cursor_grid_x, cursor_grid_y) = cursor_grid_pos;
        let target_cursor_x = cursor_grid_x as f32 * self.font_width;
        let target_cursor_y = cursor_grid_y as f32 * self.font_height;
        let (previous_cursor_x, previous_cursor_y) = self.cursor_pos;

        let cursor_x = (target_cursor_x - previous_cursor_x) * 0.5 + previous_cursor_x;
        let cursor_y = (target_cursor_y - previous_cursor_y) * 0.5 + previous_cursor_y;

        self.cursor_pos = (cursor_x, cursor_y);
        if cursor_enabled {
            let cursor_width = match cursor_type {
                CursorType::Vertical => self.font_width / 8.0,
                CursorType::Horizontal | CursorType::Block => self.font_width
            };
            let cursor_height = match cursor_type {
                CursorType::Horizontal => self.font_width / 8.0,
                CursorType::Vertical | CursorType::Block => self.font_height
            };
            let cursor = Rect::new(cursor_x, cursor_y, cursor_x + cursor_width, cursor_y + cursor_height);
            self.paint.set_color(cursor_background.to_color());
            canvas.draw_rect(cursor, &self.paint);

            if let CursorType::Block = cursor_type {
                self.paint.set_color(cursor_foreground.to_color());
                let editor = self.editor.lock().unwrap();
                let character = editor.grid[cursor_grid_y as usize][cursor_grid_x as usize].clone()
                    .map(|(character, _)| character)
                    .unwrap_or(' ');
                canvas.draw_text_blob(
                    self.shaper.shape_cached(character.to_string(), &self.font), 
                    (cursor_x, cursor_y), &self.paint);
            }
        }
    }
}
