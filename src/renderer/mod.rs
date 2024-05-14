pub mod animation_utils;
pub mod cursor_renderer;
pub mod fonts;
pub mod grid_renderer;
pub mod opengl;
pub mod profiler;
mod rendered_layer;
mod rendered_window;
mod vsync;

#[cfg(target_os = "windows")]
pub mod d3d;

use std::{
    cmp::Ordering,
    collections::{hash_map::Entry, HashMap},
    sync::Arc,
};

use itertools::Itertools;
use log::{error, warn};
use skia_safe::Canvas;

use winit::{
    event::Event,
    event_loop::{EventLoop, EventLoopProxy},
    window::{Window, WindowBuilder},
};

use crate::{
    bridge::EditorMode,
    editor::{Cursor, Style},
    profiling::{tracy_create_gpu_context, tracy_named_frame, tracy_zone},
    renderer::rendered_layer::{group_windows, FloatingLayer},
    settings::*,
    units::{to_skia_rect, GridPos, GridRect, GridSize, PixelPos},
    window::{ShouldRender, UserEvent},
    WindowSettings,
};

#[cfg(feature = "profiling")]
use crate::profiling::tracy_plot;
#[cfg(feature = "profiling")]
use skia_safe::graphics::{
    font_cache_count_limit, font_cache_count_used, font_cache_limit, font_cache_used,
    resource_cache_single_allocation_byte_limit, resource_cache_total_bytes_limit,
    resource_cache_total_bytes_used,
};

#[cfg(feature = "gpu_profiling")]
use crate::profiling::GpuCtx;

#[cfg(target_os = "windows")]
use crate::CmdLineSettings;

use cursor_renderer::CursorRenderer;
pub use fonts::caching_shaper::CachingShaper;
pub use grid_renderer::GridRenderer;
pub use rendered_window::{LineFragment, RenderedWindow, WindowDrawCommand, WindowDrawDetails};

pub use vsync::VSync;

use self::fonts::font_options::FontOptions;

#[cfg(feature = "profiling")]
fn plot_skia_cache() {
    tracy_plot!("font_cache_limit", font_cache_limit() as f64);
    tracy_plot!("font_cache_used", font_cache_used() as f64);
    tracy_plot!("font_cache_count_used", font_cache_count_used() as f64);
    tracy_plot!("font_cache_count_limit", font_cache_count_limit() as f64);
    tracy_plot!(
        "resource_cache_total_bytes_used",
        resource_cache_total_bytes_used() as f64
    );
    tracy_plot!(
        "resource_cache_total_bytes_limit",
        resource_cache_total_bytes_limit() as f64
    );
    tracy_plot!(
        "resource_cache_single_allocation_byte_limit",
        resource_cache_single_allocation_byte_limit().unwrap_or_default() as f64
    );
}

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
    text_gamma: f32,
    text_contrast: f32,
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
            text_gamma: 0.0,
            text_contrast: 0.5,
        }
    }
}

// Since draw commmands are inserted into a heap, we need to implement Ord such that
// the commands that should be processed first (such as window draw commands or close
// window) are sorted as larger than the ones that should be handled later
// So the order of the variants here matters so that the derive implementation can get
// the order in the binary heap correct
#[derive(Clone, Debug, PartialEq)]
pub enum DrawCommand {
    UpdateCursor(Cursor),
    FontChanged(String),
    LineSpaceChanged(f32),
    DefaultStyleChanged(Style),
    ModeChanged(EditorMode),
    UIReady,
    Window {
        grid_id: u64,
        command: WindowDrawCommand,
    },
    CloseWindow(u64),
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
    pub fn new(os_scale_factor: f64, init_font_settings: Option<FontSettings>) -> Self {
        let window_settings = SETTINGS.get::<WindowSettings>();

        let user_scale_factor = window_settings.scale_factor.into();
        let scale_factor = user_scale_factor * os_scale_factor;
        let cursor_renderer = CursorRenderer::new();
        let mut grid_renderer = GridRenderer::new(scale_factor);
        grid_renderer.update_font_options(init_font_settings.map(|x| x.into()).unwrap_or_default());
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
        let grid_scale = self.grid_renderer.grid_scale;

        let transparency = { SETTINGS.get::<WindowSettings>().transparency };
        root_canvas.clear(default_background.with_a((255.0 * transparency) as u8));
        root_canvas.save();
        root_canvas.reset_matrix();

        if let Some(root_window) = self.rendered_windows.get(&1) {
            let clip_rect = to_skia_rect(&root_window.pixel_region(grid_scale));
            root_canvas.clip_rect(clip_rect, None, Some(false));
        }

        let (root_windows, floating_layers) = {
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

            let mut floating_layers = vec![];

            let mut base_zindex = 0;
            let mut last_zindex = 0;
            let mut current_windows = vec![];

            for window in floating_windows {
                let zindex = window.anchor_info.as_ref().unwrap().sort_order;
                log::debug!("zindex: {}, base: {}", zindex, base_zindex);
                // Group floating windows by consecutive z indices
                if zindex - last_zindex > 1 && !current_windows.is_empty() {
                    for windows in group_windows(current_windows, grid_scale) {
                        floating_layers.push(FloatingLayer { windows });
                    }
                    current_windows = vec![];
                }

                if current_windows.is_empty() {
                    base_zindex = zindex;
                }
                current_windows.push(window);
                last_zindex = zindex;
            }

            if !current_windows.is_empty() {
                for windows in group_windows(current_windows, grid_scale) {
                    floating_layers.push(FloatingLayer { windows });
                }
            }

            for layer in &mut floating_layers {
                layer.windows.sort_by(floating_sort);
                log::debug!(
                    "layer: {:?}",
                    layer
                        .windows
                        .iter()
                        .map(|w| (w.id, w.anchor_info.as_ref().unwrap().sort_order))
                        .collect_vec()
                );
            }

            (root_windows, floating_layers)
        };

        let settings = SETTINGS.get::<RendererSettings>();
        let root_window_regions = root_windows
            .into_iter()
            .map(|window| {
                window.draw(
                    root_canvas,
                    &settings,
                    default_background.with_a((255.0 * transparency) as u8),
                    grid_scale,
                )
            })
            .collect_vec();

        let floating_window_regions = floating_layers
            .into_iter()
            .flat_map(|mut layer| {
                layer.draw(
                    root_canvas,
                    &settings,
                    default_background.with_a((255.0 * transparency) as u8),
                    grid_scale,
                )
            })
            .collect_vec();

        self.window_regions = root_window_regions
            .into_iter()
            .chain(floating_window_regions)
            .collect();
        self.cursor_renderer
            .draw(&mut self.grid_renderer, root_canvas);

        self.profiler.draw(root_canvas, dt);

        root_canvas.restore();

        #[cfg(feature = "profiling")]
        plot_skia_cache();
    }

    pub fn animate_frame(&mut self, grid_rect: &GridRect<f32>, dt: f32) -> bool {
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
            acc | window.animate(&settings, grid_rect, dt)
        });

        let windows = &self.rendered_windows;
        let grid_scale = self.grid_renderer.grid_scale;
        self.cursor_renderer
            .update_cursor_destination(grid_scale, windows);

        animating |= self
            .cursor_renderer
            .animate(&self.current_mode, &self.grid_renderer, dt);

        animating
    }

    pub fn handle_config_changed(&mut self, config: HotReloadConfigs) {
        match config {
            HotReloadConfigs::Font(font) => match font {
                Some(font) => {
                    self.grid_renderer.update_font_options(font.into());
                }
                None => {
                    self.grid_renderer
                        .update_font_options(FontOptions::default());
                }
            },
        }
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

    pub fn prepare_lines(&mut self, force: bool) {
        self.rendered_windows
            .iter_mut()
            .for_each(|(_, w)| w.prepare_lines(&mut self.grid_renderer, force));
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
                    Entry::Vacant(vacant_entry) => match command {
                        WindowDrawCommand::Position {
                            grid_position,
                            grid_size,
                            ..
                        } => {
                            let grid_position = GridPos::from(grid_position).try_cast().unwrap();
                            let grid_size = GridSize::from(grid_size).try_cast().unwrap();
                            let new_window = RenderedWindow::new(grid_id, grid_position, grid_size);
                            vacant_entry.insert(new_window);
                        }
                        WindowDrawCommand::ViewportMargins { .. } => {
                            warn!("ViewportMargins recieved before window was initialized");
                        }
                        _ => {
                            error!(
                                "WindowDrawCommand: {:?} sent for uninitialized grid {}",
                                command, grid_id
                            );
                        }
                    },
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

    pub fn get_cursor_destination(&self) -> PixelPos<f32> {
        self.cursor_renderer.get_destination()
    }

    pub fn get_grid_size(&self) -> GridSize<u32> {
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

pub enum WindowConfigType {
    OpenGL(glutin::config::Config),
    #[cfg(target_os = "windows")]
    Direct3D,
}

pub struct WindowConfig {
    pub window: Window,
    pub config: WindowConfigType,
}

pub fn build_window_config<TE>(
    winit_window_builder: WindowBuilder,
    event_loop: &EventLoop<TE>,
) -> WindowConfig {
    #[cfg(target_os = "windows")]
    {
        let cmd_line_settings = SETTINGS.get::<CmdLineSettings>();
        if cmd_line_settings.opengl {
            opengl::build_window(winit_window_builder, event_loop)
        } else {
            let window = winit_window_builder.build(event_loop).unwrap();
            let config = WindowConfigType::Direct3D;
            WindowConfig { window, config }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        opengl::build_window(winit_window_builder, event_loop)
    }
}

pub trait SkiaRenderer {
    fn window(&self) -> &Window;
    fn flush(&mut self);
    fn swap_buffers(&mut self);
    fn canvas(&mut self) -> &Canvas;
    fn resize(&mut self);
    fn create_vsync(&self, proxy: EventLoopProxy<UserEvent>) -> VSync;
    #[cfg(feature = "gpu_profiling")]
    fn tracy_create_gpu_context(&self, name: &str) -> Box<dyn GpuCtx>;
}

pub fn create_skia_renderer(
    window: WindowConfig,
    srgb: bool,
    vsync: bool,
) -> Box<dyn SkiaRenderer> {
    let renderer: Box<dyn SkiaRenderer> = match &window.config {
        WindowConfigType::OpenGL(..) => {
            Box::new(opengl::OpenGLSkiaRenderer::new(window, srgb, vsync))
        }
        #[cfg(target_os = "windows")]
        WindowConfigType::Direct3D => Box::new(d3d::D3DSkiaRenderer::new(window.window)),
    };
    tracy_create_gpu_context("main_render_context", renderer.as_ref());
    renderer
}
