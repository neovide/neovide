use super::{
    KeyboardManager, MouseManager, SkiaRenderer, UserEvent, WindowCommand, WindowSettings,
    WindowSettingsChanged,
};

#[cfg(windows)]
use crate::windows_utils::{register_right_click, unregister_right_click};
use crate::{
    bridge::{send_ui, ParallelCommand, SerialCommand},
    dimensions::Dimensions,
    profiling::{tracy_frame, tracy_gpu_collect, tracy_gpu_zone, tracy_plot, tracy_zone},
    renderer::{build_context, DrawCommand, GlWindow, Renderer, VSync, WindowedContext},
    running_tracker::RUNNING_TRACKER,
    settings::{SettingsChanged, DEFAULT_GRID_SIZE, MIN_GRID_SIZE, SETTINGS},
    window::{ShouldRender, WindowSize},
    CmdLineSettings,
};

use log::trace;
use skia_safe::{scalar, Rect};
use winit::{
    dpi::{PhysicalPosition, PhysicalSize, Position},
    event::{Event, WindowEvent},
    event_loop::EventLoopProxy,
    window::{Fullscreen, Theme},
};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct WindowPadding {
    pub top: u32,
    pub left: u32,
    pub right: u32,
    pub bottom: u32,
}

pub fn set_background(background: &str) {
    send_ui(ParallelCommand::SetBackground(background.to_string()));
}

#[derive(PartialEq)]
enum UIState {
    Initing, // Running init.vim/lua
    FirstFrame,
    Showing, // No pending resizes
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
    saved_grid_size: Option<Dimensions>,
    ime_enabled: bool,
    ime_position: PhysicalPosition<i32>,
    requested_columns: Option<u64>,
    requested_lines: Option<u64>,
    ui_state: UIState,
    window_padding: WindowPadding,
    initial_window_size: WindowSize,
    is_minimized: bool,
    pub vsync: VSync,
}

impl WinitWindowWrapper {
    pub fn new(
        window: GlWindow,
        initial_window_size: WindowSize,
        proxy: EventLoopProxy<UserEvent>,
    ) -> Self {
        let cmd_line_settings = SETTINGS.get::<CmdLineSettings>();
        let srgb = cmd_line_settings.srgb;
        let vsync_enabled = cmd_line_settings.vsync;
        let windowed_context = build_context(window, srgb, vsync_enabled);
        let window = windowed_context.window();

        let scale_factor = windowed_context.window().scale_factor();
        let renderer = Renderer::new(scale_factor);
        let saved_inner_size = window.inner_size();

        let skia_renderer = SkiaRenderer::new(&windowed_context);

        log::info!(
            "window created (scale_factor: {:.4}, font_dimensions: {:?})",
            scale_factor,
            renderer.grid_renderer.font_dimensions,
        );

        let settings = SETTINGS.get::<WindowSettings>();
        let ime_enabled = settings.input_ime;

        match settings.theme.as_str() {
            "light" => set_background("light"),
            "dark" => set_background("dark"),
            "auto" => match window.theme() {
                Some(Theme::Light) => set_background("light"),
                Some(Theme::Dark) => set_background("dark"),
                None => {}
            },
            _ => {}
        }

        let vsync = VSync::new(vsync_enabled, &windowed_context, proxy);

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
            saved_grid_size: None,
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
            initial_window_size,
            is_minimized: false,
            vsync,
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

    pub fn minimize_window(&mut self) {
        let window = self.windowed_context.window();

        window.set_minimized(true);
    }

    pub fn set_ime(&mut self, ime_enabled: bool) {
        self.ime_enabled = ime_enabled;
        self.windowed_context.window().set_ime_allowed(ime_enabled);
    }

    pub fn handle_window_command(&mut self, command: WindowCommand) {
        tracy_zone!("handle_window_commands", 0);
        match command {
            WindowCommand::TitleChanged(new_title) => self.handle_title_changed(new_title),
            WindowCommand::SetMouseEnabled(mouse_enabled) => {
                self.mouse_manager.enabled = mouse_enabled
            }
            WindowCommand::ListAvailableFonts => self.send_font_names(),
            WindowCommand::FocusWindow => {
                self.windowed_context.window().focus_window();
            }
            WindowCommand::Minimize => {
                self.minimize_window();
                self.is_minimized = true;
            }
            WindowCommand::ShowIntro(message) => {
                send_ui(ParallelCommand::ShowIntro { message });
            }
            #[cfg(windows)]
            WindowCommand::RegisterRightClick => register_right_click(),
            #[cfg(windows)]
            WindowCommand::UnregisterRightClick => unregister_right_click(),
        }
    }

    pub fn handle_window_settings_changed(&mut self, changed_setting: WindowSettingsChanged) {
        tracy_zone!("handle_window_settings_changed");
        match changed_setting {
            WindowSettingsChanged::ObservedColumns(columns) => {
                log::info!("columns changed");
                self.requested_columns = columns;
            }
            WindowSettingsChanged::ObservedLines(lines) => {
                log::info!("lines changed");
                self.requested_lines = lines;
            }
            WindowSettingsChanged::Fullscreen(fullscreen) => {
                if self.fullscreen != fullscreen {
                    self.toggle_fullscreen();
                }
            }
            WindowSettingsChanged::InputIme(ime_enabled) => {
                if self.ime_enabled != ime_enabled {
                    self.set_ime(ime_enabled);
                }
            }
            _ => {}
        }
    }

    pub fn handle_title_changed(&mut self, new_title: String) {
        self.title = new_title;
        self.windowed_context.window().set_title(&self.title);
    }

    pub fn send_font_names(&self) {
        let font_names = self.renderer.font_names();
        send_ui(ParallelCommand::DisplayAvailableFonts(font_names));
    }

    pub fn handle_quit(&mut self) {
        if SETTINGS.get::<CmdLineSettings>().server.is_none() {
            send_ui(ParallelCommand::Quit);
        } else {
            RUNNING_TRACKER.quit("window closed");
        }
    }

    pub fn handle_focus_lost(&mut self) {
        send_ui(ParallelCommand::FocusLost);
    }

    pub fn handle_focus_gained(&mut self) {
        send_ui(ParallelCommand::FocusGained);
        // Got focus back after being minimized previously
        if self.is_minimized {
            // Sending <NOP> after suspend triggers the `VimResume` AutoCmd
            send_ui(SerialCommand::Keyboard("<NOP>".into()));

            self.is_minimized = false;
        }
    }

    /// Handles an event from winit and returns an boolean indicating if
    /// the window should be rendered.
    pub fn handle_event(&mut self, event: Event<UserEvent>) -> bool {
        tracy_zone!("handle_event", 0);
        self.keyboard_manager.handle_event(&event);
        self.mouse_manager.handle_event(
            &event,
            &self.keyboard_manager,
            &self.renderer,
            self.windowed_context.window(),
        );
        let renderer_asks_to_be_rendered = self.renderer.handle_event(&event);
        let mut should_render = true;
        match event {
            Event::Resumed => {
                tracy_zone!("Resumed");
                // No need to do anything, but handle the event so that should_render gets set
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                tracy_zone!("CloseRequested");
                self.handle_quit();
            }
            Event::WindowEvent {
                event: WindowEvent::ScaleFactorChanged { scale_factor, .. },
                ..
            } => {
                tracy_zone!("ScaleFactorChanged");
                self.handle_scale_factor_update(scale_factor);
            }
            Event::WindowEvent {
                event: WindowEvent::DroppedFile(path),
                ..
            } => {
                tracy_zone!("DroppedFile");
                let file_path = path.into_os_string().into_string().unwrap();
                send_ui(ParallelCommand::FileDrop(file_path));
            }
            Event::WindowEvent {
                event: WindowEvent::Focused(focus),
                ..
            } => {
                tracy_zone!("Focused");
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
                tracy_zone!("ThemeChanged");
                let settings = SETTINGS.get::<WindowSettings>();
                if settings.theme.as_str() == "auto" {
                    let background = match theme {
                        Theme::Light => "light",
                        Theme::Dark => "dark",
                    };
                    set_background(background);
                }
            }
            Event::WindowEvent {
                event: WindowEvent::Moved(_),
                ..
            } => {
                tracy_zone!("Moved");
                self.vsync.update(&self.windowed_context);
            }
            Event::UserEvent(UserEvent::DrawCommandBatch(batch)) => {
                self.handle_draw_commands(batch);
            }
            Event::UserEvent(UserEvent::WindowCommand(e)) => {
                self.handle_window_command(e);
            }
            Event::UserEvent(UserEvent::SettingsChanged(SettingsChanged::Window(e))) => {
                self.handle_window_settings_changed(e);
            }
            _ => {
                match event {
                    Event::WindowEvent { .. } => {
                        tracy_zone!("Unknown WindowEvent");
                    }
                    Event::AboutToWait { .. } => {
                        tracy_zone!("AboutToWait");
                    }
                    Event::DeviceEvent { .. } => {
                        tracy_zone!("DeviceEvent");
                    }
                    Event::NewEvents(..) => {
                        tracy_zone!("NewEvents");
                    }
                    _ => {
                        tracy_zone!("Unknown");
                    }
                }
                should_render = renderer_asks_to_be_rendered;
            }
        }
        self.ui_state != UIState::Initing && should_render
    }

    pub fn draw_frame(&mut self, dt: f32) {
        tracy_zone!("draw_frame");
        self.renderer.draw_frame(self.skia_renderer.canvas(), dt);
        {
            tracy_gpu_zone!("skia flush");
            self.skia_renderer.gr_context.flush_and_submit();
        }
        {
            tracy_gpu_zone!("swap buffers");
            self.windowed_context.window().pre_present_notify();
            self.windowed_context.swap_buffers().unwrap();
        }
        {
            tracy_gpu_zone!("wait for vsync");
            self.vsync.wait_for_vsync();
        }
        tracy_frame();
        tracy_gpu_collect();
    }

    pub fn animate_frame(&mut self, dt: f32) -> bool {
        tracy_zone!("animate_frame", 0);

        let res = self.renderer.animate_frame(
            &self.get_grid_size_from_window(0, 0),
            &self.padding_as_grid(),
            dt,
        );
        tracy_plot!("animate_frame", res as u8 as f64);
        self.renderer.prepare_lines();
        #[allow(clippy::let_and_return)]
        res
    }

    fn handle_draw_commands(&mut self, batch: Vec<DrawCommand>) {
        tracy_zone!("handle_draw_commands");
        let handle_draw_commands_result = self.renderer.handle_draw_commands(batch);

        self.font_changed_last_frame |= handle_draw_commands_result.font_changed;

        if self.ui_state == UIState::Initing && handle_draw_commands_result.should_show {
            log::info!("Showing the Window");
            self.ui_state = UIState::FirstFrame;

            match self.initial_window_size {
                WindowSize::Maximized => {
                    self.windowed_context.window().set_visible(true);
                    self.windowed_context.window().set_maximized(true);
                }
                WindowSize::Grid(Dimensions { width, height }) => {
                    self.requested_columns = Some(width);
                    self.requested_lines = Some(height);
                    log::info!("Showing window {width}, {height}");
                    // The visibility is changed after the size is adjusted
                }
                WindowSize::NeovimGrid => {
                    let grid_size = self.renderer.get_grid_size();
                    self.requested_columns = Some(grid_size.width);
                    self.requested_lines = Some(grid_size.height);
                }
                WindowSize::Size(..) => {
                    self.requested_columns = None;
                    self.requested_lines = None;
                    self.windowed_context.window().set_visible(true);
                }
            }

            // Ensure that the window has the correct IME state
            self.set_ime(self.ime_enabled);
        };
    }

    pub fn prepare_frame(&mut self) -> ShouldRender {
        tracy_zone!("prepare_frame", 0);
        let mut should_render = ShouldRender::Wait;

        let window_settings = SETTINGS.get::<WindowSettings>();
        let window_padding = WindowPadding {
            top: window_settings.padding_top,
            left: window_settings.padding_left,
            right: window_settings.padding_right,
            bottom: window_settings.padding_bottom,
        };
        let padding_changed = window_padding != self.window_padding;

        // Don't render until the UI is fully entered and the window is shown
        if self.ui_state == UIState::Initing {
            return ShouldRender::Wait;
        } else if self.ui_state == UIState::FirstFrame {
            should_render = ShouldRender::Immediately;
            self.ui_state = UIState::Showing;
        }

        let resize_requested = self.requested_columns.is_some() || self.requested_lines.is_some();
        if resize_requested {
            // Resize requests (columns/lines) have priority over normal window sizing.
            // So, deal with them first and resize the window programmatically.
            // The new window size will then be processed in the following frame.
            self.update_window_size_from_grid(&window_padding);

            // Make the window Visible only after the size is adjusted
            self.windowed_context.window().set_visible(true);
        } else if self.windowed_context.window().is_minimized() != Some(true) {
            // NOTE: Only actually resize the grid when the window is not minimized
            // Some platforms return a zero size when that is the case, so we should not try to resize to that.
            let new_size = self.windowed_context.window().inner_size();
            if self.saved_inner_size != new_size || self.font_changed_last_frame || padding_changed
            {
                self.window_padding = window_padding;
                self.font_changed_last_frame = false;
                self.saved_inner_size = new_size;

                self.update_grid_size_from_window();
                self.skia_renderer.resize(&self.windowed_context);
                should_render = ShouldRender::Immediately;
            }
        }

        self.update_ime_position();

        should_render.update(self.renderer.prepare_frame());

        should_render
    }

    pub fn get_grid_size(&self) -> Dimensions {
        self.renderer.get_grid_size()
    }

    fn update_window_size_from_grid(&mut self, window_padding: &WindowPadding) {
        let window = self.windowed_context.window();

        let window_padding_width = window_padding.left + window_padding.right;
        let window_padding_height = window_padding.top + window_padding.bottom;

        let grid_size = Dimensions {
            width: self.requested_columns.take().unwrap_or(
                self.saved_grid_size
                    .map_or(DEFAULT_GRID_SIZE.width, |v| v.width),
            ),
            height: self.requested_lines.take().unwrap_or(
                self.saved_grid_size
                    .map_or(DEFAULT_GRID_SIZE.height, |v| v.height),
            ),
        };

        let mut new_size = self
            .renderer
            .grid_renderer
            .convert_grid_to_physical(grid_size);
        new_size.width += window_padding_width;
        new_size.height += window_padding_height;
        log::info!(
            "Resizing window based on grid. Grid Size: {:?}, Window Size {:?}",
            grid_size,
            new_size
        );
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
        let min_width = MIN_GRID_SIZE.width;
        let min_height = MIN_GRID_SIZE.height;
        let grid_size = self.get_grid_size_from_window(min_width, min_height);

        if self.saved_grid_size.as_ref() == Some(&grid_size) {
            trace!("Grid matched saved size, skip update.");
            return;
        }
        self.saved_grid_size = Some(grid_size);
        log::info!(
            "Resizing grid based on window size. Grid Size: {:?}, Window Size {:?}",
            grid_size,
            self.saved_inner_size
        );
        send_ui(ParallelCommand::Resize {
            width: grid_size.width,
            height: grid_size.height,
        });
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
