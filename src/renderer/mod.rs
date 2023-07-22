pub mod animation_utils;
pub mod cursor_renderer;
pub mod fonts;
pub mod grid_renderer;
mod opengl;
pub mod profiler;
mod rendered_window;

use std::{
    cmp::Ordering,
    collections::{hash_map::Entry, HashMap},
    sync::Arc,
};

use log::error;
use skia_safe::{Canvas, Point};
use tokio::sync::mpsc::UnboundedReceiver;
use winit::event::Event;

use crate::{
    bridge::EditorMode,
    editor::{Cursor, Style},
    event_aggregator::EVENT_AGGREGATOR,
    profiling::tracy_zone,
    settings::*,
    WindowSettings,
};

use cursor_renderer::CursorRenderer;
pub use fonts::caching_shaper::CachingShaper;
pub use grid_renderer::GridRenderer;
pub use rendered_window::{
    LineFragment, RenderedWindow, WindowDrawCommand, WindowDrawDetails, WindowPadding,
};

pub use opengl::{build_context, build_window, Context as WindowedContext};

#[derive(SettingGroup, Clone)]
pub struct RendererSettings {
    position_animation_length: f32,
    scroll_animation_length: f32,
    scroll_animation_far_scroll_lines: u32,
    floating_blur: bool,
    floating_blur_amount_x: f32,
    floating_blur_amount_y: f32,
    debug_renderer: bool,
    profiler: bool,
    underline_automatic_scaling: bool,
}

impl Default for RendererSettings {
    fn default() -> Self {
        Self {
            position_animation_length: 0.15,
            scroll_animation_length: 0.3,
            scroll_animation_far_scroll_lines: 1,
            floating_blur: true,
            floating_blur_amount_x: 2.0,
            floating_blur_amount_y: 2.0,
            debug_renderer: false,
            profiler: false,
            underline_automatic_scaling: false,
        }
    }
}

#[derive(Clone, Debug)]
pub enum DrawCommand {
    CloseWindow(u64),
    Window {
        grid_id: u64,
        command: WindowDrawCommand,
    },
    UpdateCursor(Cursor),
    FontChanged(String),
    LineSpaceChanged(i64),
    DefaultStyleChanged(Style),
    ModeChanged(EditorMode),
}

pub struct Renderer {
    cursor_renderer: CursorRenderer,
    pub grid_renderer: GridRenderer,
    current_mode: EditorMode,

    rendered_windows: HashMap<u64, RenderedWindow>,
    pub window_regions: Vec<WindowDrawDetails>,

    pub batched_draw_command_receiver: UnboundedReceiver<Vec<DrawCommand>>,
    profiler: profiler::Profiler,
    os_scale_factor: f64,
    user_scale_factor: f64,
    pub window_padding: WindowPadding,
}

impl Renderer {
    pub fn new(os_scale_factor: f64) -> Self {
        let window_settings = SETTINGS.get::<WindowSettings>();

        let user_scale_factor = window_settings.scale_factor.into();
        let scale_factor = user_scale_factor * os_scale_factor;
        let cursor_renderer = CursorRenderer::new();
        let grid_renderer = GridRenderer::new(scale_factor);
        let current_mode = EditorMode::Unknown(String::from(""));

        let rendered_windows = HashMap::new();
        let window_regions = Vec::new();

        let batched_draw_command_receiver = EVENT_AGGREGATOR.register_event::<Vec<DrawCommand>>();
        let profiler = profiler::Profiler::new(12.0);

        let window_padding = WindowPadding {
            top: window_settings.padding_top,
            left: window_settings.padding_left,
            right: window_settings.padding_right,
            bottom: window_settings.padding_bottom,
        };

        Renderer {
            rendered_windows,
            cursor_renderer,
            grid_renderer,
            current_mode,
            window_regions,
            batched_draw_command_receiver,
            profiler,
            os_scale_factor,
            user_scale_factor,
            window_padding,
        }
    }

    pub fn handle_event(&mut self, event: &Event<()>) {
        self.cursor_renderer.handle_event(event);
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
        tracy_zone!("renderer_draw_frame");
        let mut draw_commands = Vec::new();
        while let Ok(draw_command) = self.batched_draw_command_receiver.try_recv() {
            draw_commands.extend(draw_command);
        }

        let mut font_changed = false;

        let settings = SETTINGS.get::<RendererSettings>();

        for draw_command in draw_commands.into_iter() {
            if let DrawCommand::FontChanged(_) | DrawCommand::LineSpaceChanged(_) = draw_command {
                font_changed = true;
            }
            self.handle_draw_command(draw_command, &settings);
        }

        let default_background = self.grid_renderer.get_default_background();
        let font_dimensions = self.grid_renderer.font_dimensions;

        let transparency = { SETTINGS.get::<WindowSettings>().transparency };
        root_canvas.clear(default_background.with_a((255.0 * transparency) as u8));
        root_canvas.save();
        root_canvas.reset_matrix();

        let user_scale_factor = SETTINGS.get::<WindowSettings>().scale_factor.into();
        if user_scale_factor != self.user_scale_factor {
            self.user_scale_factor = user_scale_factor;
            self.grid_renderer
                .handle_scale_factor_update(self.os_scale_factor * self.user_scale_factor);
            font_changed = true;
        }

        if let Some(root_window) = self.rendered_windows.get(&1) {
            let clip_rect = root_window.pixel_region(font_dimensions);
            root_canvas.clip_rect(clip_rect, None, Some(false));
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

        self.window_regions = windows
            .into_iter()
            .map(|window| {
                if window.padding != self.window_padding {
                    window.padding = self.window_padding;
                }

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

        self.profiler.draw(root_canvas, dt);

        root_canvas.restore();

        font_changed
    }

    pub fn handle_os_scale_factor_change(&mut self, os_scale_factor: f64) {
        self.os_scale_factor = os_scale_factor;
        self.grid_renderer
            .handle_scale_factor_update(self.os_scale_factor * self.user_scale_factor);
    }

    fn handle_draw_command(
        &mut self,
        draw_command: DrawCommand,
        renderer_settings: &RendererSettings,
    ) {
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
                        rendered_window.handle_window_draw_command(
                            &mut self.grid_renderer,
                            command,
                            renderer_settings,
                        );
                    }
                    Entry::Vacant(vacant_entry) => {
                        if let WindowDrawCommand::Position {
                            grid_position: (grid_left, grid_top),
                            grid_size: (width, height),
                            ..
                        } = command
                        {
                            let new_window = RenderedWindow::new(
                                grid_id,
                                (grid_left as f32, grid_top as f32).into(),
                                (width, height).into(),
                                self.window_padding,
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
            DrawCommand::LineSpaceChanged(new_linespace) => {
                self.grid_renderer.update_linespace(new_linespace);
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

    pub fn get_cursor_position(&self) -> Point {
        self.cursor_renderer.get_current_position()
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
        // if equal, compare grid pos x
        ord = window_a
            .grid_current_position
            .x
            .partial_cmp(&window_b.grid_current_position.x)
            .unwrap();
        if ord == Ordering::Equal {
            // if equal, compare grid pos z
            ord = window_a
                .grid_current_position
                .y
                .partial_cmp(&window_b.grid_current_position.y)
                .unwrap();
        }
    }
    ord
}
