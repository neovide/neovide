use std::thread::sleep;
use std::time::{Duration, Instant};
use std::sync::Arc;
use std::sync::atomic::{Ordering, AtomicBool};
use std::sync::mpsc::{Sender, Receiver};

use log::{debug, error, warn, info, trace};
use skulpin::sdl2;
use skulpin::sdl2::event::{Event, WindowEvent};
use skulpin::sdl2::keyboard::Keycode;
use skulpin::sdl2::video::FullscreenType;
use skulpin::sdl2::Sdl;
use skulpin::ash::prelude::VkResult;
use skulpin::{
    CoordinateSystem, LogicalSize, PhysicalSize, PresentMode, Renderer as SkulpinRenderer,
    RendererBuilder, Sdl2Window, Window,
};

use crate::editor::{DrawCommand, WindowCommand};
use crate::bridge::{produce_neovim_keybinding_string, UiCommand};
use crate::redraw_scheduler::REDRAW_SCHEDULER;
use crate::renderer::Renderer;
use crate::settings::*;
use crate::INITIAL_DIMENSIONS;
use crate::error_handling::ResultPanicExplanation;

#[derive(RustEmbed)]
#[folder = "assets/"]
struct Asset;

#[cfg(target_os = "windows")]
fn windows_fix_dpi() {
    use winapi::shared::windef::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2;
    use winapi::um::winuser::SetProcessDpiAwarenessContext;
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

fn handle_new_grid_size(new_size: LogicalSize, renderer: &Renderer, ui_command_sender: &Sender<UiCommand>) {
    if new_size.width > 0 && new_size.height > 0 {
        let new_width = ((new_size.width + 1) as f32 / renderer.font_width) as u32;
        let new_height = ((new_size.height + 1) as f32 / renderer.font_height) as u32;
        // Add 1 here to make sure resizing doesn't change the grid size on startup
        ui_command_sender.send(UiCommand::Resize {
            width: new_width,
            height: new_height,
        }).ok();
    }
}

struct WindowWrapper {
    context: Sdl,
    window: sdl2::video::Window,
    skulpin_renderer: SkulpinRenderer,
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
    ui_command_sender: Sender<UiCommand>,
    running: Arc<AtomicBool>
}

pub fn window_geometry() -> Result<(u64, u64), String> {
    let prefix = "--geometry=";

    std::env::args()
        .find(|arg| arg.starts_with(prefix))
        .map_or(Ok(INITIAL_DIMENSIONS), |arg| {
            let input = &arg[prefix.len()..];
            let invalid_parse_err = format!(
                "Invalid geometry: {}\nValid format: <width>x<height>",
                input
            );

            input
                .split('x')
                .map(|dimension| {
                    dimension
                        .parse::<u64>()
                        .map_err(|_| invalid_parse_err.as_str())
                        .and_then(|dimension| {
                            if dimension > 0 {
                                Ok(dimension)
                            } else {
                                Err("Invalid geometry: Window dimensions should be greater than 0.")
                            }
                        })
                })
                .collect::<Result<Vec<_>, &str>>()
                .and_then(|dimensions| {
                    if let [width, height] = dimensions[..] {
                        Ok((width, height))
                    } else {
                        Err(invalid_parse_err.as_str())
                    }
                })
                .map_err(|msg| msg.to_owned())
        })
}

pub fn window_geometry_or_default() -> (u64, u64) {
    window_geometry().unwrap_or(INITIAL_DIMENSIONS)
}

impl WindowWrapper {
    pub fn new(ui_command_sender: Sender<UiCommand>, draw_command_receiver: Receiver<DrawCommand>, running: Arc<AtomicBool>) -> WindowWrapper {
        let context = sdl2::init().expect("Failed to initialize sdl2");
        let video_subsystem = context
            .video()
            .expect("Failed to create sdl video subsystem");
        video_subsystem.text_input().start();

        let (width, height) = window_geometry_or_default();

        let renderer = Renderer::new(draw_command_receiver);
        let logical_size = LogicalSize {
            width: (width as f32 * renderer.font_width) as u32,
            height: (height as f32 * renderer.font_height + 1.0) as u32,
        };

        #[cfg(target_os = "windows")]
        windows_fix_dpi();
        sdl2::hint::set("SDL_MOUSE_FOCUS_CLICKTHROUGH", "1");

        let sdl_window = video_subsystem
            .window("Neovide", logical_size.width, logical_size.height)
            .position_centered()
            .allow_highdpi()
            .resizable()
            .vulkan()
            .build()
            .expect("Failed to create window");
        info!("window created");

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

        info!("renderer created");

        WindowWrapper {
            context,
            window: sdl_window,
            skulpin_renderer,
            renderer,
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
            running
        }
    }

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
            self.ui_command_sender.send(UiCommand::Keyboard(keybinding_string)).unwrap_or_explained_panic(
                "Could not send UI command from the window system to the neovim process.",
            );
        }
    }

    pub fn handle_pointer_motion(&mut self, x: i32, y: i32) {
        let previous_position = self.mouse_position;
        let sdl_window_wrapper = Sdl2Window::new(&self.window);
        let logical_position = PhysicalSize::new(x as u32, y as u32)
            .to_logical(sdl_window_wrapper.scale_factor());

        let mut top_window_position = (0.0, 0.0);
        let mut top_grid_position = None;

        for (grid_id, window_region) in self.renderer.window_regions.iter() {
            if logical_position.width >= window_region.left as u32 && logical_position.width < window_region.right as u32 &&
                logical_position.height >= window_region.top as u32 && logical_position.height < window_region.bottom as u32 {
                top_window_position = (window_region.left, window_region.top);
                top_grid_position = Some((
                    grid_id, 
                    LogicalSize::new(logical_position.width - window_region.left as u32, logical_position.height - window_region.top as u32)
                ));
            }
        }

        if let Some((grid_id, grid_position)) = top_grid_position {
            self.grid_id_under_mouse = *grid_id;
            self.mouse_position = LogicalSize::new(
                (grid_position.width as f32 / self.renderer.font_width) as u32,
                (grid_position.height as f32 / self.renderer.font_height) as u32
            );

            if self.mouse_enabled && self.mouse_down && previous_position != self.mouse_position {
                let (window_left, window_top) = top_window_position;
                let adjusted_drag_left = self.mouse_position.width + (window_left / self.renderer.font_width) as u32;
                let adjusted_drag_top = self.mouse_position.height + (window_top / self.renderer.font_height) as u32;

                self.ui_command_sender.send(UiCommand::Drag {
                    grid_id: self.grid_id_under_mouse,
                    position: (adjusted_drag_left, adjusted_drag_top),
                }).ok();
            }
        }
    }

    pub fn handle_pointer_down(&mut self) {
        if self.mouse_enabled {
            self.ui_command_sender.send(UiCommand::MouseButton {
                action: String::from("press"),
                grid_id: self.grid_id_under_mouse,
                position: (self.mouse_position.width, self.mouse_position.height),
            }).ok();
        }
        self.mouse_down = true;
    }

    pub fn handle_pointer_up(&mut self) {
        if self.mouse_enabled {
            self.ui_command_sender.send(UiCommand::MouseButton {
                action: String::from("release"),
                grid_id: self.grid_id_under_mouse,
                position: (self.mouse_position.width, self.mouse_position.height),
            }).ok();
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
            self.ui_command_sender.send(UiCommand::Scroll {
                direction: input_type.to_string(),
                grid_id: self.grid_id_under_mouse,
                position: (self.mouse_position.width, self.mouse_position.height),
            }).ok();
        }

        let horizontal_input_type = match y {
            _ if x > 0 => Some("right"),
            _ if x < 0 => Some("left"),
            _ => None,
        };

        if let Some(input_type) = horizontal_input_type {
            self.ui_command_sender.send(UiCommand::Scroll {
                direction: input_type.to_string(),
                grid_id: self.grid_id_under_mouse,
                position: (self.mouse_position.width, self.mouse_position.height),
            }).ok();
        }
    }

    pub fn handle_focus_lost(&mut self) {
        self.ui_command_sender.send(UiCommand::FocusLost).ok();
    }

    pub fn handle_focus_gained(&mut self) {
        self.ui_command_sender.send(UiCommand::FocusGained).ok();
        REDRAW_SCHEDULER.queue_next_frame();
    }

    pub fn draw_frame(&mut self, dt: f32) -> VkResult<bool> {
        let sdl_window_wrapper = Sdl2Window::new(&self.window);
        let new_size = sdl_window_wrapper.logical_size();
        if self.previous_size != new_size {
            handle_new_grid_size(new_size, &self.renderer, &self.ui_command_sender);
            self.previous_size = new_size;
        }

        debug!("Render Triggered");

        if REDRAW_SCHEDULER.should_draw() || SETTINGS.get::<WindowSettings>().no_idle {
            let renderer = &mut self.renderer;
            self.skulpin_renderer
                .draw(&sdl_window_wrapper, |canvas, coordinate_system_helper| {
                    renderer.draw_frame(canvas, &coordinate_system_helper, dt) 
                })?;

            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[derive(Clone)]
struct WindowSettings {
    refresh_rate: u64,
    transparency: f32,
    no_idle: bool,
    fullscreen: bool,
}

pub fn initialize_settings() {
    let no_idle = SETTINGS
        .neovim_arguments
        .contains(&String::from("--noIdle"));

    SETTINGS.set(&WindowSettings {
        refresh_rate: 60,
        transparency: 1.0,
        no_idle,
        fullscreen: false,
    });

    register_nvim_setting!("refresh_rate", WindowSettings::refresh_rate);
    register_nvim_setting!("transparency", WindowSettings::transparency);
    register_nvim_setting!("no_idle", WindowSettings::no_idle);
    register_nvim_setting!("fullscreen", WindowSettings::fullscreen);
}

pub fn start_window(draw_command_receiver: Receiver<DrawCommand>, window_command_receiver: Receiver<WindowCommand>, ui_command_sender: Sender<UiCommand>, running: Arc<AtomicBool>) {
    let mut window = WindowWrapper::new(ui_command_sender.clone(), draw_command_receiver, running.clone());

    info!("Starting window event loop");
    let mut event_pump = window
        .context
        .event_pump()
        .expect("Could not create sdl event pump");

    let mut was_animating = false;
    let mut previous_frame_start = Instant::now();
    loop {
        if !running.load(Ordering::Relaxed) {
            break;
        }

        let frame_start = Instant::now();

        window.synchronize_settings();

        let mut keycode = None;
        let mut keytext = None;
        let mut ignore_text_this_frame = false;

        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => window.handle_quit(),
                Event::DropFile { filename, .. } => {
                    ui_command_sender.send(UiCommand::FileDrop(filename)).ok();
                }
                Event::KeyDown {
                    keycode: received_keycode,
                    ..
                } => {
                    keycode = received_keycode;
                }
                Event::TextInput { text, .. } => keytext = Some(text),
                Event::MouseMotion { x, y, .. } => window.handle_pointer_motion(x, y),
                Event::MouseButtonDown { .. } => window.handle_pointer_down(),
                Event::MouseButtonUp { .. } => window.handle_pointer_up(),
                Event::MouseWheel { x, y, .. } => window.handle_mouse_wheel(x, y),
                Event::Window {
                    win_event: WindowEvent::FocusLost,
                    ..
                } => window.handle_focus_lost(),
                Event::Window {
                    win_event: WindowEvent::FocusGained,
                    ..
                } => {
                    ignore_text_this_frame = true; // Ignore any text events on the first frame when focus is regained. https://github.com/Kethku/neovide/issues/193
                    window.handle_focus_gained();
                }
                Event::Window { .. } => REDRAW_SCHEDULER.queue_next_frame(),
                _ => {}
            }
        }

        if !ignore_text_this_frame {
            window.handle_keyboard_input(keycode, keytext);
        }

        let refresh_rate = { SETTINGS.get::<WindowSettings>().refresh_rate as f32 };
        let dt = if was_animating {
            previous_frame_start.elapsed().as_secs_f32()
        } else {
            1.0 / refresh_rate
        };

        for window_command in window_command_receiver.try_iter() {
            match window_command {
                WindowCommand::TitleChanged(new_title) => window.handle_title_changed(new_title),
                WindowCommand::SetMouseEnabled(mouse_enabled) => window.mouse_enabled = mouse_enabled,
            }
        }

        match window.draw_frame(dt) {
            Ok(animating) => {
                was_animating = animating;
            },
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
