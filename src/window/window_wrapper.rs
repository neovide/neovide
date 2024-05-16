use super::{
    KeyboardManager, MouseManager, UserEvent, WindowCommand, WindowSettings, WindowSettingsChanged,
};

#[cfg(target_os = "macos")]
use {
    crate::{error_msg, window::settings},
    winit::platform::macos::{self, WindowExtMacOS},
};

#[cfg(windows)]
use crate::windows_utils::{register_right_click, unregister_right_click};
use crate::{
    bridge::{send_ui, ParallelCommand, SerialCommand},
    profiling::{tracy_frame, tracy_gpu_collect, tracy_gpu_zone, tracy_plot, tracy_zone},
    renderer::{
        create_skia_renderer, DrawCommand, Renderer, RendererSettingsChanged, SkiaRenderer, VSync,
        WindowConfig,
    },
    settings::{
        clamped_grid_size, FontSettings, HotReloadConfigs, SettingsChanged, DEFAULT_GRID_SIZE,
        MIN_GRID_SIZE, SETTINGS,
    },
    units::{GridPos, GridRect, GridSize, PixelPos, PixelSize},
    window::{ShouldRender, WindowSize},
    CmdLineSettings,
};

#[cfg(target_os = "macos")]
use super::macos::MacosWindowFeature;

#[cfg(target_os = "macos")]
use icrate::Foundation::MainThreadMarker;

use log::trace;
use winit::{
    dpi,
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
    pub skia_renderer: Box<dyn SkiaRenderer>,
    pub renderer: Renderer,
    keyboard_manager: KeyboardManager,
    mouse_manager: MouseManager,
    title: String,
    fullscreen: bool,
    font_changed_last_frame: bool,
    saved_inner_size: dpi::PhysicalSize<u32>,
    saved_grid_size: Option<GridSize<u32>>,
    ime_enabled: bool,
    ime_position: dpi::PhysicalPosition<i32>,
    requested_columns: Option<u32>,
    requested_lines: Option<u32>,
    ui_state: UIState,
    window_padding: WindowPadding,
    initial_window_size: WindowSize,
    is_minimized: bool,
    theme: Option<Theme>,
    pub vsync: VSync,
    #[cfg(target_os = "macos")]
    pub macos_feature: MacosWindowFeature,
}

impl WinitWindowWrapper {
    pub fn new(
        window: WindowConfig,
        initial_window_size: WindowSize,
        initial_font_settings: Option<FontSettings>,
        proxy: EventLoopProxy<UserEvent>,
    ) -> Self {
        let cmd_line_settings = SETTINGS.get::<CmdLineSettings>();
        let srgb = cmd_line_settings.srgb;
        let vsync_enabled = cmd_line_settings.vsync;
        let skia_renderer = create_skia_renderer(window, srgb, vsync_enabled);
        let window = skia_renderer.window();

        let scale_factor = skia_renderer.window().scale_factor();
        let renderer = Renderer::new(scale_factor, initial_font_settings);
        let saved_inner_size = window.inner_size();

        log::info!(
            "window created (scale_factor: {:.4}, font_dimensions: {:?})",
            scale_factor,
            renderer.grid_renderer.grid_scale.0,
        );

        let WindowSettings {
            input_ime,
            theme,
            transparency,
            window_blurred,
            ..
        } = SETTINGS.get::<WindowSettings>();

        skia_renderer
            .window()
            .set_blur(window_blurred && transparency < 1.0);

        match theme.as_str() {
            "light" => set_background("light"),
            "dark" => set_background("dark"),
            "auto" => match window.theme() {
                Some(Theme::Light) => set_background("light"),
                Some(Theme::Dark) => set_background("dark"),
                None => {}
            },
            _ => {}
        }

        let vsync = VSync::new(vsync_enabled, skia_renderer.as_ref(), proxy);

        #[cfg(target_os = "macos")]
        let macos_feature = {
            let mtm = MainThreadMarker::new().expect("must be on the main thread");
            MacosWindowFeature::from_winit_window(window, mtm)
        };

        let mut wrapper = WinitWindowWrapper {
            skia_renderer,
            renderer,
            keyboard_manager: KeyboardManager::new(),
            mouse_manager: MouseManager::new(),
            title: String::from("Neovide"),
            fullscreen: false,
            font_changed_last_frame: false,
            saved_inner_size,
            saved_grid_size: None,
            ime_enabled: input_ime,
            ime_position: dpi::PhysicalPosition::new(-1, -1),
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
            theme: None,
            vsync,
            #[cfg(target_os = "macos")]
            macos_feature,
        };

        wrapper.set_ime(input_ime);
        wrapper
    }

    pub fn toggle_fullscreen(&mut self) {
        let window = self.skia_renderer.window();
        if self.fullscreen {
            window.set_fullscreen(None);
        } else {
            let handle = window.current_monitor();
            window.set_fullscreen(Some(Fullscreen::Borderless(handle)));
        }

        self.fullscreen = !self.fullscreen;
    }

    #[cfg(target_os = "macos")]
    pub fn set_macos_option_as_meta(&mut self, option: settings::OptionAsMeta) {
        let winit_option = match option {
            settings::OptionAsMeta::OnlyLeft => macos::OptionAsAlt::OnlyLeft,
            settings::OptionAsMeta::OnlyRight => macos::OptionAsAlt::OnlyRight,
            settings::OptionAsMeta::Both => macos::OptionAsAlt::Both,
            settings::OptionAsMeta::None => macos::OptionAsAlt::None,
        };
        if winit_option != self.skia_renderer.window().option_as_alt() {
            self.skia_renderer.window().set_option_as_alt(winit_option);
        }
    }

    pub fn minimize_window(&mut self) {
        let window = self.skia_renderer.window();

        window.set_minimized(true);
    }

    pub fn set_ime(&mut self, ime_enabled: bool) {
        self.ime_enabled = ime_enabled;
        self.skia_renderer.window().set_ime_allowed(ime_enabled);
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
                self.skia_renderer.window().focus_window();
            }
            WindowCommand::Minimize => {
                self.minimize_window();
                self.is_minimized = true;
            }
            WindowCommand::ThemeChanged(new_theme) => {
                self.handle_theme_changed(new_theme);
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
                self.requested_columns = columns.map(|v| v.try_into().unwrap());
            }
            WindowSettingsChanged::ObservedLines(lines) => {
                log::info!("lines changed");
                self.requested_lines = lines.map(|v| v.try_into().unwrap());
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
            WindowSettingsChanged::WindowBlurred(blur) => {
                let WindowSettings { transparency, .. } = SETTINGS.get::<WindowSettings>();
                let transparent = transparency < 1.0;
                self.skia_renderer.window().set_blur(blur && transparent);
            }
            #[cfg(target_os = "macos")]
            WindowSettingsChanged::InputMacosOptionKeyIsMeta(option) => {
                self.set_macos_option_as_meta(option);
            }
            #[cfg(target_os = "macos")]
            WindowSettingsChanged::InputMacosAltIsMeta(enabled) => {
                if enabled {
                    error_msg!(concat!(
                        "neovide_input_macos_alt_is_meta has now been removed. ",
                        "Use neovide_input_macos_option_key_is_meta instead. ",
                        "Please check https://neovide.dev/configuration.html#macos-option-key-is-meta for more information.",
                    ));
                }
            }
            _ => {}
        };
        #[cfg(target_os = "macos")]
        self.macos_feature.handle_settings_changed(changed_setting);
    }

    fn handle_render_settings_changed(&mut self, changed_setting: RendererSettingsChanged) {
        match changed_setting {
            RendererSettingsChanged::TextGamma(..) | RendererSettingsChanged::TextContrast(..) => {
                self.skia_renderer.resize();
                self.font_changed_last_frame = true;
            }
            _ => {}
        }
    }

    pub fn handle_title_changed(&mut self, new_title: String) {
        self.title = new_title;
        self.skia_renderer.window().set_title(&self.title);
    }

    pub fn handle_theme_changed(&mut self, new_theme: Option<Theme>) {
        self.theme = new_theme;
        self.skia_renderer.window().set_theme(self.theme);
    }

    pub fn send_font_names(&self) {
        let font_names = self.renderer.font_names();
        send_ui(ParallelCommand::DisplayAvailableFonts(font_names));
    }

    pub fn handle_quit(&mut self) {
        send_ui(ParallelCommand::Quit);
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
            self.skia_renderer.window(),
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
                event: WindowEvent::Resized { .. },
                ..
            } => {
                self.skia_renderer.resize();
                #[cfg(target_os = "macos")]
                self.macos_feature.handle_size_changed();
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
                self.vsync.update(self.skia_renderer.window());
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
            Event::UserEvent(UserEvent::SettingsChanged(SettingsChanged::Renderer(e))) => {
                self.handle_render_settings_changed(e);
            }
            Event::UserEvent(UserEvent::ConfigsChanged(config)) => {
                self.handle_config_changed(*config);
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
        if self.font_changed_last_frame {
            self.font_changed_last_frame = false;
            self.renderer.prepare_lines(true);
        }
        self.renderer.draw_frame(self.skia_renderer.canvas(), dt);
        self.skia_renderer.flush();
        {
            tracy_gpu_zone!("wait for vsync");
            self.vsync.wait_for_vsync();
        }
        self.skia_renderer.swap_buffers();
        tracy_frame();
        tracy_gpu_collect();
    }

    pub fn animate_frame(&mut self, dt: f32) -> bool {
        tracy_zone!("animate_frame", 0);

        let res = self
            .renderer
            .animate_frame(&self.get_grid_rect_from_window(GridSize::zero()).cast(), dt);
        tracy_plot!("animate_frame", res as u8 as f64);
        self.renderer.prepare_lines(false);
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
                    self.skia_renderer.window().set_visible(true);
                    self.skia_renderer.window().set_maximized(true);
                }
                WindowSize::Grid(GridSize { width, height, .. }) => {
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
                    self.skia_renderer.window().set_visible(true);
                }
            }

            // Ensure that the window has the correct IME state
            self.set_ime(self.ime_enabled);
        };
    }

    fn handle_config_changed(&mut self, config: HotReloadConfigs) {
        tracy_zone!("handle_config_changed");
        self.renderer.handle_config_changed(config);
        self.font_changed_last_frame = true;
    }

    pub fn prepare_frame(&mut self) -> ShouldRender {
        tracy_zone!("prepare_frame", 0);
        let mut should_render = ShouldRender::Wait;

        let window_settings = SETTINGS.get::<WindowSettings>();
        #[cfg(not(target_os = "macos"))]
        let window_padding_top = window_settings.padding_top;
        #[cfg(target_os = "macos")]
        let window_padding_top =
            window_settings.padding_top + self.macos_feature.extra_titlebar_height_in_pixels();
        let window_padding = WindowPadding {
            top: window_padding_top,
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
            self.skia_renderer.window().set_visible(true);
        } else if self.skia_renderer.window().is_minimized() != Some(true) {
            // NOTE: Only actually resize the grid when the window is not minimized
            // Some platforms return a zero size when that is the case, so we should not try to resize to that.
            let new_size = self.skia_renderer.window().inner_size();
            if self.saved_inner_size != new_size || self.font_changed_last_frame || padding_changed
            {
                self.window_padding = window_padding;
                self.saved_inner_size = new_size;

                self.update_grid_size_from_window();
                should_render = ShouldRender::Immediately;
            }
        }

        self.update_ime_position();

        should_render.update(self.renderer.prepare_frame());

        should_render
    }

    pub fn get_grid_size(&self) -> GridSize<u32> {
        self.renderer.get_grid_size()
    }

    fn update_window_size_from_grid(&mut self, window_padding: &WindowPadding) {
        let window = self.skia_renderer.window();

        let window_padding_size = PixelSize::new(
            window_padding.left + window_padding.right,
            window_padding.top + window_padding.bottom,
        );

        let grid_size = clamped_grid_size(&GridSize::new(
            self.requested_columns.take().unwrap_or(
                self.saved_grid_size
                    .map_or(DEFAULT_GRID_SIZE.width, |v| v.width),
            ),
            self.requested_lines.take().unwrap_or(
                self.saved_grid_size
                    .map_or(DEFAULT_GRID_SIZE.height, |v| v.height),
            ),
        ));

        let new_size = (grid_size.cast() * self.renderer.grid_renderer.grid_scale)
            .floor()
            .cast()
            .cast_unit()
            + window_padding_size;

        log::info!(
            "Resizing window based on grid. Grid Size: {:?}, Window Size {:?}",
            grid_size,
            new_size
        );
        let new_size = winit::dpi::PhysicalSize {
            width: new_size.width,
            height: new_size.height,
        };
        let _ = window.request_inner_size(new_size);
    }

    fn get_grid_size_from_window(&self, min: GridSize<u32>) -> GridSize<u32> {
        let window_padding = self.window_padding;
        let window_padding_size: PixelSize<u32> = PixelSize::new(
            window_padding.left + window_padding.right,
            window_padding.top + window_padding.bottom,
        );

        let content_size =
            PixelSize::new(self.saved_inner_size.width, self.saved_inner_size.height)
                - window_padding_size;

        let grid_size = (content_size.cast() / self.renderer.grid_renderer.grid_scale)
            .floor()
            .cast();

        grid_size.max(min)
    }

    fn get_grid_rect_from_window(&self, min: GridSize<u32>) -> GridRect<f32> {
        let size = self.get_grid_size_from_window(min).cast();
        let pos = PixelPos::new(self.window_padding.left, self.window_padding.top).cast()
            / self.renderer.grid_renderer.grid_scale;
        GridRect::<f32>::from_origin_and_size(pos, size)
    }

    fn update_grid_size_from_window(&mut self) {
        let grid_size = self.get_grid_size_from_window(MIN_GRID_SIZE);

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
            width: grid_size.width.into(),
            height: grid_size.height.into(),
        });
    }

    fn update_ime_position(&mut self) {
        let grid_scale = self.renderer.grid_renderer.grid_scale;
        let font_dimensions = grid_scale.0;
        let mut position = self.renderer.get_cursor_destination();
        position.y += font_dimensions.height;
        let position: GridPos<i32> = (position / grid_scale).floor().cast();
        let position = dpi::PhysicalPosition {
            x: position.x,
            y: position.y,
        };
        if position != self.ime_position {
            self.ime_position = position;
            self.skia_renderer.window().set_ime_cursor_area(
                dpi::Position::Physical(position),
                dpi::PhysicalSize::new(100, font_dimensions.height as u32),
            );
        }
    }

    fn handle_scale_factor_update(&mut self, scale_factor: f64) {
        #[cfg(target_os = "macos")]
        self.macos_feature.handle_scale_factor_update(scale_factor);
        self.renderer.handle_os_scale_factor_change(scale_factor);
        self.skia_renderer.resize();
    }
}
