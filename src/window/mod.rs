mod keyboard_manager;
mod mouse_manager;
mod renderer;
mod settings;

#[cfg(target_os = "macos")]
mod draw_background;

use std::time::{Duration, Instant};

use log::trace;
use tokio::sync::mpsc::UnboundedReceiver;
use winit::{
    dpi::PhysicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{self, Fullscreen, Icon},
};

#[cfg(target_os = "macos")]
use winit::platform::macos::WindowBuilderExtMacOS;

#[cfg(target_os = "macos")]
use draw_background::draw_background;

#[cfg(target_os = "linux")]
use winit::platform::unix::WindowBuilderExtUnix;

use image::{load_from_memory, GenericImageView, Pixel};
use keyboard_manager::KeyboardManager;
use mouse_manager::MouseManager;
use renderer::SkiaRenderer;

use crate::{
    bridge::{ParallelCommand, UiCommand},
    cmd_line::CmdLineSettings,
    dimensions::Dimensions,
    editor::EditorCommand,
    event_aggregator::EVENT_AGGREGATOR,
    frame::Frame,
    redraw_scheduler::REDRAW_SCHEDULER,
    renderer::Renderer,
    renderer::WindowPadding,
    renderer::{build_context, WindowedContext},
    running_tracker::*,
    settings::{
        load_last_window_settings, save_window_size, PersistentWindowSettings,
        DEFAULT_WINDOW_GEOMETRY, SETTINGS,
    },
};
pub use settings::{KeyboardSettings, WindowSettings};

static ICON: &[u8] = include_bytes!("../../assets/neovide.ico");

const MIN_WINDOW_WIDTH: u64 = 20;
const MIN_WINDOW_HEIGHT: u64 = 6;

#[derive(Clone, Debug)]
pub enum WindowCommand {
    TitleChanged(String),
    SetMouseEnabled(bool),
    ListAvailableFonts,
}

pub struct WinitWindowWrapper {
    windowed_context: WindowedContext,
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
}

impl WinitWindowWrapper {
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

    pub fn synchronize_settings(&mut self) {
        let fullscreen = { SETTINGS.get::<WindowSettings>().fullscreen };

        if self.fullscreen != fullscreen {
            self.toggle_fullscreen();
        }
    }

    #[allow(clippy::needless_collect)]
    pub fn handle_window_commands(&mut self) {
        while let Ok(window_command) = self.window_command_receiver.try_recv() {
            match window_command {
                WindowCommand::TitleChanged(new_title) => self.handle_title_changed(new_title),
                WindowCommand::SetMouseEnabled(mouse_enabled) => {
                    self.mouse_manager.enabled = mouse_enabled
                }
                WindowCommand::ListAvailableFonts => self.send_font_names(),
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
            Event::RedrawRequested(..) | Event::WindowEvent { .. } => {
                REDRAW_SCHEDULER.queue_next_frame()
            }
            _ => {}
        }
    }

    pub fn draw_frame(&mut self, dt: f32) {
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

        if REDRAW_SCHEDULER.should_draw() || SETTINGS.get::<WindowSettings>().no_idle {
            self.font_changed_last_frame =
                self.renderer.draw_frame(self.skia_renderer.canvas(), dt);
            self.skia_renderer.gr_context.flush(None);
            self.windowed_context.swap_buffers().unwrap();
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

pub fn create_window() {
    let icon = {
        let icon = load_from_memory(ICON).expect("Failed to parse icon data");
        let (width, height) = icon.dimensions();
        let mut rgba = Vec::with_capacity((width * height) as usize * 4);
        for (_, _, pixel) in icon.pixels() {
            rgba.extend_from_slice(&pixel.to_rgba().0);
        }
        Icon::from_rgba(rgba, width, height).expect("Failed to create icon object")
    };

    let event_loop = EventLoop::new();

    let cmd_line_settings = SETTINGS.get::<CmdLineSettings>();

    let mut maximized = cmd_line_settings.maximized;
    let mut previous_position = None;
    if let Ok(last_window_settings) = load_last_window_settings() {
        match last_window_settings {
            PersistentWindowSettings::Maximized => {
                maximized = true;
            }
            PersistentWindowSettings::Windowed { position, .. } => {
                previous_position = Some(position);
            }
        }
    }

    let winit_window_builder = window::WindowBuilder::new()
        .with_title("Neovide")
        .with_window_icon(Some(icon))
        .with_maximized(maximized)
        .with_transparent(true);

    let frame_decoration = cmd_line_settings.frame;

    // There is only two options for windows & linux, no need to match more options.
    #[cfg(not(target_os = "macos"))]
    let mut winit_window_builder =
        winit_window_builder.with_decorations(frame_decoration == Frame::Full);

    #[cfg(target_os = "macos")]
    let mut winit_window_builder = match frame_decoration {
        Frame::Full => winit_window_builder,
        Frame::None => winit_window_builder.with_decorations(false),
        Frame::Buttonless => winit_window_builder
            .with_transparent(true)
            .with_title_hidden(true)
            .with_titlebar_buttons_hidden(true)
            .with_titlebar_transparent(true)
            .with_fullsize_content_view(true),
        Frame::Transparent => winit_window_builder
            .with_title_hidden(true)
            .with_titlebar_transparent(true)
            .with_fullsize_content_view(true),
    };

    if let Some(previous_position) = previous_position {
        if !maximized {
            winit_window_builder = winit_window_builder.with_position(previous_position);
        }
    }

    #[cfg(target_os = "linux")]
    let winit_window_builder = winit_window_builder
        .with_app_id(cmd_line_settings.wayland_app_id.clone())
        .with_class(
            cmd_line_settings.x11_wm_class_instance.clone(),
            cmd_line_settings.x11_wm_class.clone(),
        );

    #[cfg(target_os = "macos")]
    let winit_window_builder = winit_window_builder.with_accepts_first_mouse(false);

    let windowed_context = build_context(&cmd_line_settings, winit_window_builder, &event_loop);

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

            let window_position = window.outer_position().ok()?;
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
    };

    let mut previous_frame_start = Instant::now();

    enum FocusedState {
        Focused,
        UnfocusedNotDrawn,
        Unfocused,
    }
    let mut focused = FocusedState::Focused;

    event_loop.run(move |e, _window_target, control_flow| {
        // Window focus changed
        if let Event::WindowEvent {
            event: WindowEvent::Focused(focused_event),
            ..
        } = e
        {
            focused = if focused_event {
                FocusedState::Focused
            } else {
                FocusedState::UnfocusedNotDrawn
            };
        }

        if !RUNNING_TRACKER.is_running() {
            let window = window_wrapper.windowed_context.window();
            save_window_size(
                window.is_maximized(),
                window.inner_size(),
                window.outer_position().ok(),
            );

            std::process::exit(RUNNING_TRACKER.exit_code());
        }

        let frame_start = Instant::now();

        window_wrapper.handle_window_commands();
        window_wrapper.synchronize_settings();
        window_wrapper.handle_event(e);

        let refresh_rate = match focused {
            FocusedState::Focused | FocusedState::UnfocusedNotDrawn => {
                SETTINGS.get::<WindowSettings>().refresh_rate as f32
            }
            FocusedState::Unfocused => SETTINGS.get::<WindowSettings>().refresh_rate_idle as f32,
        }
        .max(1.0);

        let expected_frame_length_seconds = 1.0 / refresh_rate;
        let frame_duration = Duration::from_secs_f32(expected_frame_length_seconds);

        if frame_start - previous_frame_start > frame_duration {
            let dt = previous_frame_start.elapsed().as_secs_f32();
            window_wrapper.draw_frame(dt);
            if let FocusedState::UnfocusedNotDrawn = focused {
                focused = FocusedState::Unfocused;
            }
            previous_frame_start = frame_start;
            #[cfg(target_os = "macos")]
            draw_background(window_wrapper.windowed_context.window());
        }

        *control_flow = ControlFlow::WaitUntil(previous_frame_start + frame_duration)
    });
}
