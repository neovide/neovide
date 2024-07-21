pub mod animation_utils;
pub mod cursor_renderer;
pub mod fonts;
pub mod grid_renderer;
pub mod profiler;
mod rendered_layer;
mod rendered_window;
mod vsync;

use std::{
    cmp::Ordering,
    collections::{hash_map::Entry, HashMap},
    sync::Arc,
};

use futures::executor::block_on;

use itertools::Itertools;
use log::{error, warn};
use palette::{LinSrgba, WithAlpha};

use winit::{event::WindowEvent, window::Window};

use vide::{Layer, Scene, WinitRenderer};

use crate::{
    bridge::EditorMode,
    cmd_line::CmdLineSettings,
    editor::{Cursor, Style},
    profiling::{tracy_named_frame, tracy_zone},
    renderer::rendered_layer::{group_windows, FloatingLayer},
    settings::*,
    units::{GridPos, GridRect, GridSize, PixelPos},
    window::ShouldRender,
    WindowSettings,
};

#[cfg(feature = "profiling")]
use crate::profiling::tracy_plot;

use cursor_renderer::CursorRenderer;
pub use grid_renderer::GridRenderer;
pub use rendered_window::{LineFragment, RenderedWindow, WindowDrawCommand, WindowDrawDetails};

pub use vsync::VSync;

use self::fonts::font_options::FontOptions;

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
    wgpu_renderer: Option<WinitRenderer>,
    cursor_renderer: CursorRenderer,
    pub grid_renderer: GridRenderer,
    current_mode: EditorMode,

    rendered_windows: HashMap<u64, RenderedWindow>,
    pub window_regions: Vec<WindowDrawDetails>,

    profiler: profiler::Profiler,
    pub os_scale_factor: f64,
    pub user_scale_factor: f64,

    scene: Scene,
}

async fn create_renderer(window: Arc<Window>) -> WinitRenderer {
    WinitRenderer::new(window)
        .await
        .with_default_drawables()
        .await
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

        let scene = Scene::new();

        Renderer {
            wgpu_renderer: None,
            rendered_windows,
            cursor_renderer,
            grid_renderer,
            current_mode,
            window_regions,
            profiler,
            os_scale_factor,
            user_scale_factor,
            scene,
        }
    }

    pub fn create_wgpu(&mut self, window: Arc<Window>) {
        self.wgpu_renderer = Some(block_on(create_renderer(window)));
    }

    pub fn handle_event(&mut self, event: &WindowEvent) {
        if let Some(wgpu_renderer) = self.wgpu_renderer.as_mut() {
            if let WindowEvent::Resized(new_size) = event {
                wgpu_renderer.resize(new_size.width, new_size.height);
            }
        }
        self.cursor_renderer.handle_event(event)
    }

    pub fn font_names(&self) -> Vec<String> {
        self.grid_renderer.font_names()
    }

    pub fn prepare_frame(&mut self) -> ShouldRender {
        self.cursor_renderer.prepare_frame()
    }

    pub fn draw_frame(&mut self, dt: f32) {
        tracy_zone!("renderer_draw_frame");

        let default_background: LinSrgba = self.grid_renderer.get_default_background().into();
        let transparency = { SETTINGS.get::<WindowSettings>().transparency };
        let transparent_default_background = default_background.with_alpha(transparency);

        let grid_scale = self.grid_renderer.grid_scale;

        self.scene = Scene::new();

        let layer_grouping = SETTINGS
            .get::<RendererSettings>()
            .experimental_layer_grouping;

        let background_layer = Layer::new().with_background(transparent_default_background.into());
        self.scene.add_layer(background_layer);

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
                let zindex = window.anchor_info.as_ref().unwrap().sort_order.z_index;
                log::debug!("zindex: {}, base: {}", zindex, base_zindex);
                if !current_windows.is_empty() && zindex != last_zindex {
                    // Group floating windows by consecutive z indices if layer_grouping is enabled,
                    // Otherwise group all windows inside a single layer
                    if !layer_grouping || zindex - last_zindex > 1 {
                        for windows in group_windows(current_windows, grid_scale) {
                            floating_layers.push(FloatingLayer { windows });
                        }
                        current_windows = vec![];
                    }
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
                        .map(|w| (w.id, w.anchor_info.as_ref().unwrap().sort_order.clone()))
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
                    default_background.with_alpha(255.0 * transparency),
                    &mut self.grid_renderer,
                    &mut self.scene,
                )
            })
            .collect_vec();

        let floating_window_regions = floating_layers
            .into_iter()
            .flat_map(|mut layer| {
                layer.draw(
                    &settings,
                    transparent_default_background,
                    &mut self.grid_renderer,
                    &mut self.scene,
                )
            })
            .collect_vec();

        let window_regions = root_window_regions
            .into_iter()
            .chain(floating_window_regions)
            .collect_vec();

        self.window_regions = window_regions;
        self.cursor_renderer.draw(&mut self.grid_renderer);

        self.profiler.draw(dt);
        if let Some(wgpu_renderer) = self.wgpu_renderer.as_mut() {
            tracy_zone!("wgpu_renderer.draw");
            wgpu_renderer.draw(&self.scene);
        }
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
                            let settings = SETTINGS.get::<CmdLineSettings>();
                            // Ignore the errors when not using multigrid, since Neovim wrongly sends some of these
                            if !settings.no_multi_grid {
                                error!(
                                    "WindowDrawCommand: {:?} sent for uninitialized grid {}",
                                    command, grid_id
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

    pub fn exit(&mut self) {
        self.wgpu_renderer = None;
    }
}

/// Defines how floating windows are sorted.
fn floating_sort(window_a: &&mut RenderedWindow, window_b: &&mut RenderedWindow) -> Ordering {
    let orda = &window_a.anchor_info.as_ref().unwrap().sort_order;
    let ordb = &window_b.anchor_info.as_ref().unwrap().sort_order;
    orda.cmp(ordb)
}
