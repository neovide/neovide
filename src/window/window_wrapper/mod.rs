#[macro_use]
mod layouts;
mod renderer;

use super::{handle_new_grid_size, keyboard::neovim_keybinding_string, settings::WindowSettings};
use crate::{
    bridge::UiCommand, channel_utils::*, cmd_line::CmdLineSettings, editor::WindowCommand,
    error_handling::ResultPanicExplanation, redraw_scheduler::REDRAW_SCHEDULER, renderer::Renderer,
    settings::SETTINGS,
};
use glutin::{
    self,
    dpi::{LogicalPosition, LogicalSize, PhysicalSize},
    event::{
        ElementState, Event, ModifiersState, MouseButton, MouseScrollDelta,
        VirtualKeyCode as Keycode, WindowEvent,
    },
    event_loop::{ControlFlow, EventLoop},
    window::{self, Fullscreen, Icon},
    ContextBuilder, GlProfile, WindowedContext,
};
use image::{load_from_memory, GenericImageView, Pixel};
use layouts::handle_qwerty_layout;
use renderer::SkiaRenderer;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::Receiver,
        Arc,
    },
    time::{Duration, Instant},
};

#[derive(RustEmbed)]
#[folder = "assets/"]
struct Asset;

pub struct GlutinWindowWrapper {
    windowed_context: WindowedContext<glutin::PossiblyCurrent>,
    skia_renderer: SkiaRenderer,
    renderer: Renderer,
    mouse_down: bool,
    mouse_position: LogicalPosition<u32>,
    mouse_enabled: bool,
    grid_id_under_mouse: u64,
    current_modifiers: Option<ModifiersState>,
    title: String,
    previous_size: PhysicalSize<u32>,
    fullscreen: bool,
    cached_size: LogicalSize<u32>,
    cached_position: LogicalPosition<u32>,
    ui_command_sender: LoggingTx<UiCommand>,
    window_command_receiver: Receiver<WindowCommand>,
}

impl GlutinWindowWrapper {
    pub fn toggle_fullscreen(&mut self) {
        let window = self.windowed_context.window();
        if self.fullscreen {
            window.set_fullscreen(None);

            // Use cached size and position
            window.set_inner_size(LogicalSize::new(
                self.cached_size.width,
                self.cached_size.height,
            ));
            window.set_outer_position(LogicalPosition::new(
                self.cached_position.x,
                self.cached_position.y,
            ));
        } else {
            let current_size = window.inner_size();
            self.cached_size = LogicalSize::new(current_size.width, current_size.height);
            let current_position = window.outer_position().unwrap();
            self.cached_position =
                LogicalPosition::new(current_position.x as u32, current_position.y as u32);
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
                WindowCommand::SetMouseEnabled(mouse_enabled) => self.mouse_enabled = mouse_enabled,
            }
        }
    }

    pub fn handle_title_changed(&mut self, new_title: String) {
        self.title = new_title;
        self.windowed_context.window().set_title(&self.title);
    }

    pub fn handle_quit(&mut self, running: &Arc<AtomicBool>) {
        if SETTINGS.get::<CmdLineSettings>().remote_tcp.is_none() {
            self.ui_command_sender
                .send(UiCommand::Quit)
                .expect("Could not send quit command to bridge");
        } else {
            running.store(false, Ordering::Relaxed);
        }
    }

    pub fn handle_keyboard_input(
        &mut self,
        keycode: Option<Keycode>,
        modifiers: Option<ModifiersState>,
    ) {
        if keycode.is_some() {
            log::trace!(
                "Keyboard Input Received: keycode-{:?} modifiers-{:?} ",
                keycode,
                modifiers
            );
        }

        if let Some(keybinding_string) =
            neovim_keybinding_string(keycode, None, modifiers, handle_qwerty_layout)
        {
            self.ui_command_sender
                .send(UiCommand::Keyboard(keybinding_string))
                .unwrap_or_explained_panic(
                    "Could not send UI command from the window system to the neovim process.",
                );
        }
    }

    pub fn handle_pointer_motion(&mut self, x: i32, y: i32) {
        let size = self.windowed_context.window().inner_size();
        if x < 0 || x as u32 >= size.width || y < 0 || y as u32 >= size.height {
            return;
        }

        let previous_position = self.mouse_position;

        let logical_position: LogicalSize<u32> = PhysicalSize::new(x as u32, y as u32)
            .to_logical(self.windowed_context.window().scale_factor());

        let mut top_window_position = (0.0, 0.0);
        let mut top_grid_position = None;

        for details in self.renderer.window_regions.iter() {
            if logical_position.width >= details.region.left as u32
                && logical_position.width < details.region.right as u32
                && logical_position.height >= details.region.top as u32
                && logical_position.height < details.region.bottom as u32
            {
                top_window_position = (details.region.left, details.region.top);
                top_grid_position = Some((
                    details.id,
                    LogicalSize::<u32>::new(
                        logical_position.width - details.region.left as u32,
                        logical_position.height - details.region.top as u32,
                    ),
                    details.floating_order.is_some(),
                ));
            }
        }

        if let Some((grid_id, grid_position, grid_floating)) = top_grid_position {
            self.grid_id_under_mouse = grid_id;
            self.mouse_position = LogicalPosition::new(
                (grid_position.width as u64 / self.renderer.font_width) as u32,
                (grid_position.height as u64 / self.renderer.font_height) as u32,
            );

            if self.mouse_enabled && self.mouse_down && previous_position != self.mouse_position {
                let (window_left, window_top) = top_window_position;

                // Until https://github.com/neovim/neovim/pull/12667 is merged, we have to special
                // case non floating windows. Floating windows correctly transform mouse positions
                // into grid coordinates, but non floating windows do not.
                let position = if grid_floating {
                    (self.mouse_position.x, self.mouse_position.y)
                } else {
                    let adjusted_drag_left =
                        self.mouse_position.x + (window_left / self.renderer.font_width as f32) as u32;
                    let adjusted_drag_top =
                        self.mouse_position.y + (window_top / self.renderer.font_height as f32) as u32;
                    (adjusted_drag_left, adjusted_drag_top)
                };

                self.ui_command_sender
                    .send(UiCommand::Drag {
                        grid_id: self.grid_id_under_mouse,
                        position,
                    })
                    .ok();
            }
        }
    }

    pub fn handle_pointer_down(&mut self) {
        if self.mouse_enabled {
            self.ui_command_sender
                .send(UiCommand::MouseButton {
                    action: String::from("press"),
                    grid_id: self.grid_id_under_mouse,
                    position: (self.mouse_position.x, self.mouse_position.y),
                })
                .ok();
        }
        self.mouse_down = true;
    }

    pub fn handle_pointer_up(&mut self) {
        if self.mouse_enabled {
            self.ui_command_sender
                .send(UiCommand::MouseButton {
                    action: String::from("release"),
                    grid_id: self.grid_id_under_mouse,
                    position: (self.mouse_position.x, self.mouse_position.y),
                })
                .ok();
        }
        self.mouse_down = false;
    }

    pub fn handle_mouse_wheel(&mut self, x: f32, y: f32) {
        if !self.mouse_enabled {
            return;
        }

        let vertical_input_type = match y {
            _ if y > 0.0 => Some("up"),
            _ if y < 0.0 => Some("down"),
            _ => None,
        };

        if let Some(input_type) = vertical_input_type {
            self.ui_command_sender
                .send(UiCommand::Scroll {
                    direction: input_type.to_string(),
                    grid_id: self.grid_id_under_mouse,
                    position: (self.mouse_position.x, self.mouse_position.y),
                })
                .ok();
        }

        let horizontal_input_type = match y {
            _ if x > 0.0 => Some("right"),
            _ if x < 0.0 => Some("left"),
            _ => None,
        };

        if let Some(input_type) = horizontal_input_type {
            self.ui_command_sender
                .send(UiCommand::Scroll {
                    direction: input_type.to_string(),
                    grid_id: self.grid_id_under_mouse,
                    position: (self.mouse_position.x, self.mouse_position.y),
                })
                .ok();
        }
    }

    pub fn handle_focus_lost(&mut self) {
        self.ui_command_sender.send(UiCommand::FocusLost).ok();
    }

    pub fn handle_focus_gained(&mut self) {
        self.ui_command_sender.send(UiCommand::FocusGained).ok();
        REDRAW_SCHEDULER.queue_next_frame();
    }

    pub fn handle_event(&mut self, event: Event<()>, running: &Arc<AtomicBool>) {
        let mut keycode = None;
        let mut ignore_text_this_frame = false;

        match event {
            Event::LoopDestroyed => {
                self.handle_quit(running);
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                self.handle_quit(running);
            }
            Event::WindowEvent {
                event: WindowEvent::DroppedFile(path),
                ..
            } => {
                self.ui_command_sender
                    .send(UiCommand::FileDrop(
                        path.into_os_string().into_string().unwrap(),
                    ))
                    .ok();
            }
            Event::WindowEvent {
                event: WindowEvent::KeyboardInput { input, .. },
                ..
            } => {
                if input.state == ElementState::Pressed {
                    keycode = input.virtual_keycode;
                }
            }
            Event::WindowEvent {
                event: WindowEvent::ModifiersChanged(m),
                ..
            } => {
                self.current_modifiers = Some(m);
            }
            Event::WindowEvent {
                event: WindowEvent::CursorMoved { position, .. },
                ..
            } => self.handle_pointer_motion(position.x as i32, position.y as i32),
            Event::WindowEvent {
                event:
                    WindowEvent::MouseWheel {
                        delta: MouseScrollDelta::LineDelta(x, y),
                        ..
                    },
                ..
            } => self.handle_mouse_wheel(x as f32, y as f32),
            Event::WindowEvent {
                event:
                    WindowEvent::MouseWheel {
                        delta: MouseScrollDelta::PixelDelta(logical_position),
                        ..
                    },
                ..
            } => self.handle_mouse_wheel(0.0, logical_position.y as f32),
            Event::WindowEvent {
                event:
                    WindowEvent::MouseInput {
                        button: MouseButton::Left,
                        state,
                        ..
                    },
                ..
            } => {
                if state == ElementState::Pressed {
                    self.handle_pointer_down();
                } else {
                    self.handle_pointer_up();
                }
            }
            Event::WindowEvent {
                event: WindowEvent::Focused(focus),
                ..
            } => {
                if focus {
                    ignore_text_this_frame = true; // Ignore any text events on the first frame when focus is regained. https://github.com/Kethku/neovide/issues/193
                    self.handle_focus_gained();
                } else {
                    self.handle_focus_lost();
                }
            }
            Event::WindowEvent { .. } => REDRAW_SCHEDULER.queue_next_frame(),
            _ => {}
        }

        if !ignore_text_this_frame {
            self.handle_keyboard_input(keycode, self.current_modifiers);
        }
    }

    pub fn draw_frame(&mut self, dt: f32) {
        let window = self.windowed_context.window();
        let new_size = window.inner_size();
        if self.previous_size != new_size {
            self.previous_size = new_size;
            let new_size: LogicalSize<u32> = new_size.to_logical(window.scale_factor());
            handle_new_grid_size(
                (new_size.width as u64, new_size.height as u64),
                &self.renderer,
                &self.ui_command_sender,
            );
            self.skia_renderer.resize(&self.windowed_context);
        }

        let current_size = self.previous_size;
        let ui_command_sender = self.ui_command_sender.clone();

        if REDRAW_SCHEDULER.should_draw() || SETTINGS.get::<WindowSettings>().no_idle {
            log::debug!("Render Triggered");

            let scaling = 1.0 / self.windowed_context.window().scale_factor();
            let renderer = &mut self.renderer;

            {
                let canvas = self.skia_renderer.canvas();

                if renderer.draw_frame(canvas, dt, scaling as f32) {
                    handle_new_grid_size(
                        (current_size.width as u64, current_size.height as u64), 
                        &renderer, 
                        &ui_command_sender);
                }
            }

            self.skia_renderer.gr_context.flush(None);

            self.windowed_context.swap_buffers().unwrap();
        }
    }
}

pub fn start_loop(
    window_command_receiver: Receiver<WindowCommand>,
    ui_command_sender: LoggingTx<UiCommand>,
    running: Arc<AtomicBool>,
    logical_size: (u64, u64),
    renderer: Renderer,
) {
    let icon = {
        let icon_data = Asset::get("nvim.ico").expect("Failed to read icon data");
        let icon = load_from_memory(&icon_data).expect("Failed to parse icon data");
        let (width, height) = icon.dimensions();
        let mut rgba = Vec::with_capacity((width * height) as usize * 4);
        for (_, _, pixel) in icon.pixels() {
            rgba.extend_from_slice(&pixel.to_rgba().0);
        }
        Icon::from_rgba(rgba, width, height).expect("Failed to create icon object")
    };
    log::info!("icon created");

    let event_loop = EventLoop::new();
    let (width, height) = logical_size;
    let logical_size: LogicalSize<u32> = (width as u32, height as u32).into();
    let winit_window_builder = window::WindowBuilder::new()
        .with_title("Neovide")
        .with_inner_size(logical_size)
        .with_window_icon(Some(icon))
        .with_maximized(SETTINGS.get::<CmdLineSettings>().maximized)
        .with_decorations(!SETTINGS.get::<CmdLineSettings>().frameless);

    let windowed_context = ContextBuilder::new()
        .with_pixel_format(24, 8)
        .with_gl_profile(GlProfile::Core)
        .with_vsync(false)
        .with_srgb(false)
        .build_windowed(winit_window_builder, &event_loop)
        .unwrap();
    let windowed_context = unsafe { windowed_context.make_current().unwrap() };
    let previous_size = logical_size.to_physical(windowed_context.window().scale_factor());

    log::info!("window created");

    let skia_renderer = SkiaRenderer::new(&windowed_context);

    let mut window_wrapper = GlutinWindowWrapper {
        windowed_context,
        skia_renderer,
        renderer,
        mouse_down: false,
        mouse_position: LogicalPosition::new(0, 0),
        mouse_enabled: true,
        grid_id_under_mouse: 0,
        current_modifiers: None,
        title: String::from("Neovide"),
        previous_size,
        fullscreen: false,
        cached_size: LogicalSize::new(0, 0),
        cached_position: LogicalPosition::new(0, 0),
        ui_command_sender,
        window_command_receiver,
    };

    let mut previous_frame_start = Instant::now();

    event_loop.run(move |e, _window_target, control_flow| {
        if !running.load(Ordering::Relaxed) {
            std::process::exit(0);
        }

        let frame_start = Instant::now();

        window_wrapper.handle_window_commands();
        window_wrapper.synchronize_settings();
        window_wrapper.handle_event(e, &running);

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
