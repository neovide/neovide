use std::collections::HashMap;
use std::sync::mpsc::Receiver;
use std::sync::Arc;

use glutin::dpi::PhysicalSize;
use log::{error, trace};
use skia_safe::{colors, dash_path_effect, BlendMode, Canvas, Color, Paint, Rect, HSV};

pub mod animation_utils;
pub mod cursor_renderer;
mod fonts;
mod rendered_window;

pub use fonts::caching_shaper::CachingShaper;
pub use rendered_window::{RenderedWindow, WindowDrawDetails};

use crate::bridge::EditorMode;
use crate::editor::{Colors, DrawCommand, Style, WindowDrawCommand};
use crate::settings::*;
use cursor_renderer::CursorRenderer;

#[derive(SettingGroup)]
#[setting_prefix = "window"]
#[derive(Clone)]
pub struct RendererSettings {
    position_animation_length: f32,
    scroll_animation_length: f32,
    floating_opacity: f32,
    floating_blur: bool,
}

impl Default for RendererSettings {
    fn default() -> Self {
        Self {
            position_animation_length: 0.15,
            scroll_animation_length: 0.3,
            floating_opacity: 0.7,
            floating_blur: true,
        }
    }
}

pub struct Renderer {
    rendered_windows: HashMap<u64, RenderedWindow>,
    cursor_renderer: CursorRenderer,

    pub current_mode: EditorMode,
    pub paint: Paint,
    pub shaper: CachingShaper,
    pub default_style: Arc<Style>,
    pub font_width: u64,
    pub font_height: u64,
    pub window_regions: Vec<WindowDrawDetails>,
    pub batched_draw_command_receiver: Receiver<Vec<DrawCommand>>,
    pub is_ready: bool,
}

impl Renderer {
    pub fn new(
        batched_draw_command_receiver: Receiver<Vec<DrawCommand>>,
        scale_factor: f64,
    ) -> Renderer {
        let rendered_windows = HashMap::new();
        let cursor_renderer = CursorRenderer::new();

        let current_mode = EditorMode::Unknown(String::from(""));
        let mut paint = Paint::new(colors::WHITE, None);
        paint.set_anti_alias(false);
        let mut shaper = CachingShaper::new(scale_factor as f32);
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
            current_mode,
            paint,
            shaper,
            default_style,
            font_width,
            font_height,
            window_regions,
            batched_draw_command_receiver,
            is_ready: false,
        }
    }

    // TODO: Refactor code to use these two functions instead of multiplication.
    /// Convert PhysicalSize to grid size
    pub fn to_grid_size(&self, new_size: PhysicalSize<u32>) -> (u64, u64) {
        let width = new_size.width as u64 / self.font_width;
        let height = new_size.height as u64 / self.font_height;
        (width, height)
    }

    /// Convert grid size to PhysicalSize
    pub fn to_physical_size(&self, new_size: (u64, u64)) -> PhysicalSize<u32> {
        let (width, height) = new_size;
        PhysicalSize {
            width: (width * self.font_width) as u32,
            height: (height * self.font_height) as u32,
        }
    }

    pub fn handle_scale_factor_update(&mut self, scale_factor: f64) {
        self.shaper.update_scale_factor(scale_factor as f32);
        self.update_font_dimensions();
    }

    fn update_font(&mut self, guifont_setting: &str) {
        self.shaper.update_font(guifont_setting);
        self.update_font_dimensions();
    }

    fn update_font_dimensions(&mut self) {
        let (font_width, font_height) = self.shaper.font_base_dimensions();
        self.font_width = font_width;
        self.font_height = font_height;
        self.is_ready = true;
        trace!(
            "Updating font dimensions: {}x{}",
            self.font_width,
            self.font_height,
        );
    }

    fn compute_text_region(&self, grid_pos: (u64, u64), cell_width: u64) -> Rect {
        let (grid_x, grid_y) = grid_pos;
        let x = grid_x * self.font_width;
        let y = grid_y * self.font_height;
        let width = cell_width * self.font_width;
        let height = self.font_height;
        Rect::new(x as f32, y as f32, (x + width) as f32, (y + height) as f32)
    }

    fn get_default_background(&self) -> Color {
        self.default_style.colors.background.unwrap().to_color()
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

        if cfg!(feature = "debug-renderer") {
            let random_hsv: HSV = (rand::random::<f32>() * 360.0, 0.3, 0.3).into();
            let random_color = random_hsv.to_color(255);
            self.paint.set_color(random_color);
        } else {
            self.paint
                .set_color(style.background(&self.default_style.colors).to_color());
        }
        canvas.draw_rect(region, &self.paint);
    }

    fn draw_foreground(
        &mut self,
        canvas: &mut Canvas,
        cells: &[String],
        grid_pos: (u64, u64),
        cell_width: u64,
        style: &Option<Arc<Style>>,
    ) {
        let (grid_x, grid_y) = grid_pos;
        let x = grid_x * self.font_width;
        let y = grid_y * self.font_height;
        let width = cell_width * self.font_width;

        let style = style.as_ref().unwrap_or(&self.default_style);

        canvas.save();

        let region = self.compute_text_region(grid_pos, cell_width);

        canvas.clip_rect(region, None, Some(false));

        if style.underline || style.undercurl {
            let line_position = self.shaper.underline_position();
            let stroke_width = self.shaper.current_size() / 10.0;

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
                (x as f32, (y - line_position + self.font_height) as f32),
                (
                    (x + width) as f32,
                    (y - line_position + self.font_height) as f32,
                ),
                &self.paint,
            );
        }

        let y_adjustment = self.shaper.y_adjustment();

        if cfg!(feature = "debug-renderer") {
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
            .shape_cached(cells, style.bold, style.italic)
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

    pub fn handle_draw_command(&mut self, root_canvas: &mut Canvas, draw_command: DrawCommand) {
        match draw_command {
            DrawCommand::Window {
                grid_id,
                command: WindowDrawCommand::Close,
            } => {
                self.rendered_windows.remove(&grid_id);
            }
            DrawCommand::Window { grid_id, command } => {
                if let Some(rendered_window) = self.rendered_windows.remove(&grid_id) {
                    let rendered_window = rendered_window.handle_window_draw_command(self, command);
                    self.rendered_windows.insert(grid_id, rendered_window);
                } else if let WindowDrawCommand::Position {
                    grid_position: (grid_left, grid_top),
                    grid_size: (width, height),
                    ..
                } = command
                {
                    let new_window = RenderedWindow::new(
                        root_canvas,
                        self,
                        grid_id,
                        (grid_left as f32, grid_top as f32).into(),
                        width,
                        height,
                    );
                    self.rendered_windows.insert(grid_id, new_window);
                } else {
                    error!("WindowDrawCommand sent for uninitialized grid {}", grid_id);
                }
            }
            DrawCommand::UpdateCursor(new_cursor) => {
                self.cursor_renderer.update_cursor(new_cursor);
            }
            DrawCommand::FontChanged(new_font) => {
                self.update_font(&new_font);
            }
            DrawCommand::DefaultStyleChanged(new_style) => {
                self.default_style = Arc::new(new_style);
            }
            DrawCommand::ModeChanged(new_mode) => {
                self.current_mode = new_mode;
            }
            _ => {}
        }
    }

    #[allow(clippy::needless_collect)]
    pub fn draw_frame(&mut self, root_canvas: &mut Canvas, dt: f32) -> bool {
        let mut font_changed = false;

        let draw_commands: Vec<_> = self
            .batched_draw_command_receiver
            .try_iter() // Iterator of Vec of DrawCommand
            .map(|batch| batch.into_iter()) // Iterator of Iterator of DrawCommand
            .flatten() // Iterator of DrawCommand
            .collect();

        for draw_command in draw_commands.into_iter() {
            if let DrawCommand::FontChanged(_) = draw_command {
                font_changed = true;
            }
            self.handle_draw_command(root_canvas, draw_command);
        }

        root_canvas.clear(self.default_style.colors.background.unwrap().to_color());
        root_canvas.save();
        root_canvas.reset_matrix();

        if let Some(root_window) = self.rendered_windows.get(&1) {
            let clip_rect = root_window.pixel_region(self.font_width, self.font_height);
            root_canvas.clip_rect(&clip_rect, None, Some(false));
        }

        let default_background = self.get_default_background();
        let font_width = self.font_width;
        let font_height = self.font_height;

        let windows: Vec<&mut RenderedWindow> = {
            let (mut root_windows, mut floating_windows): (
                Vec<&mut RenderedWindow>,
                Vec<&mut RenderedWindow>,
            ) = self
                .rendered_windows
                .values_mut()
                .filter(|window| !window.hidden)
                .partition(|window| window.floating_order.is_none());

            root_windows
                .sort_by(|window_a, window_b| window_a.id.partial_cmp(&window_b.id).unwrap());
            floating_windows.sort_by(|window_a, window_b| {
                window_a
                    .floating_order
                    .unwrap()
                    .partial_cmp(&window_b.floating_order.unwrap())
                    .unwrap()
            });

            root_windows
                .into_iter()
                .chain(floating_windows.into_iter())
                .collect()
        };

        let settings = SETTINGS.get::<RendererSettings>();
        self.window_regions = windows
            .into_iter()
            .map(|window| {
                window.draw(
                    root_canvas,
                    &settings,
                    default_background,
                    font_width,
                    font_height,
                    dt,
                )
            })
            .collect();

        let windows = &self.rendered_windows;
        self.cursor_renderer
            .update_cursor_destination(font_width, font_height, windows);

        self.cursor_renderer.draw(
            &self.default_style.colors,
            (self.font_width, self.font_height),
            &self.current_mode,
            &mut self.shaper,
            root_canvas,
            dt,
        );

        root_canvas.restore();

        font_changed
    }
}
