pub mod animation_utils;
pub mod cursor_renderer;
pub mod fonts;
pub mod grid_renderer;
mod opengl;
pub mod profiler;
mod rendered_window;
mod vsync;

use std::{
    cmp::Ordering,
    collections::{hash_map::Entry, HashMap},
    sync::Arc,
};

use log::error;
use skia_safe::{Canvas, Point, Rect};
use winit::event::Event;

use crate::{
    bridge::EditorMode,
    dimensions::Dimensions,
    editor::{Cursor, Style},
    profiling::{tracy_named_frame, tracy_zone},
    settings::*,
    window::{ShouldRender, UserEvent},
    WindowSettings,
};

use cursor_renderer::CursorRenderer;
pub use fonts::caching_shaper::CachingShaper;
pub use grid_renderer::GridRenderer;
pub use rendered_window::{LineFragment, RenderedWindow, WindowDrawCommand, WindowDrawDetails};

pub use opengl::{build_context, build_window, Context as WindowedContext, GlWindow};
pub use vsync::VSync;

#[derive(SettingGroup, Clone)]
pub struct RendererSettings {
    position_animation_length: f32,
    scroll_animation_length: f32,
    scroll_animation_far_lines: u32,
    floating_blur: bool,
    floating_blur_amount_x: f32,
    floating_blur_amount_y: f32,
    floating_shadow: bool,
    floating_z_height: f32,
    light_angle_degrees: f32,
    light_radius: f32,
    debug_renderer: bool,
    profiler: bool,
    underline_stroke_scale: f32,
}

impl Default for RendererSettings {
    fn default() -> Self {
        Self {
            position_animation_length: 0.15,
            scroll_animation_length: 0.3,
            scroll_animation_far_lines: 1,
            floating_blur: true,
            floating_blur_amount_x: 2.0,
            floating_blur_amount_y: 2.0,
            floating_shadow: true,
            floating_z_height: 10.,
            light_angle_degrees: 45.,
            light_radius: 5.,
            debug_renderer: false,
            profiler: false,
            underline_stroke_scale: 1.,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
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
    UIReady,
}

pub struct Renderer {
    cursor_renderer: CursorRenderer,
    pub grid_renderer: GridRenderer,
    current_mode: EditorMode,

    rendered_windows: HashMap<u64, RenderedWindow>,
    pub window_regions: Vec<WindowDrawDetails>,

    profiler: profiler::Profiler,
    os_scale_factor: f64,
    user_scale_factor: f64,
}

/// Results of processing the draw commands from the command channel.
pub struct DrawCommandResult {
    pub font_changed: bool,
    pub should_show: bool,
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

        let profiler = profiler::Profiler::new(12.0);

        Renderer {
            rendered_windows,
            cursor_renderer,
            grid_renderer,
            current_mode,
            window_regions,
            profiler,
            os_scale_factor,
            user_scale_factor,
        }
    }

    pub fn handle_event(&mut self, event: &Event<UserEvent>) -> bool {
        self.cursor_renderer.handle_event(event)
    }

    pub fn font_names(&self) -> Vec<String> {
        self.grid_renderer.font_names()
    }

    pub fn prepare_frame(&mut self) -> ShouldRender {
        self.cursor_renderer.prepare_frame()
    }

    pub fn draw_frame(&mut self, root_canvas: &Canvas, dt: f32) {
        tracy_zone!("renderer_draw_frame");
        let default_background = self.grid_renderer.get_default_background();
        let font_dimensions = self.grid_renderer.font_dimensions;

        let transparency = { SETTINGS.get::<WindowSettings>().transparency };
        root_canvas.clear(default_background.with_a((255.0 * transparency) as u8));
        root_canvas.save();
        root_canvas.reset_matrix();

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
                .partition(|window| window.anchor_info.is_none());

            root_windows
                .sort_by(|window_a, window_b| window_a.id.partial_cmp(&window_b.id).unwrap());

            floating_windows.sort_by(floating_sort);

            root_windows.into_iter().chain(floating_windows).collect()
        };

        let settings = SETTINGS.get::<RendererSettings>();
        let mut floating_rects = Vec::new();

        self.window_regions = windows
            .into_iter()
            .map(|window| {
                window.draw(
                    root_canvas,
                    &settings,
                    default_background.with_a((255.0 * transparency) as u8),
                    font_dimensions,
                    &mut floating_rects,
                )
            })
            .collect();

        self.cursor_renderer
            .draw(&mut self.grid_renderer, root_canvas);

        self.profiler.draw(root_canvas, dt);

        root_canvas.restore();
    }

    pub fn animate_frame(
        &mut self,
        window_size: &Dimensions,
        padding_as_grid: &Rect,
        dt: f32,
    ) -> bool {
        let windows = {
            let (mut root_windows, mut floating_windows): (
                Vec<&mut RenderedWindow>,
                Vec<&mut RenderedWindow>,
            ) = self
                .rendered_windows
                .values_mut()
                .filter(|window| !window.hidden)
                .partition(|window| window.anchor_info.is_none());

            root_windows
                .sort_by(|window_a, window_b| window_a.id.partial_cmp(&window_b.id).unwrap());

            floating_windows.sort_by(floating_sort);

            root_windows.into_iter().chain(floating_windows)
        };

        let settings = SETTINGS.get::<RendererSettings>();
        // Clippy recommends short-circuiting with any which is not what we want
        #[allow(clippy::unnecessary_fold)]
        let mut animating = windows.fold(false, |acc, window| {
            acc | window.animate(&settings, window_size, padding_as_grid, dt)
        });

        let windows = &self.rendered_windows;
        let font_dimensions = self.grid_renderer.font_dimensions;
        self.cursor_renderer
            .update_cursor_destination(font_dimensions.into(), windows);

        animating |= self
            .cursor_renderer
            .animate(&self.current_mode, &self.grid_renderer, dt);

        animating
    }

    pub fn handle_draw_commands(&mut self, batch: Vec<DrawCommand>) -> DrawCommandResult {
        let settings = SETTINGS.get::<RendererSettings>();
        let mut result = DrawCommandResult {
            font_changed: false,
            should_show: false,
        };

        for draw_command in batch {
            self.handle_draw_command(draw_command, &mut result);
            tracy_named_frame!("neovim draw batch processed");
        }
        self.flush(&settings);

        let user_scale_factor = SETTINGS.get::<WindowSettings>().scale_factor.into();
        if user_scale_factor != self.user_scale_factor {
            self.user_scale_factor = user_scale_factor;
            self.grid_renderer
                .handle_scale_factor_update(self.os_scale_factor * self.user_scale_factor);
            result.font_changed = true;
        }

        result
    }

    pub fn handle_os_scale_factor_change(&mut self, os_scale_factor: f64) {
        self.os_scale_factor = os_scale_factor;
        self.grid_renderer
            .handle_scale_factor_update(self.os_scale_factor * self.user_scale_factor);
    }

    pub fn prepare_lines(&mut self) {
        self.rendered_windows
            .iter_mut()
            .for_each(|(_, w)| w.prepare_lines(&mut self.grid_renderer));
    }

    fn handle_draw_command(&mut self, draw_command: DrawCommand, result: &mut DrawCommandResult) {
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
                        rendered_window.handle_window_draw_command(command);
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
                result.font_changed = true;
            }
            DrawCommand::LineSpaceChanged(new_linespace) => {
                self.grid_renderer.update_linespace(new_linespace);
                result.font_changed = true;
            }
            DrawCommand::DefaultStyleChanged(new_style) => {
                self.grid_renderer.default_style = Arc::new(new_style);
            }
            DrawCommand::ModeChanged(new_mode) => {
                self.current_mode = new_mode;
            }
            DrawCommand::UIReady => {
                result.should_show = true;
            }
            _ => {}
        }
    }

    pub fn flush(&mut self, renderer_settings: &RendererSettings) {
        self.rendered_windows
            .iter_mut()
            .for_each(|(_, w)| w.flush(renderer_settings));
    }

    pub fn get_cursor_position(&self) -> Point {
        self.cursor_renderer.get_current_position()
    }

    pub fn get_grid_size(&self) -> Dimensions {
        if let Some(main_grid) = self.rendered_windows.get(&1) {
            main_grid.grid_size
        } else {
            DEFAULT_GRID_SIZE
        }
    }
}

/// Defines how floating windows are sorted.
fn floating_sort(window_a: &&mut RenderedWindow, window_b: &&mut RenderedWindow) -> Ordering {
    // First, compare floating order
    let mut ord = window_a
        .anchor_info
        .as_ref()
        .unwrap()
        .sort_order
        .partial_cmp(&window_b.anchor_info.as_ref().unwrap().sort_order)
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
