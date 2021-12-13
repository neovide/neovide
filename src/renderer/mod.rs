pub mod animation_utils;
pub mod cursor_renderer;
mod fonts;
pub mod grid_renderer;
mod rendered_window;

use crate::WindowSettings;
use std::cmp::Ordering;
use std::collections::{hash_map::Entry, HashMap};
use std::sync::mpsc::Receiver;
use std::sync::Arc;

use log::error;
use skia_safe::Canvas;

use crate::bridge::EditorMode;
use crate::editor::{DrawCommand, WindowDrawCommand};
use crate::settings::*;
use cursor_renderer::CursorRenderer;
pub use fonts::caching_shaper::CachingShaper;
pub use grid_renderer::GridRenderer;
pub use rendered_window::{RenderedWindow, WindowDrawDetails};

#[derive(SettingGroup, Clone)]
pub struct RendererSettings {
    position_animation_length: f32,
    scroll_animation_length: f32,
    floating_opacity: f32,
    floating_blur: bool,
    debug_renderer: bool,
}

impl Default for RendererSettings {
    fn default() -> Self {
        Self {
            position_animation_length: 0.15,
            scroll_animation_length: 0.3,
            floating_opacity: 0.7,
            floating_blur: true,
            debug_renderer: false,
        }
    }
}

pub struct Renderer {
    cursor_renderer: CursorRenderer,
    pub grid_renderer: GridRenderer,
    current_mode: EditorMode,

    rendered_windows: HashMap<u64, RenderedWindow>,
    pub window_regions: Vec<WindowDrawDetails>,

    pub batched_draw_command_receiver: Receiver<Vec<DrawCommand>>,
}

impl Renderer {
    pub fn new(
        batched_draw_command_receiver: Receiver<Vec<DrawCommand>>,
        scale_factor: f64,
    ) -> Self {
        let cursor_renderer = CursorRenderer::new();
        let grid_renderer = GridRenderer::new(scale_factor);
        let current_mode = EditorMode::Unknown(String::from(""));

        let rendered_windows = HashMap::new();
        let window_regions = Vec::new();

        Renderer {
            rendered_windows,
            cursor_renderer,
            grid_renderer,
            current_mode,
            window_regions,
            batched_draw_command_receiver,
        }
    }

    pub fn font_names(&self) -> Vec<String> {
        self.grid_renderer.font_names()
    }

    /// Draws frame
    ///
    /// # Returns
    /// `bool` indicating whether or not font was changed during this frame.
    #[allow(clippy::needless_collect)]
    pub fn draw_frame(&mut self, root_canvas: &mut Canvas, dt: f32) -> bool {
        let draw_commands: Vec<_> = self
            .batched_draw_command_receiver
            .try_iter() // Iterator of Vec of DrawCommand
            .map(|batch| batch.into_iter()) // Iterator of Iterator of DrawCommand
            .flatten() // Iterator of DrawCommand
            .collect();
        let mut font_changed = false;

        for draw_command in draw_commands.into_iter() {
            if let DrawCommand::FontChanged(_) = draw_command {
                font_changed = true;
            }
            self.handle_draw_command(root_canvas, draw_command);
        }

        let default_background = self.grid_renderer.get_default_background();
        let font_dimensions = self.grid_renderer.font_dimensions;

        let transparency = { SETTINGS.get::<WindowSettings>().transparency };
        root_canvas.clear(default_background.with_a((255.0 * transparency) as u8));
        root_canvas.save();
        root_canvas.reset_matrix();

        if let Some(root_window) = self.rendered_windows.get(&1) {
            let clip_rect = root_window.pixel_region(font_dimensions);
            root_canvas.clip_rect(&clip_rect, None, Some(false));
        }

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

            floating_windows.sort_by(floating_sort);

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
                    default_background.with_a((255.0 * transparency) as u8),
                    font_dimensions,
                    dt,
                )
            })
            .collect();

        let windows = &self.rendered_windows;
        self.cursor_renderer
            .update_cursor_destination(font_dimensions.into(), windows);

        self.cursor_renderer
            .draw(&mut self.grid_renderer, &self.current_mode, root_canvas, dt);

        root_canvas.restore();

        font_changed
    }

    fn handle_draw_command(&mut self, root_canvas: &mut Canvas, draw_command: DrawCommand) {
        match draw_command {
            DrawCommand::Window {
                grid_id,
                command: WindowDrawCommand::Close,
            } => {
                self.rendered_windows.remove(&grid_id);
            }
            DrawCommand::Window { grid_id, command } => {
                match self.rendered_windows.entry(grid_id) {
                    Entry::Occupied(mut occupied_entry) => {
                        let rendered_window = occupied_entry.get_mut();
                        rendered_window
                            .handle_window_draw_command(&mut self.grid_renderer, command);
                    }
                    Entry::Vacant(vacant_entry) => {
                        if let WindowDrawCommand::Position {
                            grid_position: (grid_left, grid_top),
                            grid_size: (width, height),
                            ..
                        } = command
                        {
                            let new_window = RenderedWindow::new(
                                root_canvas,
                                &self.grid_renderer,
                                grid_id,
                                (grid_left as f32, grid_top as f32).into(),
                                (width, height).into(),
                            );
                            vacant_entry.insert(new_window);
                        } else {
                            error!("WindowDrawCommand sent for uninitialized grid {}", grid_id);
                        }
                    }
                }
            }
            DrawCommand::UpdateCursor(new_cursor) => {
                self.cursor_renderer.update_cursor(new_cursor);
            }
            DrawCommand::FontChanged(new_font) => {
                self.grid_renderer.update_font(&new_font);
            }
            DrawCommand::DefaultStyleChanged(new_style) => {
                self.grid_renderer.default_style = Arc::new(new_style);
            }
            DrawCommand::ModeChanged(new_mode) => {
                self.current_mode = new_mode;
            }
            _ => {}
        }
    }
}

/// Defines how floating windows are sorted.
fn floating_sort(window_a: &&mut RenderedWindow, window_b: &&mut RenderedWindow) -> Ordering {
    // First, compare floating order
    let mut ord = window_a
        .floating_order
        .unwrap()
        .partial_cmp(&window_b.floating_order.unwrap())
        .unwrap();
    if ord == Ordering::Equal {
        // If equal, compare grid pos x
        ord = window_a
            .grid_current_position
            .x
            .partial_cmp(&window_b.grid_current_position.x)
            .unwrap();
        if ord == Ordering::Equal {
            // If equal, compare grid pos z
            ord = window_a
                .grid_current_position
                .y
                .partial_cmp(&window_b.grid_current_position.y)
                .unwrap();
        }
    }
    ord
}
