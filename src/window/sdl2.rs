use std::sync::atomic::Ordering;
use std::thread::sleep;
use std::time::{Duration, Instant};

use log::{error, info, trace};
use skulpin::sdl2;
use skulpin::sdl2::event::{Event, WindowEvent};
use skulpin::sdl2::keyboard::KeyboardUtil;
use skulpin::sdl2::keyboard::Keycode;
use skulpin::sdl2::video::FullscreenType;
use skulpin::sdl2::video::Window;
use skulpin::sdl2::EventPump;
use skulpin::{
    CoordinateSystem, LogicalSize, PhysicalSize, PresentMode, Renderer as SkulpinRenderer,
    RendererBuilder, Sdl2Window, Window as OtherWindow,
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
        let new_width = (new_size.width as f32 / renderer.font_width) as u32;
        let new_height = (new_size.height as f32 / renderer.font_height) as u32;
        // Add 1 here to make sure resizing doesn't change the grid size on startup
        BRIDGE.queue_command(UiCommand::Resize {
            width: new_width,
            height: new_height,
        });
    }
}

struct WindowHelper {
    renderer: Renderer,
    keyboard: Option<KeyboardUtil>,
    mouse_down: bool,
    mouse_position: LogicalSize,
    logical_size: LogicalSize,
    // previous_position: (i32, i32),
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

impl WindowHelper {
    pub fn new() -> WindowHelper {
        let (width, height) = window_geometry_or_default();
        let renderer = Renderer::new();
        let logical_size = LogicalSize {
            width: (width as f32 * renderer.font_width) as u32,
            height: (height as f32 * renderer.font_height) as u32,
        };

        WindowHelper {
            renderer,
            keyboard: None,
            mouse_down: false,
            mouse_position: LogicalSize {
                width: 0,
                height: 0,
            },
            logical_size,
            // previous_position: (0, 0),
        }
    }

    pub fn handle_quit(&mut self) {
        BRIDGE.queue_command(UiCommand::Quit);
    }

    pub fn handle_keyboard_input(&mut self, keycode: Option<Keycode>, text: Option<String>) {
        let modifiers = self.keyboard.as_ref().unwrap().mod_state();

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

    pub fn handle_pointer_motion(&mut self, x: i32, y: i32, window: &Window) {
        let previous_position = self.mouse_position;
        let physical_size = PhysicalSize::new(
            (x as f32 / self.renderer.font_width) as u32,
            (y as f32 / self.renderer.font_height) as u32,
        );

        let sdl_window_wrapper = Sdl2Window::new(window);
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

    pub fn process_editor_events(&mut self, event_pump: &mut EventPump, window: &mut Window) {
        let mut keyboard_inputs = Vec::new();
        let mut keycode = None;
        let mut keytext = None;
        let mut ignore_text_this_frame = false;

        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => self.handle_quit(),
                Event::DropFile { filename, .. } => {
                    BRIDGE.queue_command(UiCommand::FileDrop(filename));
                }
                Event::KeyDown {
                    keycode: received_keycode,
                    ..
                } => {
                    // If keycode has a value, add it to the list as the new keycode supercedes
                    // this one.
                    if keycode.is_some() {
                        keyboard_inputs.push((keycode, None));
                    }

                    keycode = received_keycode;
                }
                Event::TextInput { text, .. } => {
                    // If keycode has a value, add it to the list as the new keycode supercedes
                    // this one.
                    if keytext.is_some() {
                        keyboard_inputs.push((None, keytext));
                    }

                    keytext = Some(text);
                }
                Event::MouseMotion { x, y, .. } => self.handle_pointer_motion(x, y, window),
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
                Event::Window {
                    win_event: WindowEvent::Resized(width, height),
                    ..
                } => {
                    let new_size = LogicalSize::new(width as u32, height as u32);
                    handle_new_grid_size(new_size, &self.renderer);
                    self.logical_size = new_size;
                }
                Event::Window { .. } => REDRAW_SCHEDULER.queue_next_frame(),
                _ => {}
            }
        }

        keyboard_inputs.push((keycode, keytext));

        if !ignore_text_this_frame {
            for (keycode, keytext) in keyboard_inputs.into_iter() {
                self.handle_keyboard_input(keycode, keytext);
            }
        }
    }

    pub fn should_draw(&self) -> bool {
        REDRAW_SCHEDULER.should_draw() || SETTINGS.get::<WindowSettings>().no_idle
    }

    pub fn draw_frame(
        &mut self,
        window: &mut Sdl2Window,
        skulpin_renderer: &mut SkulpinRenderer,
    ) -> bool {
        if !BRIDGE.running.load(Ordering::Relaxed) {
            return false;
        }
        if self.should_draw() {
            let renderer = &mut self.renderer;
            let error = skulpin_renderer
                .draw(window, |canvas, coordinate_system_helper| {
                    let dt = 1.0 / (SETTINGS.get::<WindowSettings>().refresh_rate as f32);
                    renderer.draw(canvas, &coordinate_system_helper, dt);
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

pub fn toggle_fullscreen(window: &mut Window) {
    match window.fullscreen_state() {
        FullscreenType::Off => {
            if cfg!(target_os = "windows") {
                let video_subsystem = window.subsystem();
                if let Ok(rect) = window
                    .display_index()
                    .and_then(|index| video_subsystem.display_bounds(index))
                {
                    // Set window to fullscreen
                    unsafe {
                        let raw_handle = window.raw();
                        sdl2::sys::SDL_SetWindowResizable(
                            raw_handle,
                            sdl2::sys::SDL_bool::SDL_FALSE,
                        );
                    }
                    window.set_size(rect.width(), rect.height()).unwrap();
                    window.set_position(
                        sdl2::video::WindowPos::Positioned(rect.x()),
                        sdl2::video::WindowPos::Positioned(rect.y()),
                    );
                }
            } else {
                window.set_fullscreen(FullscreenType::Desktop).ok();
            }
        }
        _ => {
            if cfg!(target_os = "windows") {
                unsafe {
                    let raw_handle = window.raw();
                    sdl2::sys::SDL_SetWindowResizable(raw_handle, sdl2::sys::SDL_bool::SDL_TRUE);
                }
            } else {
                window.set_fullscreen(FullscreenType::Off).ok();
            }
        }
    }
}

pub fn synchronize_settings(window: &mut Window) {
    let editor_title = { EDITOR.lock().title.clone() };

    if window.title() != editor_title {
        window
            .set_title(&editor_title)
            .expect("Could not set title");
    }

    if let Ok(opacity) = window.opacity() {
        let transparency = { SETTINGS.get::<WindowSettings>().transparency };
        if (opacity - transparency).abs() > std::f32::EPSILON {
            window.set_opacity(transparency).ok();
        }
    }

    let fullscreen_state = if SETTINGS.get::<WindowSettings>().fullscreen {
        FullscreenType::Desktop
    } else {
        FullscreenType::Off
    };
    if window.fullscreen_state() != fullscreen_state {
        println!("{:?}", fullscreen_state);
        toggle_fullscreen(window);
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
    let mut window_helper = WindowHelper::new();
    let context = sdl2::init().expect("Failed to initialize sdl2");
    let video_subsystem = context
        .video()
        .expect("Failed to create sdl video subsystem");
    video_subsystem.text_input().start();

    window_helper.keyboard = Some(context.keyboard());

    #[cfg(target_os = "windows")]
    windows_fix_dpi();
    sdl2::hint::set("SDL_MOUSE_FOCUS_CLICKTHROUGH", "1");

    let mut sdl_window = video_subsystem
        .window(
            "Neovide",
            window_helper.logical_size.width,
            window_helper.logical_size.height,
        )
        .position_centered()
        .allow_highdpi()
        .resizable()
        .vulkan()
        .build()
        .expect("Failed to create window");
    info!("window created");

    let mut skulpin_renderer = {
        let sdl_window_wrapper = Sdl2Window::new(&sdl_window);
        RendererBuilder::new()
            .prefer_integrated_gpu()
            .use_vulkan_debug_layer(false)
            .present_mode_priority(vec![PresentMode::Immediate])
            .coordinate_system(CoordinateSystem::Logical)
            .build(&sdl_window_wrapper)
            .expect("Failed to create renderer")
    };

    info!("Starting window event loop");
    let mut event_pump = context
        .event_pump()
        .expect("Could not create sdl event pump");

    loop {
        let frame_start = Instant::now();
        synchronize_settings(&mut sdl_window);

        window_helper.process_editor_events(&mut event_pump, &mut sdl_window);
        let mut sdl_window_wrapper = Sdl2Window::new(&sdl_window);
        if !window_helper.draw_frame(&mut sdl_window_wrapper, &mut skulpin_renderer) {
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
