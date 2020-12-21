use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::thread::sleep;
use std::time::{Duration, Instant};

use crossfire::mpsc::TxUnbounded;
use log::{debug, error, trace};
use skulpin::ash::prelude::VkResult;
use skulpin::sdl2;
use skulpin::sdl2::event::{Event, WindowEvent};
use skulpin::sdl2::keyboard::Keycode;
use skulpin::sdl2::video::FullscreenType;
use skulpin::sdl2::EventPump;
use skulpin::sdl2::Sdl;
use skulpin::{
    CoordinateSystem, LogicalSize, PhysicalSize, PresentMode, Renderer as SkulpinRenderer,
    RendererBuilder, Sdl2Window, Window,
};

use super::handle_new_grid_size;
use super::settings::*;
use crate::bridge::{produce_neovim_keybinding_string, UiCommand};
use crate::editor::WindowCommand;
use crate::error_handling::ResultPanicExplanation;
use crate::redraw_scheduler::REDRAW_SCHEDULER;
use crate::renderer::Renderer;
use crate::settings::*;

#[derive(RustEmbed)]
#[folder = "assets/"]
struct Asset;

pub struct Sdl2WindowWrapper {
    context: Sdl,
    window: sdl2::video::Window,
    skulpin_renderer: SkulpinRenderer,
    event_pump: EventPump,
    renderer: Renderer,
    mouse_down: bool,
    mouse_position: LogicalSize,
    mouse_enabled: bool,
    grid_id_under_mouse: u64,
    title: String,
    previous_size: LogicalSize,
    transparency: f32,
    fullscreen: bool,
    cached_size: (u32, u32),
    cached_position: (i32, i32),
    ui_command_sender: TxUnbounded<UiCommand>,
    window_command_receiver: Receiver<WindowCommand>,
    running: Arc<AtomicBool>,
}

impl Sdl2WindowWrapper {
    pub fn toggle_fullscreen(&mut self) {
        if self.fullscreen {
            if cfg!(target_os = "windows") {
                unsafe {
                    let raw_handle = self.window.raw();
                    sdl2::sys::SDL_SetWindowResizable(raw_handle, sdl2::sys::SDL_bool::SDL_TRUE);
                }
            } else {
                self.window.set_fullscreen(FullscreenType::Off).ok();
            }

            // Use cached size and position
            self.window
                .set_size(self.cached_size.0, self.cached_size.1)
                .unwrap();
            self.window.set_position(
                sdl2::video::WindowPos::Positioned(self.cached_position.0),
                sdl2::video::WindowPos::Positioned(self.cached_position.1),
            );
        } else {
            self.cached_size = self.window.size();
            self.cached_position = self.window.position();

            if cfg!(target_os = "windows") {
                let video_subsystem = self.window.subsystem();
                if let Ok(rect) = self
                    .window
                    .display_index()
                    .and_then(|index| video_subsystem.display_bounds(index))
                {
                    // Set window to fullscreen
                    unsafe {
                        let raw_handle = self.window.raw();
                        sdl2::sys::SDL_SetWindowResizable(
                            raw_handle,
                            sdl2::sys::SDL_bool::SDL_FALSE,
                        );
                    }
                    self.window.set_size(rect.width(), rect.height()).unwrap();
                    self.window.set_position(
                        sdl2::video::WindowPos::Positioned(rect.x()),
                        sdl2::video::WindowPos::Positioned(rect.y()),
                    );
                }
            } else {
                self.window.set_fullscreen(FullscreenType::Desktop).ok();
            }
        }

        self.fullscreen = !self.fullscreen;
    }

    pub fn synchronize_settings(&mut self) {
        let transparency = { SETTINGS.get::<WindowSettings>().transparency };

        if let Ok(opacity) = self.window.opacity() {
            if (opacity - transparency).abs() > std::f32::EPSILON {
                self.window.set_opacity(transparency).ok();
                self.transparency = transparency;
            }
        }

        let fullscreen = { SETTINGS.get::<WindowSettings>().fullscreen };

        if self.fullscreen != fullscreen {
            self.toggle_fullscreen();
        }
    }

    pub fn handle_title_changed(&mut self, new_title: String) {
        self.title = new_title;
        self.window
            .set_title(&self.title)
            .expect("Could not set title");
    }

    pub fn handle_quit(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }

    pub fn handle_keyboard_input(&mut self, keycode: Option<Keycode>, text: Option<String>) {
        let modifiers = self.context.keyboard().mod_state();

        if keycode.is_some() || text.is_some() {
            trace!(
                "Keyboard Input Received: keycode-{:?} modifiers-{:?} text-{:?}",
                keycode,
                modifiers,
                text
            );
        }

        if let Some(keybinding_string) = produce_neovim_keybinding_string(keycode, text, modifiers)
        {
            self.ui_command_sender
                .send(UiCommand::Keyboard(keybinding_string))
                .unwrap_or_explained_panic(
                    "Could not send UI command from the window system to the neovim process.",
                );
        }
    }

    pub fn handle_pointer_motion(&mut self, x: i32, y: i32) {
        let previous_position = self.mouse_position;
        let sdl_window_wrapper = Sdl2Window::new(&self.window);
        let logical_position =
            PhysicalSize::new(x as u32, y as u32).to_logical(sdl_window_wrapper.scale_factor());

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
                    LogicalSize::new(
                        logical_position.width - details.region.left as u32,
                        logical_position.height - details.region.top as u32,
                    ),
                    details.floating,
                ));
            }
        }

        if let Some((grid_id, grid_position, grid_floating)) = top_grid_position {
            self.grid_id_under_mouse = grid_id;
            self.mouse_position = LogicalSize::new(
                (grid_position.width as f32 / self.renderer.font_width) as u32,
                (grid_position.height as f32 / self.renderer.font_height) as u32,
            );

            if self.mouse_enabled && self.mouse_down && previous_position != self.mouse_position {
                let (window_left, window_top) = top_window_position;

                // Until https://github.com/neovim/neovim/pull/12667 is merged, we have to special
                // case non floating windows. Floating windows correctly transform mouse positions
                // into grid coordinates, but non floating windows do not.
                let position = if grid_floating {
                    (self.mouse_position.width, self.mouse_position.height)
                } else {
                    let adjusted_drag_left =
                        self.mouse_position.width + (window_left / self.renderer.font_width) as u32;
                    let adjusted_drag_top = self.mouse_position.height
                        + (window_top / self.renderer.font_height) as u32;
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
                    position: (self.mouse_position.width, self.mouse_position.height),
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
                    position: (self.mouse_position.width, self.mouse_position.height),
                })
                .ok();
        }
        self.mouse_down = false;
    }

    pub fn handle_mouse_wheel(&mut self, x: i32, y: i32) {
        if !self.mouse_enabled {
            return;
        }

        let vertical_input_type = match y {
            _ if y > 0 => Some("up"),
            _ if y < 0 => Some("down"),
            _ => None,
        };

        if let Some(input_type) = vertical_input_type {
            self.ui_command_sender
                .send(UiCommand::Scroll {
                    direction: input_type.to_string(),
                    grid_id: self.grid_id_under_mouse,
                    position: (self.mouse_position.width, self.mouse_position.height),
                })
                .ok();
        }

        let horizontal_input_type = match y {
            _ if x > 0 => Some("right"),
            _ if x < 0 => Some("left"),
            _ => None,
        };

        if let Some(input_type) = horizontal_input_type {
            self.ui_command_sender
                .send(UiCommand::Scroll {
                    direction: input_type.to_string(),
                    grid_id: self.grid_id_under_mouse,
                    position: (self.mouse_position.width, self.mouse_position.height),
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

    fn handle_events(&mut self) {
        self.synchronize_settings();

        let mut keycode = None;
        let mut keytext = None;
        let mut ignore_text_this_frame = false;

        let window_events: Vec<Event> = self.event_pump.poll_iter().collect();
        for event in window_events.into_iter() {
            match event {
                Event::Quit { .. } => self.handle_quit(),
                Event::DropFile { filename, .. } => {
                    self.ui_command_sender
                        .send(UiCommand::FileDrop(filename))
                        .ok();
                }
                Event::KeyDown {
                    keycode: received_keycode,
                    ..
                } => {
                    keycode = received_keycode;
                }
                Event::TextInput { text, .. } => keytext = Some(text),
                Event::MouseMotion { x, y, .. } => self.handle_pointer_motion(x, y),
                Event::MouseButtonDown { .. } => self.handle_pointer_down(),
                Event::MouseButtonUp { .. } => self.handle_pointer_up(),
                Event::MouseWheel { x, y, .. } => self.handle_mouse_wheel(x, y),
                Event::Window {
                    win_event: WindowEvent::FocusLost,
                    ..
                } => self.handle_focus_lost(),
                Event::Window {
                    win_event: WindowEvent::FocusGained,
                    ..
                } => {
                    ignore_text_this_frame = true; // Ignore any text events on the first frame when focus is regained. https://github.com/Kethku/neovide/issues/193
                    self.handle_focus_gained();
                }
                Event::Window { .. } => REDRAW_SCHEDULER.queue_next_frame(),
                _ => {}
            }
        }

        if !ignore_text_this_frame {
            self.handle_keyboard_input(keycode, keytext);
        }

        let window_commands: Vec<WindowCommand> = self.window_command_receiver.try_iter().collect();
        for window_command in window_commands.into_iter() {
            match window_command {
                WindowCommand::TitleChanged(new_title) => self.handle_title_changed(new_title),
                WindowCommand::SetMouseEnabled(mouse_enabled) => self.mouse_enabled = mouse_enabled,
            }
        }
    }

    fn draw_frame(&mut self, dt: f32) -> VkResult<bool> {
        let sdl_window_wrapper = Sdl2Window::new(&self.window);
        let new_size = sdl_window_wrapper.logical_size();
        if self.previous_size != new_size {
            handle_new_grid_size(new_size, &self.renderer, &self.ui_command_sender);
            self.previous_size = new_size;
        }

        let current_size = self.previous_size;
        let ui_command_sender = self.ui_command_sender.clone();

        if REDRAW_SCHEDULER.should_draw() || SETTINGS.get::<WindowSettings>().no_idle {
            debug!("Render Triggered");

            let renderer = &mut self.renderer;
            self.skulpin_renderer.draw(
                &sdl_window_wrapper,
                |canvas, coordinate_system_helper| {
                    if renderer.draw_frame(canvas, &coordinate_system_helper, dt) {
                        handle_new_grid_size(current_size, &renderer, &ui_command_sender);
                    }
                },
            )?;

            Ok(true)
        } else {
            Ok(false)
        }
    }
}

pub fn start_loop(
    window_command_receiver: Receiver<WindowCommand>,
    ui_command_sender: TxUnbounded<UiCommand>,
    running: Arc<AtomicBool>,
    logical_size: LogicalSize,
    renderer: Renderer,
) {
    sdl2::hint::set("SDL_MOUSE_FOCUS_CLICKTHROUGH", "1");

    let context = sdl2::init().expect("Failed to initialize sdl2");
    let video_subsystem = context
        .video()
        .expect("Failed to create sdl video subsystem");
    video_subsystem.text_input().start();

    let sdl_window = video_subsystem
        .window("Neovide", logical_size.width, logical_size.height)
        .position_centered()
        .allow_highdpi()
        .resizable()
        .vulkan()
        .build()
        .expect("Failed to create window");

    let skulpin_renderer = {
        let sdl_window_wrapper = Sdl2Window::new(&sdl_window);
        RendererBuilder::new()
            .prefer_integrated_gpu()
            .use_vulkan_debug_layer(false)
            .present_mode_priority(vec![PresentMode::Immediate])
            .coordinate_system(CoordinateSystem::Logical)
            .build(&sdl_window_wrapper)
            .expect("Failed to create renderer")
    };

    let event_pump = context
        .event_pump()
        .expect("Could not create sdl event pump");

    let mut window_wrapper = Sdl2WindowWrapper {
        context,
        window: sdl_window,
        skulpin_renderer,
        renderer,
        event_pump,
        mouse_down: false,
        mouse_position: LogicalSize {
            width: 0,
            height: 0,
        },
        mouse_enabled: true,
        grid_id_under_mouse: 0,
        title: String::from("Neovide"),
        previous_size: logical_size,
        transparency: 1.0,
        fullscreen: false,
        cached_size: (0, 0),
        cached_position: (0, 0),
        ui_command_sender,
        window_command_receiver,
        running: running.clone(),
    };

    let mut was_animating = false;
    let mut previous_frame_start = Instant::now();
    loop {
        if !running.load(Ordering::Relaxed) {
            break;
        }

        let frame_start = Instant::now();

        let refresh_rate = { SETTINGS.get::<WindowSettings>().refresh_rate as f32 };
        let dt = if was_animating {
            previous_frame_start.elapsed().as_secs_f32()
        } else {
            1.0 / refresh_rate
        };

        window_wrapper.handle_events();

        match window_wrapper.draw_frame(dt) {
            Ok(animating) => {
                was_animating = animating;
            }
            Err(error) => {
                error!("Render failed: {}", error);
                break;
            }
        }

        let elapsed = frame_start.elapsed();
        let expected_frame_length = Duration::from_secs_f32(1.0 / refresh_rate);

        previous_frame_start = frame_start;

        if elapsed < expected_frame_length {
            sleep(expected_frame_length - elapsed);
        }
    }

    std::process::exit(0);
}
