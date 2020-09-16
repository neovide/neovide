use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::Receiver;

use log::{trace, warn, error};
use skulpin::skia_safe::{
    colors, dash_path_effect, Canvas, Paint, Rect, BlendMode, Color
};
use skulpin::CoordinateSystemHelper;

mod caching_shaper;
mod rendered_window;
pub mod cursor_renderer;
pub mod font_options;
pub mod animation_utils;

pub use caching_shaper::CachingShaper;
pub use font_options::*;

use crate::editor::{Style, Colors, DrawCommand, WindowDrawCommand};
use crate::settings::*;
use cursor_renderer::CursorRenderer;
use rendered_window::RenderedWindow;


// ----------------------------------------------------------------------------

#[derive(Clone)]
pub struct RendererSettings {
    animation_length: f32,
    floating_opacity: f32,
    floating_blur: bool,
}

pub fn initialize_settings() {
    SETTINGS.set(&RendererSettings {
        animation_length: 0.15,
        floating_opacity: 0.7,
        floating_blur: true,
    });

    register_nvim_setting!("window_animation_length", RendererSettings::animation_length);
    register_nvim_setting!("floating_window_opacity", RendererSettings::floating_opacity);
    register_nvim_setting!("floating_window_blur", RendererSettings::floating_opacity);
}

// ----------------------------------------------------------------------------


pub struct Renderer {
    rendered_windows: HashMap<u64, RenderedWindow>,
    cursor_renderer: CursorRenderer,
    settings: RendererSettings,

    pub paint: Paint,
    pub shaper: CachingShaper,
    pub default_style: Arc<Style>,
    pub font_width: f32,
    pub font_height: f32,
    pub window_regions: Vec<(u64, Rect)>,
    pub draw_command_receiver: Receiver<DrawCommand>,
}

impl Renderer {
    pub fn new(draw_command_receiver: Receiver<DrawCommand>) -> Renderer {
        let rendered_windows = HashMap::new();
        let cursor_renderer = CursorRenderer::new();
        let settings = SETTINGS.get::<RendererSettings>();

        let mut paint = Paint::new(colors::WHITE, None);
        paint.set_anti_alias(false);
        let mut shaper = CachingShaper::new();
        let (font_width, font_height) = shaper.font_base_dimensions();
        let default_style = Arc::new(Style::new(Colors::new(
            Some(colors::WHITE),
            Some(colors::BLACK),
            Some(colors::GREY),
        )));
        let window_regions = Vec::new();

        Renderer {
            rendered_windows,
            cursor_renderer,
            settings,

            paint,
            shaper,
            default_style,
            font_width,
            font_height,
            window_regions,
            draw_command_receiver,
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
    ) {
        self.paint.set_blend_mode(BlendMode::Src);

        let region = self.compute_text_region(grid_pos, cell_width);
        let style = style.as_ref().unwrap_or(&self.default_style);

        self.paint.set_color(style.background(&self.default_style.colors).to_color());
        canvas.draw_rect(region, &self.paint);
    }

    fn draw_foreground(
        &mut self,
        canvas: &mut Canvas,
        text: &str,
        grid_pos: (u64, u64),
        cell_width: u64,
        style: &Option<Arc<Style>>,
    ) {
        let (grid_x, grid_y) = grid_pos;
        let x = grid_x as f32 * self.font_width;
        let y = grid_y as f32 * self.font_height;
        let width = cell_width as f32 * self.font_width;

        let style = style.as_ref().unwrap_or(&self.default_style);

        canvas.save();

        let region = self.compute_text_region(grid_pos, cell_width);

        canvas.clip_rect(region, None, Some(false));
        self.paint.set_blend_mode(BlendMode::Src);
        let transparent = Color::from_argb(0, 255, 255, 255);
        self.paint.set_color(transparent);
        canvas.draw_rect(region, &self.paint);

        if style.underline || style.undercurl {
            let line_position = self.shaper.underline_position();
            let stroke_width = self.shaper.options.size / 10.0;
            self.paint
                .set_color(style.special(&self.default_style.colors).to_color());
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
            .set_color(style.foreground(&self.default_style.colors).to_color());
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
                .set_color(style.special(&self.default_style.colors).to_color());
            canvas.draw_line((x, line_position), (x + width, line_position), &self.paint);
        }

        canvas.restore();
    }

    pub fn handle_draw_command(&mut self, root_canvas: &mut Canvas, draw_command: DrawCommand) {
        warn!("{:?}", &draw_command);
        match draw_command {
            DrawCommand::Window {
                grid_id,
                command: WindowDrawCommand::Close
            } => {
                self.rendered_windows.remove(&grid_id);
            },
            DrawCommand::Window {
                grid_id,
                command
            } => {
                if let Some(rendered_window) = self.rendered_windows.remove(&grid_id) {
                    warn!("Window positioned {}", grid_id);
                    let rendered_window = rendered_window.handle_window_draw_command(self, command);
                    self.rendered_windows.insert(grid_id, rendered_window);
                } else if let WindowDrawCommand::Position { 
                    grid_left, grid_top,
                    width, height, ..
                } = command {
                    warn!("Created window {}", grid_id);
                    let new_window = RenderedWindow::new(
                        root_canvas, &self, grid_id, 
                        (grid_left as f32, grid_top as f32).into(), 
                        width, height);
                    self.rendered_windows.insert(grid_id, new_window);
                } else {
                    error!("WindowDrawCommand sent for uninitialized grid {}", grid_id);
                }
            },
            DrawCommand::UpdateCursor(new_cursor) => {
                self.cursor_renderer.update_cursor(new_cursor);
            },
            DrawCommand::FontChanged(new_font) => {
                if self.update_font(&new_font) {
                    // Resize all the grids
                }
            },
            DrawCommand::DefaultStyleChanged(new_style) => {
                self.default_style = Arc::new(new_style);
            },
            _ => { }
        }
    }

    pub fn draw_frame(
        &mut self,
        root_canvas: &mut Canvas,
        coordinate_system_helper: &CoordinateSystemHelper,
        dt: f32,
    ) {
        trace!("Rendering");

        let draw_commands: Vec<DrawCommand> = self.draw_command_receiver.try_iter().collect();
        for draw_command in draw_commands.into_iter() {
            self.handle_draw_command(root_canvas, draw_command);
        }

        root_canvas.clear(self.default_style.colors.background.clone().unwrap().to_color());

        coordinate_system_helper.use_logical_coordinates(root_canvas);

        let windows: Vec<&mut RenderedWindow> = {
            let (mut root_windows, mut floating_windows): (Vec<&mut RenderedWindow>, Vec<&mut RenderedWindow>) = self.rendered_windows
                .values_mut()
                .filter(|window| !window.hidden)
                .partition(|window| !window.floating);

            root_windows.sort_by(|window_a, window_b| window_a.id.partial_cmp(&window_b.id).unwrap());
            floating_windows.sort_by(|window_a, window_b| window_a.id.partial_cmp(&window_b.id).unwrap());

            root_windows.into_iter().chain(floating_windows.into_iter()).collect()
        };

        let settings = &self.settings;
        let font_width = self.font_width;
        let font_height = self.font_height;
        self.window_regions = windows
            .into_iter()
            .map(|window| window.draw(root_canvas, settings, font_width, font_height, dt))
            .collect();

        self.cursor_renderer.draw(
            &self.default_style.colors,
            (self.font_width, self.font_height),
            &mut self.shaper,
            root_canvas,
            dt,
        );
    }
}
