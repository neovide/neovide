use super::{
    KeyboardManager, KeyboardSettings, MouseManager, SkiaRenderer, UserEvent, WindowCommand,
    WindowSettings,
};

use crate::{
    bridge::{ParallelCommand, UiCommand},
    dimensions::Dimensions,
    editor::EditorCommand,
    event_aggregator::EVENT_AGGREGATOR,
    profiling::{emit_frame_mark, tracy_gpu_collect, tracy_gpu_zone, tracy_zone},
    renderer::{build_context, GlWindow, Renderer, VSync, WindowedContext},
    running_tracker::RUNNING_TRACKER,
    settings::{DEFAULT_WINDOW_GEOMETRY, SETTINGS},
    window::{load_last_window_settings, PersistentWindowSettings},
    CmdLineSettings,
};

use log::trace;
use skia_safe::{scalar, Rect};
use tokio::sync::mpsc::UnboundedReceiver;
use winit::{
    dpi::{PhysicalPosition, PhysicalSize, Position},
    event::{Event, WindowEvent},
    window::{Fullscreen, Theme},
};

const MIN_WINDOW_WIDTH: u64 = 20;
const MIN_WINDOW_HEIGHT: u64 = 6;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct WindowPadding {
    pub top: u32,
    pub left: u32,
    pub right: u32,
    pub bottom: u32,
}

pub fn set_background(background: &str) {
    EVENT_AGGREGATOR.send(UiCommand::Parallel(ParallelCommand::SetBackground(
        background.to_string(),
    )));
}

#[derive(PartialEq)]
enum UIState {
    Initing,    // Running init.vim/lua
    Ready,      // No pending resizes
}

pub struct WinitWindowWrapper {
    // Don't rearrange this, unless you have a good reason to do so
    // The destruction order has to be correct
    renderer: Renderer,
    skia_renderer: SkiaRenderer,
    pub windowed_context: WindowedContext,
    keyboard_manager: KeyboardManager,
    mouse_manager: MouseManager,
    title: String,
    fullscreen: bool,
    font_changed_last_frame: bool,
    saved_inner_size: PhysicalSize<u32>,
    saved_grid_size: Dimensions,
    window_command_receiver: UnboundedReceiver<WindowCommand>,
    ime_enabled: bool,
    ime_position: PhysicalPosition<i32>,
    requested_columns: Option<u64>,
    requested_lines: Option<u64>,
    ui_state: UIState,
    window_padding: WindowPadding,
}

impl WinitWindowWrapper {
    pub fn new(window: GlWindow) -> Self {
        let cmd_line_settings = SETTINGS.get::<CmdLineSettings>();
        let windowed_context = build_context(window, &cmd_line_settings);
        let window = windowed_context.window();

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

        let mut wrapper = WinitWindowWrapper {
            windowed_context,
            skia_renderer,
            renderer,
            keyboard_manager: KeyboardManager::new(),
            mouse_manager: MouseManager::new(),
            title: String::from("Neovide"),
            fullscreen: false,
            font_changed_last_frame: false,
            saved_inner_size,
            saved_grid_size: DEFAULT_WINDOW_GEOMETRY,
            window_command_receiver,
            ime_enabled,
            ime_position: PhysicalPosition::new(-1, -1),
            requested_columns: None,
            requested_lines: None,
            ui_state: UIState::Initing,
            window_padding: WindowPadding {
                left: 0,
                right: 0,
                top: 0,
                bottom: 0,
            },
        };

        wrapper.set_ime(ime_enabled);
        wrapper
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
                WindowCommand::Columns(columns) => {
                    log::info!("Requested columns {columns}");
                    self.requested_columns = Some(columns);
                }
                WindowCommand::Lines(lines) => {
                    log::info!("Requested lines {lines}");
                    self.requested_lines = Some(lines);
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
    }

    /// Handles an event from winit and returns an boolean indicating if
    /// the window should be rendered.
    pub fn handle_event(&mut self, event: Event<UserEvent>) -> bool {
        tracy_zone!("handle_event", 0);
        let mut should_render = false;
        self.keyboard_manager.handle_event(&event);
        self.mouse_manager.handle_event(
            &event,
            &self.keyboard_manager,
            &self.renderer,
            self.windowed_context.window(),
        );
        self.renderer.handle_event(&event);
        match event {
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
                    should_render = true;
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
            _ => {}
        }
        should_render
    }

    pub fn draw_frame(&mut self, vsync: &mut VSync, dt: f32) {
        tracy_zone!("draw_frame");
        self.renderer.prepare_lines();
        self.renderer.draw_frame(self.skia_renderer.canvas(), dt);
        {
            tracy_gpu_zone!("skia flush");
            self.skia_renderer.gr_context.flush_and_submit();
        }
        {
            tracy_gpu_zone!("wait for vsync");
            vsync.wait_for_vsync();
        }
        {
            tracy_gpu_zone!("swap buffers");
            self.windowed_context.window().pre_present_notify();
            self.windowed_context.swap_buffers().unwrap();
        }
        emit_frame_mark();
        tracy_gpu_collect();
    }

    pub fn animate_frame(&mut self, dt: f32) -> bool {
        tracy_zone!("animate_frame", 0);

        self.renderer.animate_frame(
            &self.get_grid_size_from_window(0, 0),
            &self.padding_as_grid(),
            dt,
        )
    }

    /// Prepares a frame to render.
    /// Returns a boolean indicating whether the frame should get
    /// drawn to the screen.
    pub fn prepare_frame(&mut self) -> bool {
        tracy_zone!("prepare_frame", 0);
        let mut should_render = false;

        let window_settings = SETTINGS.get::<WindowSettings>();
        let window_padding = WindowPadding {
            top: window_settings.padding_top,
            left: window_settings.padding_left,
            right: window_settings.padding_right,
            bottom: window_settings.padding_bottom,
        };
        let padding_changed = window_padding != self.window_padding;

        let resize_requested = self.requested_columns.is_some() || self.requested_lines.is_some();

        let handle_draw_commands_result = self.renderer.handle_draw_commands();

        self.font_changed_last_frame |= handle_draw_commands_result.font_changed;
        should_render |= handle_draw_commands_result.any_handled;

        if self.ui_state == UIState::Initing && handle_draw_commands_result.should_show {
            self.ui_state = UIState::Ready;
            should_render = true;

            self.windowed_context.window().set_visible(true);
            if SETTINGS.get::<CmdLineSettings>().maximized
                || matches!(
                    load_last_window_settings().ok(),
                    Some(PersistentWindowSettings::Maximized)
                )
            {
                self.windowed_context.window().set_maximized(true);
            }
            // Ensure that the window has the correct IME state
            self.set_ime(self.ime_enabled);
        }

        // Don't render until the the UI is fully entered and the window is shown
        if self.ui_state != UIState::Ready {
            return false;
        }

        if resize_requested {
            // Resize requests (columns/lines) have priority over normal window sizing.
            // So, deal with them first and resize the window programmatically.
            // The new window size will then be processed in the following frame.
            self.update_window_size_from_grid(&window_padding);
        } else {
            let new_size = self.windowed_context.window().inner_size();
            if self.saved_inner_size != new_size || self.font_changed_last_frame || padding_changed
            {
                self.window_padding = window_padding;
                self.font_changed_last_frame = false;
                self.saved_inner_size = new_size;

                self.update_grid_size_from_window();
                self.skia_renderer.resize(&self.windowed_context);
                should_render = true;
            }
        }

        self.update_ime_position();

        should_render
    }

    fn update_window_size_from_grid(&mut self, window_padding: &WindowPadding) {
        let window = self.windowed_context.window();

        let window_padding_width = window_padding.left + window_padding.right;
        let window_padding_height = window_padding.top + window_padding.bottom;

        let geometry = Dimensions {
            width: self
                .requested_columns
                .take()
                .unwrap_or(self.saved_grid_size.width),
            height: self
                .requested_lines
                .take()
                .unwrap_or(self.saved_grid_size.height),
        };

        let mut new_size = self
            .renderer
            .grid_renderer
            .convert_grid_to_physical(geometry);
        new_size.width += window_padding_width;
        new_size.height += window_padding_height;
        let _ = window.request_inner_size(new_size);
    }

    fn get_grid_size_from_window(&self, min_width: u64, min_height: u64) -> Dimensions {
        let window_padding = self.window_padding;
        let window_padding_width = window_padding.left + window_padding.right;
        let window_padding_height = window_padding.top + window_padding.bottom;

        let content_size = PhysicalSize {
            width: self.saved_inner_size.width - window_padding_width,
            height: self.saved_inner_size.height - window_padding_height,
        };

        let grid_size = self
            .renderer
            .grid_renderer
            .convert_physical_to_grid(content_size);

        Dimensions {
            width: grid_size.width.max(min_width),
            height: grid_size.height.max(min_height),
        }
    }

    fn update_grid_size_from_window(&mut self) {
        let grid_size = self.get_grid_size_from_window(MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT);

        if self.saved_grid_size == grid_size {
            trace!("Grid matched saved size, skip update.");
            return;
        }
        self.saved_grid_size = grid_size;
        EVENT_AGGREGATOR.send(UiCommand::Parallel(ParallelCommand::Resize {
            width: grid_size.width,
            height: grid_size.height,
        }));
    }

    fn update_ime_position(&mut self) {
        let font_dimensions = self.renderer.grid_renderer.font_dimensions;
        let cursor_position = self.renderer.get_cursor_position();
        let position = PhysicalPosition::new(
            cursor_position.x.round() as i32,
            cursor_position.y.round() as i32 + font_dimensions.height as i32,
        );
        if position != self.ime_position {
            self.ime_position = position;
            self.windowed_context.window().set_ime_cursor_area(
                Position::Physical(position),
                PhysicalSize::new(100, font_dimensions.height as u32),
            );
        }
    }

    fn handle_scale_factor_update(&mut self, scale_factor: f64) {
        self.renderer.handle_os_scale_factor_change(scale_factor);
        EVENT_AGGREGATOR.send(EditorCommand::RedrawScreen);
    }

    fn padding_as_grid(&self) -> Rect {
        let font_dimensions = self.renderer.grid_renderer.font_dimensions;
        Rect {
            left: self.window_padding.left as scalar / font_dimensions.width as scalar,
            right: self.window_padding.right as scalar / font_dimensions.width as scalar,
            top: self.window_padding.top as scalar / font_dimensions.height as scalar,
            bottom: self.window_padding.bottom as scalar / font_dimensions.height as scalar,
        }
    }
}
