use std::sync::atomic::Ordering;
use std::thread::sleep;
use std::time::{Duration, Instant};

use log::{debug, error, info, trace};
use skulpin::sdl2;
use skulpin::sdl2::event::{Event, WindowEvent};
use skulpin::sdl2::keyboard::Keycode;
use skulpin::sdl2::video::FullscreenType;
use skulpin::sdl2::Sdl;
use skulpin::{
    CoordinateSystem, LogicalSize, PhysicalSize, PresentMode, Renderer as SkulpinRenderer,
    RendererBuilder, Sdl2Window, Window,
};

use crate::bridge::{produce_neovim_keybinding_string, UiCommand, BRIDGE};
use crate::editor::EDITOR;
use crate::redraw_scheduler::REDRAW_SCHEDULER;
use crate::renderer::Renderer;
use crate::settings::*;
use crate::INITIAL_DIMENSIONS;

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

fn handle_new_grid_size(new_size: LogicalSize, renderer: &Renderer) {
    if new_size.width > 0 && new_size.height > 0 {
        let new_width = ((new_size.width + 1) as f32 / renderer.font_width) as u32;
        let new_height = ((new_size.height + 1) as f32 / renderer.font_height) as u32;
        // Add 1 here to make sure resizing doesn't change the grid size on startup
        BRIDGE.queue_command(UiCommand::Resize {
            width: new_width,
            height: new_height,
        });
    }
}

struct WindowWrapper {
    context: Sdl,
    window: sdl2::video::Window,
    skulpin_renderer: SkulpinRenderer,
    renderer: Renderer,
    mouse_down: bool,
    mouse_position: LogicalSize,
    title: String,
    previous_size: LogicalSize,
    transparency: f32,
    fullscreen: bool,
    cached_size: (u32, u32),
    cached_position: (i32, i32),
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
    pub fn new() -> WindowWrapper {
        let context = sdl2::init().expect("Failed to initialize sdl2");
        let video_subsystem = context
            .video()
            .expect("Failed to create sdl video subsystem");
        video_subsystem.text_input().start();

        let (width, height) = window_geometry_or_default();

        let renderer = Renderer::new();
        let logical_size = LogicalSize {
            width: (width as f32 * renderer.font_width) as u32,
            height: (height as f32 * renderer.font_height + 1.0) as u32,
        };

        #[cfg(target_os = "windows")]
        windows_fix_dpi();
        sdl2::hint::set("SDL_MOUSE_FOCUS_CLICKTHROUGH", "1");

        // let icon = {
        //     let icon_data = Asset::get("nvim.ico").expect("Failed to read icon data");
        //     let icon = load_from_memory(&icon_data).expect("Failed to parse icon data");
        //     let (width, height) = icon.dimensions();
        //     let mut rgba = Vec::with_capacity((width * height) as usize * 4);
        //     for (_, _, pixel) in icon.pixels() {
        //         rgba.extend_from_slice(&pixel.to_rgba().0);
        //     }
        //     Icon::from_rgba(rgba, width, height).expect("Failed to create icon object")
        // };
        // info!("icon created");

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
            title: String::from("Neovide"),
            previous_size: logical_size,
            transparency: 1.0,
            fullscreen: false,
            cached_size: (0, 0),
            cached_position: (0, 0),
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
        let editor_title = { EDITOR.lock().title.clone() };

        if self.title != editor_title {
            self.title = editor_title;
            self.window
                .set_title(&self.title)
                .expect("Could not set title");
        }

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

    pub fn handle_quit(&mut self) {
        BRIDGE.queue_command(UiCommand::Quit);
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
            BRIDGE.queue_command(UiCommand::Keyboard(keybinding_string));
        }
    }

    pub fn handle_pointer_motion(&mut self, x: i32, y: i32) {
        let previous_position = self.mouse_position;
        let physical_size = PhysicalSize::new(
            (x as f32 / self.renderer.font_width) as u32,
            (y as f32 / self.renderer.font_height) as u32,
        );

        let sdl_window_wrapper = Sdl2Window::new(&self.window);
        self.mouse_position = physical_size.to_logical(sdl_window_wrapper.scale_factor());
        if self.mouse_down && previous_position != self.mouse_position {
            BRIDGE.queue_command(UiCommand::Drag(
                self.mouse_position.width,
                self.mouse_position.height,
            ));
        }
    }

    pub fn handle_pointer_down(&mut self) {
        BRIDGE.queue_command(UiCommand::MouseButton {
            action: String::from("press"),
            position: (self.mouse_position.width, self.mouse_position.height),
        });
        self.mouse_down = true;
    }

    pub fn handle_pointer_up(&mut self) {
        BRIDGE.queue_command(UiCommand::MouseButton {
            action: String::from("release"),
            position: (self.mouse_position.width, self.mouse_position.height),
        });
        self.mouse_down = false;
    }

    pub fn handle_mouse_wheel(&mut self, x: i32, y: i32) {
        let vertical_input_type = match y {
            _ if y > 0 => Some("up"),
            _ if y < 0 => Some("down"),
            _ => None,
        };

        if let Some(input_type) = vertical_input_type {
            BRIDGE.queue_command(UiCommand::Scroll {
                direction: input_type.to_string(),
                position: (self.mouse_position.width, self.mouse_position.height),
            });
        }

        let horizontal_input_type = match y {
            _ if x > 0 => Some("right"),
            _ if x < 0 => Some("left"),
            _ => None,
        };

        if let Some(input_type) = horizontal_input_type {
            BRIDGE.queue_command(UiCommand::Scroll {
                direction: input_type.to_string(),
                position: (self.mouse_position.width, self.mouse_position.height),
            });
        }
    }

    pub fn handle_focus_lost(&mut self) {
        BRIDGE.queue_command(UiCommand::FocusLost);
    }

    pub fn handle_focus_gained(&mut self) {
        BRIDGE.queue_command(UiCommand::FocusGained);
        REDRAW_SCHEDULER.queue_next_frame();
    }

    pub fn draw_frame(&mut self) -> bool {
        if !BRIDGE.running.load(Ordering::Relaxed) {
            return false;
        }

        let sdl_window_wrapper = Sdl2Window::new(&self.window);
        let new_size = sdl_window_wrapper.logical_size();
        if self.previous_size != new_size {
            handle_new_grid_size(new_size, &self.renderer);
            self.previous_size = new_size;
        }

        debug!("Render Triggered");

        let current_size = self.previous_size;

        if REDRAW_SCHEDULER.should_draw() || SETTINGS.get::<WindowSettings>().no_idle {
            let renderer = &mut self.renderer;
            let error = self
                .skulpin_renderer
                .draw(&sdl_window_wrapper, |canvas, coordinate_system_helper| {
                    let dt = 1.0 / (SETTINGS.get::<WindowSettings>().refresh_rate as f32);

                    if renderer.draw(canvas, &coordinate_system_helper, dt) {
                        handle_new_grid_size(current_size, &renderer)
                    }
                })
                .is_err();
            if error {
                error!("Render failed. Closing");
                return false;
            }
        }

        true
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

pub fn ui_loop() {
    let mut window = WindowWrapper::new();

    info!("Starting window event loop");
    let mut event_pump = window
        .context
        .event_pump()
        .expect("Could not create sdl event pump");

    loop {
        let frame_start = Instant::now();

        window.synchronize_settings();

        let mut keycode = None;
        let mut keytext = None;
        let mut ignore_text_this_frame = false;

        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => window.handle_quit(),
                Event::DropFile { filename, .. } => {
                    BRIDGE.queue_command(UiCommand::FileDrop(filename));
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

        if !window.draw_frame() {
            break;
        }

        let elapsed = frame_start.elapsed();
        let refresh_rate = { SETTINGS.get::<WindowSettings>().refresh_rate as f32 };
        let frame_length = Duration::from_secs_f32(1.0 / refresh_rate);

        if elapsed < frame_length {
            sleep(frame_length - elapsed);
        }
    }

    std::process::exit(0);
}
