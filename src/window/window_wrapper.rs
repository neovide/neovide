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
    },
    settings::{
        clamped_grid_size, FontSettings, HotReloadConfigs, SettingsChanged, DEFAULT_GRID_SIZE,
        MIN_GRID_SIZE, SETTINGS,
    },
    units::{GridPos, GridRect, GridSize, PixelPos, PixelSize},
    window::{create_window, PhysicalSize, ShouldRender, WindowSize},
    CmdLineSettings,
};

#[cfg(target_os = "macos")]
use super::macos::MacosWindowFeature;

#[cfg(target_os = "macos")]
use icrate::Foundation::MainThreadMarker;

use log::trace;
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use winit::{
    dpi,
    event::{Event, WindowEvent},
    event_loop::{EventLoopProxy, EventLoopWindowTarget},
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

#[derive(PartialEq, PartialOrd)]
enum UIState {
    Initing, // Running init.vim/lua
    WaitingForWindowCreate,
    FirstFrame,
    Showing, // No pending resizes
}

pub struct WinitWindowWrapper {
    // Don't rearrange this, unless you have a good reason to do so
    // The destruction order has to be correct
    pub skia_renderer: Option<Box<dyn SkiaRenderer>>,
    pub renderer: Renderer,
    keyboard_manager: KeyboardManager,
    mouse_manager: MouseManager,
    title: String,
    fullscreen: bool,
    font_changed_last_frame: bool,
    saved_inner_size: dpi::PhysicalSize<u32>,
    saved_grid_size: Option<GridSize<u32>>,
    ime_position: dpi::PhysicalPosition<i32>,
    requested_columns: Option<u32>,
    requested_lines: Option<u32>,
    ui_state: UIState,
    window_padding: WindowPadding,
    initial_window_size: WindowSize,
    is_minimized: bool,
    pub vsync: Option<VSync>,
    #[cfg(target_os = "macos")]
    pub macos_feature: Option<MacosWindowFeature>,
}

impl WinitWindowWrapper {
    pub fn new(
        initial_window_size: WindowSize,
        initial_font_settings: Option<FontSettings>,
    ) -> Self {
        let saved_inner_size = Default::default();
        let renderer = Renderer::new(1.0, initial_font_settings);

        Self {
            skia_renderer: None,
            renderer,
            keyboard_manager: KeyboardManager::new(),
            mouse_manager: MouseManager::new(),
            title: String::from("Neovide"),
            fullscreen: false,
            font_changed_last_frame: false,
            saved_inner_size,
            saved_grid_size: None,
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
            vsync: None,
            #[cfg(target_os = "macos")]
            macos_feature: None,
        }
    }

    pub fn toggle_fullscreen(&mut self) {
        if let Some(skia_renderer) = &self.skia_renderer {
            let window = skia_renderer.window();
            if self.fullscreen {
                window.set_fullscreen(None);
            } else {
                let handle = window.current_monitor();
                window.set_fullscreen(Some(Fullscreen::Borderless(handle)));
            }
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

        if let Some(skia_renderer) = &self.skia_renderer {
            let window = skia_renderer.window();
            if winit_option != window.option_as_alt() {
                window.set_option_as_alt(winit_option);
            }
        }
    }

    pub fn minimize_window(&mut self) {
        if let Some(skia_renderer) = &self.skia_renderer {
            let window = skia_renderer.window();

            window.set_minimized(true);
        }
    }

    pub fn set_ime(&mut self, ime_enabled: bool) {
        if let Some(skia_renderer) = &self.skia_renderer {
            skia_renderer.window().set_ime_allowed(ime_enabled);
        }
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
                if let Some(skia_renderer) = &self.skia_renderer {
                    skia_renderer.window().focus_window();
                }
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
                self.set_ime(ime_enabled);
            }
            WindowSettingsChanged::WindowBlurred(blur) => {
                if let Some(skia_renderer) = &self.skia_renderer {
                    let WindowSettings { transparency, .. } = SETTINGS.get::<WindowSettings>();
                    let transparent = transparency < 1.0;
                    skia_renderer.window().set_blur(blur && transparent);
                }
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
        if let Some(macos_feature) = &self.macos_feature {
            macos_feature.handle_settings_changed(changed_setting);
        }
    }

    fn handle_render_settings_changed(&mut self, changed_setting: RendererSettingsChanged) {
        match changed_setting {
            RendererSettingsChanged::TextGamma(..) | RendererSettingsChanged::TextContrast(..) => {
                if let Some(skia_renderer) = &mut self.skia_renderer {
                    skia_renderer.resize();
                }
                self.font_changed_last_frame = true;
            }
            _ => {}
        }
    }

    pub fn handle_title_changed(&mut self, new_title: String) {
        self.title = new_title;
        if let Some(skia_renderer) = &self.skia_renderer {
            skia_renderer.window().set_title(&self.title);
        }
    }

    pub fn handle_theme_changed(&mut self, new_theme: Option<Theme>) {
        if let Some(skia_renderer) = &self.skia_renderer {
            skia_renderer.window().set_theme(new_theme);
        }
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

        let renderer_asks_to_be_rendered = self.renderer.handle_event(&event);
        let mut should_render = true;
        match event {
            Event::Resumed => {
                tracy_zone!("Resumed");
                // No need to do anything, but handle the event so that should_render gets set
            }
            Event::WindowEvent { event, .. } => {
                if !self.handle_window_event(event) {
                    should_render = renderer_asks_to_be_rendered;
                }
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
        self.ui_state >= UIState::FirstFrame && should_render
    }

    fn handle_window_event(&mut self, event: WindowEvent) -> bool {
        // The renderer and vsync should always be created when a window event is received
        let skia_renderer = self.skia_renderer.as_mut().unwrap();
        let vsync = self.vsync.as_mut().unwrap();

        self.mouse_manager.handle_event(
            &event,
            &self.keyboard_manager,
            &self.renderer,
            skia_renderer.window(),
        );
        self.keyboard_manager.handle_event(&event);

        match event {
            WindowEvent::CloseRequested => {
                tracy_zone!("CloseRequested");
                self.handle_quit();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                tracy_zone!("ScaleFactorChanged");
                self.handle_scale_factor_update(scale_factor);
            }
            WindowEvent::Resized { .. } => {
                skia_renderer.resize();
                #[cfg(target_os = "macos")]
                self.macos_feature.as_mut().unwrap().handle_size_changed();
            }
            WindowEvent::DroppedFile(path) => {
                tracy_zone!("DroppedFile");
                let file_path = path.into_os_string().into_string().unwrap();
                send_ui(ParallelCommand::FileDrop(file_path));
            }
            WindowEvent::Focused(focus) => {
                tracy_zone!("Focused");
                if focus {
                    self.handle_focus_gained();
                } else {
                    self.handle_focus_lost();
                }
            }
            WindowEvent::ThemeChanged(theme) => {
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
            WindowEvent::Moved(_) => {
                tracy_zone!("Moved");
                vsync.update(skia_renderer.window());
            }
            _ => {
                tracy_zone!("Unknown WindowEvent");
                return false;
            }
        }
        true
    }

    pub fn draw_frame(&mut self, dt: f32) {
        tracy_zone!("draw_frame");
        if self.skia_renderer.is_none() {
            return;
        }
        let skia_renderer = self.skia_renderer.as_mut().unwrap();
        let vsync = self.vsync.as_mut().unwrap();

        if self.font_changed_last_frame {
            self.font_changed_last_frame = false;
            self.renderer.prepare_lines(true);
        }
        self.renderer.draw_frame(skia_renderer.canvas(), dt);
        skia_renderer.flush();
        {
            tracy_gpu_zone!("wait for vsync");
            vsync.wait_for_vsync();
        }
        skia_renderer.swap_buffers();
        if self.ui_state == UIState::FirstFrame {
            skia_renderer.window().set_visible(true);
            self.ui_state = UIState::Showing;
        }
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

    pub fn try_create_window(
        &mut self,
        event_loop: &EventLoopWindowTarget<UserEvent>,
        proxy: &EventLoopProxy<UserEvent>,
    ) {
        if self.ui_state != UIState::WaitingForWindowCreate {
            return;
        }
        tracy_zone!("create_window");

        let maximized = matches!(self.initial_window_size, WindowSize::Maximized);

        let window_config = create_window(event_loop, maximized, &self.title);
        let window = &window_config.window;

        // It's important that this is created before the window is resized, since it can change the padding and affect the size
        #[cfg(target_os = "macos")]
        {
            self.macos_feature = {
                let mtm = MainThreadMarker::new().expect("must be on the main thread");
                Some(MacosWindowFeature::from_winit_window(window, mtm))
            };
        }

        let scale_factor = window.scale_factor();
        self.renderer.handle_os_scale_factor_change(scale_factor);

        let mut size = PhysicalSize::default();
        match self.initial_window_size {
            WindowSize::Maximized => {}
            WindowSize::Grid(grid_size) => {
                let window_size = self.get_window_size_from_grid(&grid_size);
                size = PhysicalSize::new(window_size.width, window_size.height);
            }
            WindowSize::NeovimGrid => {
                let grid_size = self.renderer.get_grid_size();
                let window_size = self.get_window_size_from_grid(&grid_size);
                size = PhysicalSize::new(window_size.width, window_size.height);
            }
            WindowSize::Size(window_size) => {
                size = window_size;
            }
        };
        if !maximized {
            tracy_zone!("request_inner_size");
            let _ = window.request_inner_size(size);
        }

        // Check that window is visible in some monitor, and reposition it if not.
        if let Ok(previous_position) = window.outer_position() {
            if let Some(current_monitor) = window.current_monitor() {
                let monitor_position = current_monitor.position();
                let monitor_size = current_monitor.size();
                let monitor_width = monitor_size.width as i32;
                let monitor_height = monitor_size.height as i32;

                let window_position = previous_position;

                let window_size = window.outer_size();
                let window_width = window_size.width as i32;
                let window_height = window_size.height as i32;

                if window_position.x + window_width < monitor_position.x
                    || window_position.y + window_height < monitor_position.y
                    || window_position.x > monitor_position.x + monitor_width
                    || window_position.y > monitor_position.y + monitor_height
                {
                    window.set_outer_position(monitor_position);
                };
            };
        }
        log::info!("Showing window size: {:#?}, maximized: {}", size, maximized);
        let is_wayland = matches!(window.raw_window_handle(), RawWindowHandle::Wayland(_));
        // On Wayland we can show the window now, since internally it's only shown after the first rendering
        // On the other platforms the window is shown after rendering to avoid flickering
        if is_wayland {
            window.set_visible(true);
        }

        let cmd_line_settings = SETTINGS.get::<CmdLineSettings>();
        let srgb = cmd_line_settings.srgb;
        let vsync_enabled = cmd_line_settings.vsync;
        let skia_renderer = create_skia_renderer(window_config, srgb, vsync_enabled);
        let window = skia_renderer.window();

        self.saved_inner_size = window.inner_size();

        log::info!(
            "window created (scale_factor: {:.4}, font_dimensions: {:?})",
            scale_factor,
            self.renderer.grid_renderer.grid_scale.0,
        );

        let WindowSettings {
            input_ime,
            theme,
            transparency,
            window_blurred,
            ..
        } = SETTINGS.get::<WindowSettings>();

        window.set_blur(window_blurred && transparency < 1.0);

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

        self.vsync = Some(VSync::new(
            vsync_enabled,
            skia_renderer.as_ref(),
            proxy.clone(),
        ));

        {
            tracy_zone!("request_redraw");
            window.request_redraw();
        }

        // Ensure that the window has the correct IME state
        self.set_ime(input_ime);

        self.ui_state = UIState::FirstFrame;
        self.skia_renderer = Some(skia_renderer);
    }

    fn handle_draw_commands(&mut self, batch: Vec<DrawCommand>) {
        tracy_zone!("handle_draw_commands");
        let handle_draw_commands_result = self.renderer.handle_draw_commands(batch);

        self.font_changed_last_frame |= handle_draw_commands_result.font_changed;

        if self.ui_state == UIState::Initing && handle_draw_commands_result.should_show {
            log::info!("Showing the Window");
            self.ui_state = UIState::WaitingForWindowCreate;
        };
    }

    fn handle_config_changed(&mut self, config: HotReloadConfigs) {
        tracy_zone!("handle_config_changed");
        self.renderer.handle_config_changed(config);
        self.font_changed_last_frame = true;
    }

    fn calculate_window_padding(&self) -> WindowPadding {
        let window_settings = SETTINGS.get::<WindowSettings>();
        #[cfg(not(target_os = "macos"))]
        let window_padding_top = window_settings.padding_top;

        #[cfg(target_os = "macos")]
        let window_padding_top = {
            let mut padding_top = window_settings.padding_top;
            if let Some(macos_feature) = &self.macos_feature {
                padding_top += macos_feature.extra_titlebar_height_in_pixels();
            }
            padding_top
        };

        WindowPadding {
            top: window_padding_top,
            left: window_settings.padding_left,
            right: window_settings.padding_right,
            bottom: window_settings.padding_bottom,
        }
    }

    pub fn prepare_frame(&mut self) -> ShouldRender {
        tracy_zone!("prepare_frame", 0);
        let mut should_render = ShouldRender::Wait;

        let window_padding = self.calculate_window_padding();
        let padding_changed = window_padding != self.window_padding;

        // Don't render until the UI is fully entered and the window is shown
        if self.ui_state < UIState::FirstFrame {
            return ShouldRender::Wait;
        } else if self.ui_state == UIState::FirstFrame {
            should_render = ShouldRender::Immediately;
        }

        // The skia renderer shuld always be created when this point is reached, since the < UIState::FirstFrame check will return true
        let skia_renderer = self.skia_renderer.as_ref().unwrap();

        let resize_requested = self.requested_columns.is_some() || self.requested_lines.is_some();
        if resize_requested {
            // Resize requests (columns/lines) have priority over normal window sizing.
            // So, deal with them first and resize the window programmatically.
            // The new window size will then be processed in the following frame.
            self.update_window_size_from_grid();
        } else if skia_renderer.window().is_minimized() != Some(true) {
            // NOTE: Only actually resize the grid when the window is not minimized
            // Some platforms return a zero size when that is the case, so we should not try to resize to that.
            let new_size = skia_renderer.window().inner_size();
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

    fn get_window_size_from_grid(&self, grid_size: &GridSize<u32>) -> PixelSize<u32> {
        let window_padding = self.calculate_window_padding();

        let window_padding_size = PixelSize::new(
            window_padding.left + window_padding.right,
            window_padding.top + window_padding.bottom,
        );

        let window_size = (grid_size.cast() * self.renderer.grid_renderer.grid_scale)
            .floor()
            .cast()
            .cast_unit()
            + window_padding_size;

        log::info!(
            "get_window_size_from_grid: Grid Size: {:?}, Window Size {:?}",
            grid_size,
            window_size
        );
        window_size
    }

    fn update_window_size_from_grid(&mut self) {
        let window = self.skia_renderer.as_ref().unwrap().window();

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
        let new_size = self.get_window_size_from_grid(&grid_size);

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
        if self.skia_renderer.is_none() {
            return;
        }
        let skia_renderer = self.skia_renderer.as_ref().unwrap();
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
            skia_renderer.window().set_ime_cursor_area(
                dpi::Position::Physical(position),
                dpi::PhysicalSize::new(100, font_dimensions.height as u32),
            );
        }
    }

    fn handle_scale_factor_update(&mut self, scale_factor: f64) {
        if self.skia_renderer.is_none() {
            return;
        }
        let skia_renderer = self.skia_renderer.as_mut().unwrap();
        #[cfg(target_os = "macos")]
        self.macos_feature
            .as_mut()
            .unwrap()
            .handle_scale_factor_update(scale_factor);
        self.renderer.handle_os_scale_factor_change(scale_factor);
        skia_renderer.resize();
    }
}
