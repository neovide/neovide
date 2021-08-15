pub mod animation_utils;
pub mod cursor_renderer;
mod fonts;
pub mod grid_renderer;
mod rendered_window;
mod skia_renderer;

use std::collections::{hash_map::Entry, HashMap};
use std::sync::mpsc::Receiver;
use std::sync::Arc;

use glutin::{PossiblyCurrent, WindowedContext};
use glutin::dpi::PhysicalSize;
use log::{trace,error};
use skia_safe::Canvas;

use crate::cmd_line::CmdLineSettings;
use crate::bridge::EditorMode;
use crate::editor::{DrawCommand, WindowDrawCommand};
use crate::settings::*;
use crate::utils::Dimensions;
use cursor_renderer::CursorRenderer;
pub use fonts::caching_shaper::CachingShaper;
pub use grid_renderer::GridRenderer;
pub use rendered_window::{RenderedWindow, WindowDrawDetails};
use skia_renderer::SkiaRenderer;

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
    pub saved_grid_size: Option<Dimensions>,
    skia_renderer: SkiaRenderer,

    saved_inner_size: PhysicalSize<u32>,
}

impl Renderer {
    pub fn new(
        batched_draw_command_receiver: Receiver<Vec<DrawCommand>>,
        windowed_context: &WindowedContext<PossiblyCurrent>,
    ) -> Self {
        let window = windowed_context.window();
        let scale_factor = window.scale_factor();
        let cursor_renderer = CursorRenderer::new();
        let grid_renderer = GridRenderer::new(scale_factor);
        let current_mode = EditorMode::Unknown(String::from(""));

        log::info!(
            "Creating Renderer (scale_factor: {:.4}, font_dimensions: {:?})",
            scale_factor,
            grid_renderer.font_dimensions,
        );

        let rendered_windows = HashMap::new();
        let window_regions = Vec::new();

        Renderer {
            rendered_windows,
            cursor_renderer,
            grid_renderer,
            current_mode,
            window_regions,
            batched_draw_command_receiver,
            saved_inner_size: window.inner_size(),
            saved_grid_size: None,
            skia_renderer: SkiaRenderer::new(&windowed_context),
        }
    }

    /// Draws frame
    ///
    /// # Returns
    /// `bool` indicating whether or not font was changed during this frame.
    #[allow(clippy::needless_collect)]
    pub fn draw_frame(&mut self, dt: f32) -> bool {
        let draw_commands: Vec<_> = self
            .batched_draw_command_receiver
            .try_iter() // Iterator of Vec of DrawCommand
            .map(|batch| batch.into_iter()) // Iterator of Iterator of DrawCommand
            .flatten() // Iterator of DrawCommand
            .collect();
        let mut font_changed = false;
        let root_canvas = self.skia_renderer.canvas();

        for draw_command in draw_commands.into_iter() {
            if let DrawCommand::FontChanged(_) = draw_command {
                font_changed = true;
            }
            self.handle_draw_command(root_canvas, draw_command);
        }

        let default_background = self.grid_renderer.get_default_background();
        let font_dimensions = self.grid_renderer.font_dimensions;

        root_canvas.clear(default_background);
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
        self.skia_renderer.gr_context.flush(None);

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

    pub fn post_redraw(
        &mut self,
        windowed_context: &WindowedContext<PossiblyCurrent>,
        mut font_changed: bool,
    ) -> Option<Dimensions> {
        // Wait until fonts are loaded, so we can set proper window size.
        if !self.grid_renderer.is_ready {
            return None;
        }

        let window = windowed_context.window();
        if self.saved_grid_size.is_none() && !window.is_maximized() {
            let size = SETTINGS.get::<CmdLineSettings>().geometry;
            window.set_inner_size(self.grid_renderer.convert_grid_to_physical(size));
            self.saved_grid_size = Some(size);
            // Font change at startup is ignored, so grid size (and startup screen) could be preserved.
            font_changed = false;
        }

        let new_size = window.inner_size();
        let mut retval = None;

        if self.saved_inner_size != new_size || font_changed {
            self.saved_inner_size = new_size;
            retval = self.handle_new_grid_size(new_size);
            self.skia_renderer.resize(windowed_context);
        }
        retval
    }

    fn handle_new_grid_size(&mut self, new_size: PhysicalSize<u32>) -> Option<Dimensions> {
        let grid_size = self
            .grid_renderer
            .convert_physical_to_grid(new_size);
        if self.saved_grid_size == Some(grid_size) {
            trace!("Grid matched saved size, skip update.");
            return None;
        }
        self.saved_grid_size = Some(grid_size);
        self.saved_grid_size
    }
}
