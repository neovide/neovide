mod keyboard_manager;
mod mouse_manager;
mod renderer;

use std::{
    sync::mpsc::Receiver,
    time::{Duration, Instant},
};

use glutin::{
    self,
    dpi::PhysicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{self, Fullscreen, Icon},
    ContextBuilder, GlProfile, WindowedContext,
};
use log::trace;

#[cfg(target_os = "macos")]
use glutin::platform::macos::WindowBuilderExtMacOS;

#[cfg(target_os = "linux")]
use glutin::platform::unix::WindowBuilderExtUnix;

use super::settings::WindowSettings;
use crate::{
    bridge::{ParallelCommand, UiCommand},
    channel_utils::*,
    cmd_line::CmdLineSettings,
    editor::DrawCommand,
    editor::WindowCommand,
    redraw_scheduler::REDRAW_SCHEDULER,
    renderer::Renderer,
    running_tracker::*,
    settings::{maybe_save_window_size, SETTINGS},
    utils::Dimensions,
};
use image::{load_from_memory, GenericImageView, Pixel};
use keyboard_manager::KeyboardManager;
use mouse_manager::MouseManager;
use renderer::SkiaRenderer;

static ICON: &[u8] = include_bytes!("../../../assets/neovide.ico");

const MIN_WINDOW_WIDTH: u64 = 20;
const MIN_WINDOW_HEIGHT: u64 = 6;

pub struct GlutinWindowWrapper {
    windowed_context: WindowedContext<glutin::PossiblyCurrent>,
    skia_renderer: SkiaRenderer,
    renderer: Renderer,
    keyboard_manager: KeyboardManager,
    mouse_manager: MouseManager,
    title: String,
    fullscreen: bool,
    saved_inner_size: PhysicalSize<u32>,
    saved_grid_size: Option<Dimensions>,
    ui_command_sender: LoggingTx<UiCommand>,
    window_command_receiver: Receiver<WindowCommand>,
}

impl GlutinWindowWrapper {
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
        let window_commands: Vec<WindowCommand> = self.window_command_receiver.try_iter().collect();
        for window_command in window_commands.into_iter() {
            match window_command {
                WindowCommand::TitleChanged(new_title) => self.handle_title_changed(new_title),
                WindowCommand::SetMouseEnabled(mouse_enabled) => {
                    self.mouse_manager.enabled = mouse_enabled
                }
            }
        }
    }

    pub fn handle_title_changed(&mut self, new_title: String) {
        self.title = new_title;
        self.windowed_context.window().set_title(&self.title);
    }

    pub fn handle_quit(&mut self) {
        if SETTINGS.get::<CmdLineSettings>().remote_tcp.is_none() {
            self.ui_command_sender
                .send(ParallelCommand::Quit.into())
                .expect("Could not send quit command to bridge");
        } else {
            RUNNING_TRACKER.quit("window closed");
        }
    }

    pub fn handle_focus_lost(&mut self) {
        self.ui_command_sender
            .send(ParallelCommand::FocusLost.into())
            .ok();
    }

    pub fn handle_focus_gained(&mut self) {
        self.ui_command_sender
            .send(ParallelCommand::FocusGained.into())
            .ok();
        REDRAW_SCHEDULER.queue_next_frame();
    }

    pub fn handle_event(&mut self, event: Event<()>) {
        self.keyboard_manager.handle_event(&event);
        self.mouse_manager.handle_event(
            &event,
            &self.keyboard_manager,
            &self.renderer,
            &self.windowed_context,
        );
        match event {
            Event::LoopDestroyed => {
                self.handle_quit();
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
                self.ui_command_sender
                    .send(
                        ParallelCommand::FileDrop(path.into_os_string().into_string().unwrap())
                            .into(),
                    )
                    .ok();
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
        let mut font_changed = false;

        if REDRAW_SCHEDULER.should_draw() || SETTINGS.get::<WindowSettings>().no_idle {
            font_changed = self.renderer.draw_frame(self.skia_renderer.canvas(), dt);
            self.skia_renderer.gr_context.flush(None);
            self.windowed_context.swap_buffers().unwrap();
        }

        // Wait until fonts are loaded, so we can set proper window size.
        if !self.renderer.grid_renderer.is_ready {
            return;
        }

        let new_size = window.inner_size();
        let settings = SETTINGS.get::<CmdLineSettings>();
        // Resize at startup happens when window is maximized or when using tiling WM
        // which already resized window.
        let resized_at_startup = settings.maximized || is_already_resized(new_size);

        if self.saved_grid_size.is_none() && !resized_at_startup {
            window.set_inner_size(
                self.renderer
                    .grid_renderer
                    .convert_grid_to_physical(settings.geometry),
            );
            self.saved_grid_size = Some(settings.geometry);
            // Font change at startup is ignored, so grid size (and startup screen) could be preserved.
            // But only when not resized yet. With maximized or resized window we should redraw grid.
            font_changed = false;
        }

        if self.saved_inner_size != new_size || font_changed {
            self.saved_inner_size = new_size;
            self.handle_new_grid_size(new_size);
            self.skia_renderer.resize(&self.windowed_context);
        }
    }

    fn handle_new_grid_size(&mut self, new_size: PhysicalSize<u32>) {
        let grid_size = self
            .renderer
            .grid_renderer
            .convert_physical_to_grid(new_size);

        // Have a minimum size
        if grid_size.width < MIN_WINDOW_WIDTH || grid_size.height < MIN_WINDOW_HEIGHT {
            return;
        }

        if self.saved_grid_size == Some(grid_size) {
            trace!("Grid matched saved size, skip update.");
            return;
        }
        self.saved_grid_size = Some(grid_size);
        self.ui_command_sender
            .send(
                ParallelCommand::Resize {
                    width: grid_size.width,
                    height: grid_size.height,
                }
                .into(),
            )
            .ok();
    }

    fn handle_scale_factor_update(&mut self, scale_factor: f64) {
        self.renderer
            .grid_renderer
            .handle_scale_factor_update(scale_factor);
    }
}

pub fn create_window(
    batched_draw_command_receiver: Receiver<Vec<DrawCommand>>,
    window_command_receiver: Receiver<WindowCommand>,
    ui_command_sender: LoggingTx<UiCommand>,
) {
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
    #[cfg(not(target_os = "macos"))]
    let winit_window_builder = window::WindowBuilder::new()
        .with_title("Neovide")
        .with_window_icon(Some(icon))
        .with_maximized(cmd_line_settings.maximized)
        .with_decorations(!cmd_line_settings.frameless);

    #[cfg(target_os = "macos")]
    let frame = (cmd_line_settings.frameless, cmd_line_settings.buttonless);

    #[cfg(target_os = "macos")]
    let winit_window_builder = match frame {
        (false, false) => window::WindowBuilder::new()
            .with_title("Neovide")
            .with_window_icon(Some(icon))
            .with_maximized(cmd_line_settings.maximized),
        (true, false) => window::WindowBuilder::new()
            .with_title("Neovide")
            .with_window_icon(Some(icon))
            .with_maximized(cmd_line_settings.maximized)
            .with_decorations(false),
        (_, true) => window::WindowBuilder::new()
            .with_window_icon(Some(icon))
            .with_maximized(cmd_line_settings.maximized)
            //.with_has_shadow(false)
            .with_title_hidden(true)
            .with_titlebar_buttons_hidden(true)
            .with_titlebar_transparent(true)
            .with_fullsize_content_view(true),
    };

    #[cfg(target_os = "linux")]
    let winit_window_builder = winit_window_builder
        .with_app_id(SETTINGS.get::<CmdLineSettings>().wayland_app_id)
        .with_class(
            "neovide".to_string(),
            SETTINGS.get::<CmdLineSettings>().x11_wm_class,
        );

    let windowed_context = ContextBuilder::new()
        .with_pixel_format(24, 8)
        .with_stencil_buffer(8)
        .with_gl_profile(GlProfile::Core)
        .with_vsync(false)
        .with_srgb(cmd_line_settings.srgb)
        .build_windowed(winit_window_builder, &event_loop)
        .unwrap();
    let windowed_context = unsafe { windowed_context.make_current().unwrap() };

    let window = windowed_context.window();

    let scale_factor = windowed_context.window().scale_factor();
    let renderer = Renderer::new(batched_draw_command_receiver, scale_factor);
    let saved_inner_size = window.inner_size();

    let skia_renderer = SkiaRenderer::new(&windowed_context);

    log::info!(
        "window created (scale_factor: {:.4}, font_dimensions: {:?})",
        scale_factor,
        renderer.grid_renderer.font_dimensions,
    );

    let mut window_wrapper = GlutinWindowWrapper {
        windowed_context,
        skia_renderer,
        renderer,
        keyboard_manager: KeyboardManager::new(ui_command_sender.clone()),
        mouse_manager: MouseManager::new(ui_command_sender.clone()),
        title: String::from("Neovide"),
        fullscreen: false,
        saved_inner_size,
        saved_grid_size: None,
        ui_command_sender,
        window_command_receiver,
    };

    let mut previous_frame_start = Instant::now();

    event_loop.run(move |e, _window_target, control_flow| {
        if !RUNNING_TRACKER.is_running() {
            maybe_save_window_size(window_wrapper.saved_grid_size);
            std::process::exit(0);
        }

        let frame_start = Instant::now();

        window_wrapper.handle_window_commands();
        window_wrapper.synchronize_settings();
        window_wrapper.handle_event(e);

        let refresh_rate = { SETTINGS.get::<WindowSettings>().refresh_rate as f32 };
        let expected_frame_length_seconds = 1.0 / refresh_rate;
        let frame_duration = Duration::from_secs_f32(expected_frame_length_seconds);

        if frame_start - previous_frame_start > frame_duration {
            let dt = previous_frame_start.elapsed().as_secs_f32();
            window_wrapper.draw_frame(dt);
            previous_frame_start = frame_start;
        }

        *control_flow = ControlFlow::WaitUntil(previous_frame_start + frame_duration)
    });
}

fn is_already_resized(size: PhysicalSize<u32>) -> bool {
    size != PhysicalSize::from((800, 600))
}
