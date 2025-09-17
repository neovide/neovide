pub mod animation_utils;
pub mod box_drawing;
pub mod cursor_renderer;
pub mod fonts;
pub mod grid_renderer;
pub mod opengl;
pub mod profiler;
pub mod progress_bar;
mod rendered_layer;
mod rendered_window;
mod vsync;

#[cfg(target_os = "windows")]
pub mod d3d;

#[cfg(target_os = "macos")]
mod metal;

use std::{
    cmp::Ordering,
    collections::{hash_map::Entry, HashMap},
    rc::Rc,
    sync::Arc,
};

use itertools::Itertools;
use log::error;
use progress_bar::{ProgressBar, ProgressBarSettings};
use skia_safe::Canvas;

use winit::{
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoopProxy},
    window::{Window, WindowAttributes},
};

use crate::{
    bridge::EditorMode,
    cmd_line::CmdLineSettings,
    editor::{Cursor, Style, WindowType},
    profiling::{tracy_create_gpu_context, tracy_named_frame, tracy_zone},
    renderer::rendered_layer::{group_windows, FloatingLayer},
    settings::*,
    units::{to_skia_rect, GridRect, GridSize, PixelPos},
    window::{EventPayload, ShouldRender},
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

use cursor_renderer::CursorRenderer;
pub use fonts::caching_shaper::CachingShaper;
pub use grid_renderer::GridRenderer;
pub use rendered_window::{RenderedWindow, WindowDrawCommand, WindowDrawDetails};

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
    floating_corner_radius: f32,
    light_angle_degrees: f32,
    light_radius: f32,
    debug_renderer: bool,
    profiler: bool,
    underline_stroke_scale: f32,
    text_gamma: f32,
    text_contrast: f32,
    experimental_layer_grouping: bool,
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
            floating_corner_radius: 0.0,
            light_angle_degrees: 45.,
            light_radius: 5.,
            debug_renderer: false,
            profiler: false,
            underline_stroke_scale: 1.,
            text_gamma: 0.0,
            text_contrast: 0.5,
            experimental_layer_grouping: false,
        }
    }
}

// Since draw commmands are inserted into a heap, we need to implement Ord such that
// the commands that should be processed first (such as window draw commands or close
// window) are sorted as larger than the ones that should be handled later
// So the order of the variants here matters so that the derive implementation can get
// the order in the binary heap correct
#[derive(Debug, Clone, PartialEq)]
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
}

pub struct Renderer {
    cursor_renderer: CursorRenderer,
    pub grid_renderer: GridRenderer,
    current_mode: EditorMode,

    pub progress_bar: ProgressBar,

    rendered_windows: HashMap<u64, RenderedWindow>,
    pub window_regions: Vec<WindowDrawDetails>,

    profiler: profiler::Profiler,
    pub os_scale_factor: f64,
    pub user_scale_factor: f64,

    settings: Arc<Settings>,
}

/// Results of processing the draw commands from the command channel.
pub struct DrawCommandResult {
    pub font_changed: bool,
    pub should_show: bool,
}

impl Renderer {
    pub fn new(os_scale_factor: f64, init_config: Config, settings: Arc<Settings>) -> Self {
        let window_settings = settings.get::<WindowSettings>();

        let user_scale_factor = window_settings.scale_factor.into();
        let scale_factor = user_scale_factor * os_scale_factor;
        let cursor_renderer = CursorRenderer::new(settings.clone());
        let mut grid_renderer = GridRenderer::new(scale_factor, settings.clone());
        grid_renderer.update_font_options(init_config.font.map(|x| x.into()).unwrap_or_default());
        grid_renderer.handle_box_drawing_update(init_config.box_drawing.unwrap_or_default());
        let current_mode = EditorMode::Unknown(String::from(""));

        let rendered_windows = HashMap::new();
        let window_regions = Vec::new();

        let profiler = profiler::Profiler::new(12.0, settings.clone());

        let progress_bar = ProgressBar::new();

        Renderer {
            rendered_windows,
            cursor_renderer,
            grid_renderer,
            current_mode,
            window_regions,
            profiler,
            progress_bar,
            os_scale_factor,
            user_scale_factor,
            settings,
        }
    }

    pub fn handle_event(&mut self, event: &WindowEvent) {
        self.cursor_renderer.handle_event(event);
    }

    pub fn font_names(&self) -> Vec<String> {
        self.grid_renderer.font_names()
    }

    pub fn prepare_frame(&mut self) -> ShouldRender {
        self.cursor_renderer.prepare_frame()
    }

    pub fn draw_frame(&mut self, root_canvas: &Canvas, dt: f32) {
        tracy_zone!("renderer_draw_frame");
        let window_settings = self.settings.get::<WindowSettings>();
        let opacity = if window_settings.normal_opacity < 1.0 {
            window_settings.normal_opacity
        } else {
            window_settings.opacity
        };
        let default_background = self.grid_renderer.get_default_background(opacity);
        let grid_scale = self.grid_renderer.grid_scale;

        let layer_grouping = self
            .settings
            .get::<RendererSettings>()
            .experimental_layer_grouping;
        root_canvas.clear(default_background);
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

            let mut prev_is_message = false;
            for window in floating_windows {
                let zindex = window.anchor_info.as_ref().unwrap().sort_order.z_index;
                log::debug!("zindex: {zindex}, base: {base_zindex}");
                let is_message = matches!(window.window_type, WindowType::Message { .. });
                // NOTE: The message window is always on it's own layer
                if !current_windows.is_empty() && zindex != last_zindex
                    || is_message
                    || prev_is_message
                {
                    // Group floating windows by consecutive z indices if layer_grouping is enabled,
                    // Otherwise group all windows inside a single layer

                    if !layer_grouping || zindex - last_zindex > 1 {
                        for windows in group_windows(current_windows, grid_scale) {
                            floating_layers.push(FloatingLayer { windows });
                        }
                        current_windows = vec![];
                    }
                }
                prev_is_message = is_message;

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
                        .map(|w| (w.id, w.anchor_info.as_ref().unwrap().sort_order.clone()))
                        .collect_vec()
                );
            }

            (root_windows, floating_layers)
        };

        let settings = self.settings.get::<RendererSettings>();
        let root_window_regions = root_windows
            .into_iter()
            .map(|window| window.draw(root_canvas, default_background, grid_scale))
            .collect_vec();

        let floating_window_regions = floating_layers
            .into_iter()
            .flat_map(|mut layer| {
                layer.draw(root_canvas, &settings, default_background, grid_scale)
            })
            .collect_vec();

        self.window_regions = root_window_regions
            .into_iter()
            .chain(floating_window_regions)
            .collect();
        self.cursor_renderer
            .draw(&mut self.grid_renderer, root_canvas);

        self.profiler.draw(root_canvas, dt);

        let grid_size = self.get_grid_size();

        root_canvas.restore();

        let progress_bar_settings = self.settings.get::<ProgressBarSettings>();
        self.progress_bar.draw(
            &progress_bar_settings,
            root_canvas,
            &self.grid_renderer,
            grid_size,
        );

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

        let settings = self.settings.get::<RendererSettings>();
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

        let progress_bar_settings = self.settings.get::<ProgressBarSettings>();
        self.progress_bar.animate(&progress_bar_settings, dt);
        animating |= self.progress_bar.is_animating();

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
            HotReloadConfigs::BoxDrawing(settings) => self
                .grid_renderer
                .handle_box_drawing_update(settings.unwrap_or_default()),
        }
    }

    pub fn handle_draw_commands(&mut self, batch: Vec<DrawCommand>) -> DrawCommandResult {
        let settings = self.settings.get::<RendererSettings>();
        let mut result = DrawCommandResult {
            font_changed: false,
            should_show: false,
        };

        for draw_command in batch {
            self.handle_draw_command(draw_command, &mut result);
            tracy_named_frame!("neovim draw batch processed");
        }
        self.flush(&settings);

        result
    }

    pub fn handle_os_scale_factor_change(&mut self, os_scale_factor: f64) {
        self.os_scale_factor = os_scale_factor;
        self.grid_renderer
            .handle_scale_factor_update(self.os_scale_factor * self.user_scale_factor);
    }

    pub fn prepare_lines(&mut self, force: bool) {
        let opacity = self.settings.get::<WindowSettings>().opacity;
        self.rendered_windows
            .iter_mut()
            .for_each(|(_, w)| w.prepare_lines(&mut self.grid_renderer, opacity, force));
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
                        WindowDrawCommand::Position { .. }
                        | WindowDrawCommand::ViewportMargins { .. } => {
                            let mut new_window = RenderedWindow::new(grid_id);
                            new_window.handle_window_draw_command(command);
                            vacant_entry.insert(new_window);
                        }
                        _ => {
                            let settings = self.settings.get::<CmdLineSettings>();
                            // Ignore the errors when not using multigrid, since Neovim wrongly sends some of these
                            if !settings.no_multi_grid {
                                error!(
                                    "WindowDrawCommand: {command:?} sent for uninitialized grid {grid_id}"
                                );
                            }
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
    let orda = &window_a.anchor_info.as_ref().unwrap().sort_order;
    let ordb = &window_b.anchor_info.as_ref().unwrap().sort_order;
    orda.cmp(ordb)
}

#[derive(Clone)]
pub enum WindowConfigType {
    OpenGL(glutin::config::Config),
    #[cfg(target_os = "windows")]
    Direct3D,
    #[cfg(target_os = "macos")]
    Metal,
}

#[derive(Clone)]
pub struct WindowConfig {
    pub window: Rc<Window>,
    pub config: WindowConfigType,
}

#[cfg(target_os = "macos")]
pub fn build_window_config(
    window_attributes: WindowAttributes,
    event_loop: &ActiveEventLoop,
    settings: &Settings,
) -> WindowConfig {
    let cmd_line_settings = settings.get::<CmdLineSettings>();
    if cmd_line_settings.opengl {
        opengl::build_window(window_attributes, event_loop)
    } else {
        let window = event_loop.create_window(window_attributes).unwrap();
        let config = WindowConfigType::Metal;
        WindowConfig {
            window: window.into(),
            config,
        }
    }
}

#[cfg(target_os = "windows")]
pub fn build_window_config(
    window_attributes: WindowAttributes,
    event_loop: &ActiveEventLoop,
    settings: &Settings,
) -> WindowConfig {
    let cmd_line_settings = settings.get::<CmdLineSettings>();
    if cmd_line_settings.opengl {
        opengl::build_window(window_attributes, event_loop)
    } else {
        let window = event_loop.create_window(window_attributes).unwrap();
        let config = WindowConfigType::Direct3D;
        WindowConfig {
            window: window.into(),
            config,
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn build_window_config(
    window_attributes: WindowAttributes,
    event_loop: &ActiveEventLoop,
    _settings: &Settings,
) -> WindowConfig {
    opengl::build_window(window_attributes, event_loop)
}

pub trait SkiaRenderer {
    fn window(&self) -> Rc<Window>;
    fn flush(&mut self);
    fn swap_buffers(&mut self);
    fn canvas(&mut self) -> &Canvas;
    fn resize(&mut self);
    fn create_vsync(&self, proxy: EventLoopProxy<EventPayload>) -> VSync;
    #[cfg(feature = "gpu_profiling")]
    fn tracy_create_gpu_context(&self, name: &str) -> Box<dyn GpuCtx>;
}

pub fn create_skia_renderer(
    window: &WindowConfig,
    srgb: bool,
    vsync: bool,
    settings: Arc<Settings>,
) -> Box<dyn SkiaRenderer> {
    let renderer: Box<dyn SkiaRenderer> = match &window.config {
        WindowConfigType::OpenGL(..) => Box::new(opengl::OpenGLSkiaRenderer::new(
            window.clone(),
            srgb,
            vsync,
            settings.clone(),
        )),
        #[cfg(target_os = "windows")]
        WindowConfigType::Direct3D => Box::new(d3d::D3DSkiaRenderer::new(
            window.window.clone(),
            settings.clone(),
        )),
        #[cfg(target_os = "macos")]
        WindowConfigType::Metal => Box::new(metal::MetalSkiaRenderer::new(
            window.window.clone(),
            srgb,
            vsync,
            settings.clone(),
        )),
    };
    tracy_create_gpu_context("main_render_context", renderer.as_ref());
    renderer
}
