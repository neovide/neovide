use super::{
    KeyboardManager, KeyboardSettings, MouseManager, SkiaRenderer, WindowCommand, WindowSettings,
};

use crate::{
    bridge::{ParallelCommand, UiCommand},
    dimensions::Dimensions,
    editor::EditorCommand,
    event_aggregator::EVENT_AGGREGATOR,
    profiling::{emit_frame_mark, tracy_gpu_collect, tracy_gpu_zone, tracy_zone},
    redraw_scheduler::REDRAW_SCHEDULER,
    renderer::{build_context, Renderer, WindowPadding, WindowedContext},
    running_tracker::RUNNING_TRACKER,
    settings::{
        load_last_window_settings, PersistentWindowSettings, DEFAULT_WINDOW_GEOMETRY, SETTINGS,
    },
    CmdLineSettings,
};

use glutin::config::Config;
use log::trace;
use tokio::sync::mpsc::UnboundedReceiver;
use winit::{
    dpi::{PhysicalPosition, PhysicalSize, Position},
    event::{Event, WindowEvent},
    window::{Fullscreen, Theme, Window},
};

const MIN_WINDOW_WIDTH: u64 = 20;
const MIN_WINDOW_HEIGHT: u64 = 6;

pub struct WinitWindowWrapper {
    pub windowed_context: WindowedContext,
    skia_renderer: SkiaRenderer,
    renderer: Renderer,
    keyboard_manager: KeyboardManager,
    mouse_manager: MouseManager,
    title: String,
    fullscreen: bool,
    font_changed_last_frame: bool,
    saved_inner_size: PhysicalSize<u32>,
    saved_grid_size: Option<Dimensions>,
    size_at_startup: PhysicalSize<u32>,
    maximized_at_startup: bool,
    window_command_receiver: UnboundedReceiver<WindowCommand>,
    ime_enabled: bool,
}

pub fn set_background(background: &str) {
    EVENT_AGGREGATOR.send(UiCommand::Parallel(ParallelCommand::SetBackground(
        background.to_string(),
    )));
}

impl WinitWindowWrapper {
    pub fn new(
        window: Window,
        config: Config,
        cmd_line_settings: &CmdLineSettings,
        previous_position: Option<PhysicalPosition<i32>>,
        maximized: bool,
    ) -> Self {
        let windowed_context = build_context(window, config, cmd_line_settings);
        let window = windowed_context.window();
        let initial_size = window.inner_size();

        // Check that window is visible in some monitor, and reposition it if not.
        let did_reposition = window
            .current_monitor()
            .and_then(|current_monitor| {
                let monitor_position = current_monitor.position();
                let monitor_size = current_monitor.size();
                let monitor_width = monitor_size.width as i32;
                let monitor_height = monitor_size.height as i32;

                let window_position = previous_position
                    .filter(|_| !maximized)
                    .or_else(|| window.outer_position().ok())?;

                let window_size = window.outer_size();
                let window_width = window_size.width as i32;
                let window_height = window_size.height as i32;

                if window_position.x + window_width < monitor_position.x
                    || window_position.y + window_height < monitor_position.y
                    || window_position.x > monitor_position.x + monitor_width
                    || window_position.y > monitor_position.y + monitor_height
                {
                    window.set_outer_position(monitor_position);
                }

                Some(())
            })
            .is_some();

        log::trace!("repositioned window: {}", did_reposition);

        let scale_factor = windowed_context.window().scale_factor();
        let renderer = Renderer::new(scale_factor);
        let saved_inner_size = window.inner_size();

        let skia_renderer = SkiaRenderer::new(&windowed_context);

        let window_command_receiver = EVENT_AGGREGATOR.register_event::<WindowCommand>();

        log::info!(
            "window created (scale_factor: {:.4}, font_dimensions: {:?})",
            scale_factor,
            renderer.grid_renderer.font_dimensions,
        );

        let ime_enabled = { SETTINGS.get::<KeyboardSettings>().ime };

        match SETTINGS.get::<WindowSettings>().theme.as_str() {
            "light" => set_background("light"),
            "dark" => set_background("dark"),
            "auto" => match window.theme() {
                Some(Theme::Light) => set_background("light"),
                Some(Theme::Dark) => set_background("dark"),
                None => {}
            },
            _ => {}
        }

        let mut window_wrapper = WinitWindowWrapper {
            windowed_context,
            skia_renderer,
            renderer,
            keyboard_manager: KeyboardManager::new(),
            mouse_manager: MouseManager::new(),
            title: String::from("Neovide"),
            fullscreen: false,
            font_changed_last_frame: false,
            size_at_startup: initial_size,
            maximized_at_startup: maximized,
            saved_inner_size,
            saved_grid_size: None,
            window_command_receiver,
            ime_enabled,
        };

        window_wrapper.set_ime(ime_enabled);
        window_wrapper
    }

    pub fn toggle_fullscreen(&mut self) {
        let window = self.windowed_context.window();
        if self.fullscreen {
            window.set_fullscreen(None);
        } else {
            let handle = window.current_monitor();
            window.set_fullscreen(Some(Fullscreen::Borderless(handle)));
        }

        self.fullscreen = !self.fullscreen;
    }

    pub fn set_ime(&mut self, ime_enabled: bool) {
        self.ime_enabled = ime_enabled;
        self.windowed_context.window().set_ime_allowed(ime_enabled);
    }

    pub fn synchronize_settings(&mut self) {
        let fullscreen = { SETTINGS.get::<WindowSettings>().fullscreen };

        if self.fullscreen != fullscreen {
            self.toggle_fullscreen();
        }

        let ime_enabled = { SETTINGS.get::<KeyboardSettings>().ime };

        if self.ime_enabled != ime_enabled {
            self.set_ime(ime_enabled);
        }
    }

    #[allow(clippy::needless_collect)]
    pub fn handle_window_commands(&mut self) {
        tracy_zone!("handle_window_commands", 0);
        while let Ok(window_command) = self.window_command_receiver.try_recv() {
            match window_command {
                WindowCommand::TitleChanged(new_title) => self.handle_title_changed(new_title),
                WindowCommand::SetMouseEnabled(mouse_enabled) => {
                    self.mouse_manager.enabled = mouse_enabled
                }
                WindowCommand::ListAvailableFonts => self.send_font_names(),
                WindowCommand::FocusWindow => {
                    self.windowed_context.window().focus_window();
                }
            }
        }
    }

    pub fn handle_title_changed(&mut self, new_title: String) {
        self.title = new_title;
        self.windowed_context.window().set_title(&self.title);
    }

    pub fn send_font_names(&self) {
        let font_names = self.renderer.font_names();
        EVENT_AGGREGATOR.send(UiCommand::Parallel(ParallelCommand::DisplayAvailableFonts(
            font_names,
        )));
    }

    pub fn handle_quit(&mut self) {
        if SETTINGS.get::<CmdLineSettings>().server.is_none() {
            EVENT_AGGREGATOR.send(UiCommand::Parallel(ParallelCommand::Quit));
        } else {
            RUNNING_TRACKER.quit("window closed");
        }
    }

    pub fn handle_focus_lost(&mut self) {
        EVENT_AGGREGATOR.send(UiCommand::Parallel(ParallelCommand::FocusLost));
    }

    pub fn handle_focus_gained(&mut self) {
        EVENT_AGGREGATOR.send(UiCommand::Parallel(ParallelCommand::FocusGained));
        REDRAW_SCHEDULER.queue_next_frame();
    }

    pub fn handle_event(&mut self, event: Event<()>) {
        tracy_zone!("handle_event", 0);
        self.keyboard_manager.handle_event(&event);
        self.mouse_manager.handle_event(
            &event,
            &self.keyboard_manager,
            &self.renderer,
            self.windowed_context.window(),
        );
        self.renderer.handle_event(&event);
        match event {
            Event::LoopDestroyed => {
                self.handle_quit();
            }
            Event::Resumed => {
                EVENT_AGGREGATOR.send(EditorCommand::RedrawScreen);
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                self.handle_quit();
            }
            Event::WindowEvent {
                event: WindowEvent::ScaleFactorChanged { scale_factor, .. },
                ..
            } => {
                self.handle_scale_factor_update(scale_factor);
            }
            Event::WindowEvent {
                event: WindowEvent::DroppedFile(path),
                ..
            } => {
                let file_path = path.into_os_string().into_string().unwrap();
                EVENT_AGGREGATOR.send(UiCommand::Parallel(ParallelCommand::FileDrop(file_path)));
            }
            Event::WindowEvent {
                event: WindowEvent::Focused(focus),
                ..
            } => {
                if focus {
                    self.handle_focus_gained();
                } else {
                    self.handle_focus_lost();
                }
            }
            Event::WindowEvent {
                event: WindowEvent::ThemeChanged(theme),
                ..
            } => {
                let settings = SETTINGS.get::<WindowSettings>();
                if settings.theme.as_str() == "auto" {
                    let background = match theme {
                        Theme::Light => "light",
                        Theme::Dark => "dark",
                    };
                    set_background(background);
                }
            }
            Event::RedrawRequested(..) | Event::WindowEvent { .. } => {
                REDRAW_SCHEDULER.queue_next_frame()
            }
            _ => {}
        }
    }

    pub fn draw_frame(&mut self, dt: f32) {
        tracy_zone!("draw_frame");
        let window = self.windowed_context.window();

        let window_settings = SETTINGS.get::<WindowSettings>();
        let window_padding = WindowPadding {
            top: window_settings.padding_top,
            left: window_settings.padding_left,
            right: window_settings.padding_right,
            bottom: window_settings.padding_bottom,
        };

        let padding_changed = window_padding != self.renderer.window_padding;
        if padding_changed {
            self.renderer.window_padding = window_padding;
        }

        let new_size = window.inner_size();
        if self.saved_inner_size != new_size || self.font_changed_last_frame || padding_changed {
            self.font_changed_last_frame = false;
            self.saved_inner_size = new_size;

            self.handle_new_grid_size(new_size);
            self.skia_renderer.resize(&self.windowed_context);
        }

        if REDRAW_SCHEDULER.should_draw() || !SETTINGS.get::<WindowSettings>().idle {
            let prev_cursor_position = self.renderer.get_cursor_position();
            self.font_changed_last_frame =
                self.renderer.draw_frame(self.skia_renderer.canvas(), dt);
            {
                tracy_gpu_zone!("skia flush");
                self.skia_renderer.gr_context.flush(None);
            }
            {
                tracy_gpu_zone!("swap buffers");
                self.windowed_context.swap_buffers().unwrap();
            }
            emit_frame_mark();
            tracy_gpu_collect();
            let current_cursor_position = self.renderer.get_cursor_position();
            if current_cursor_position != prev_cursor_position {
                let font_dimensions = self.renderer.grid_renderer.font_dimensions;
                let position = PhysicalPosition::new(
                    current_cursor_position.x.round() as i32,
                    current_cursor_position.y.round() as i32 + font_dimensions.height as i32,
                );
                self.windowed_context.window().set_ime_cursor_area(
                    Position::Physical(position),
                    PhysicalSize::new(100, font_dimensions.height as u32),
                );
            }
        }

        // Wait until fonts are loaded, so we can set proper window size.
        if !self.renderer.grid_renderer.is_ready {
            return;
        }

        // Resize at startup happens when window is maximized or when using tiling WM
        // which already resized window.
        let resized_at_startup = self.maximized_at_startup || self.has_been_resized();

        log::trace!("Inner size: {:?}", new_size);

        if self.saved_grid_size.is_none() && !resized_at_startup {
            self.init_window_size();
        }
    }

    fn init_window_size(&self) {
        let settings = SETTINGS.get::<CmdLineSettings>();
        log::trace!("Settings geometry {:?}", settings.geometry,);
        log::trace!("Settings size {:?}", settings.size);

        let window = self.windowed_context.window();
        let inner_size = if let Some(size) = settings.size {
            // --size
            size.into()
        } else if let Some(geometry) = settings.geometry {
            // --geometry
            self.renderer
                .grid_renderer
                .convert_grid_to_physical(geometry)
        } else if let Ok(PersistentWindowSettings::Windowed {
            pixel_size: Some(size),
            ..
        }) = load_last_window_settings()
        {
            // remembered size
            size
        } else {
            // default geometry
            self.renderer
                .grid_renderer
                .convert_grid_to_physical(DEFAULT_WINDOW_GEOMETRY)
        };
        window.set_inner_size(inner_size);
        // next frame will detect change in window.inner_size() and hence will
        // handle_new_grid_size automatically
    }

    fn handle_new_grid_size(&mut self, new_size: PhysicalSize<u32>) {
        let window_padding = self.renderer.window_padding;
        let window_padding_width = window_padding.left + window_padding.right;
        let window_padding_height = window_padding.top + window_padding.bottom;

        let content_size = PhysicalSize {
            width: new_size.width - window_padding_width,
            height: new_size.height - window_padding_height,
        };

        let grid_size = self
            .renderer
            .grid_renderer
            .convert_physical_to_grid(content_size);

        // Have a minimum size
        if grid_size.width < MIN_WINDOW_WIDTH || grid_size.height < MIN_WINDOW_HEIGHT {
            return;
        }

        if self.saved_grid_size == Some(grid_size) {
            trace!("Grid matched saved size, skip update.");
            return;
        }
        self.saved_grid_size = Some(grid_size);
        EVENT_AGGREGATOR.send(UiCommand::Parallel(ParallelCommand::Resize {
            width: grid_size.width,
            height: grid_size.height,
        }));
    }

    fn handle_scale_factor_update(&mut self, scale_factor: f64) {
        self.renderer.handle_os_scale_factor_change(scale_factor);
        EVENT_AGGREGATOR.send(EditorCommand::RedrawScreen);
    }

    fn has_been_resized(&self) -> bool {
        self.windowed_context.window().inner_size() != self.size_at_startup
    }
}
