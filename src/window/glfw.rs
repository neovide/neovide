use std::sync::atomic::Ordering;
use std::thread::sleep;
use std::time::{Duration, Instant};

use std::sync::mpsc::Receiver;

use log::{error, info, trace};
use skulpin::glfw;
use skulpin::glfw::Glfw;
use skulpin::glfw::Key as Keycode;
use skulpin::glfw::{Action, WindowEvent};
use skulpin::{
    CoordinateSystem, GlfwWindow, LogicalSize, PresentMode,
    Renderer as SkulpinRenderer, RendererBuilder, Window,
};

use skulpin::glfw::Modifiers as Mod;

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
    context: Glfw,
    window: glfw::Window,
    events: Receiver<(f64, WindowEvent)>,
    skulpin_renderer: SkulpinRenderer,
    renderer: Renderer,
    title: String,
    previous_size: LogicalSize,
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
        let context = glfw::init(glfw::FAIL_ON_ERRORS).expect("Failed to initialize glfw");

        let (width, height) = window_geometry_or_default();

        let renderer = Renderer::new();
        let logical_size = LogicalSize {
            width: (width as f32 * renderer.font_width) as u32,
            height: (height as f32 * renderer.font_height + 1.0) as u32,
        };

        #[cfg(target_os = "windows")]
        windows_fix_dpi();

        let (mut glfw_window, events) = context
            .create_window(
                logical_size.width,
                logical_size.height,
                "Neovide",
                glfw::WindowMode::Windowed,
            )
            .expect("Failed to create GLFW window.");

        glfw_window.set_key_polling(true);
        glfw_window.set_char_polling(true);
        glfw_window.set_close_polling(true);

        let skulpin_renderer = {
            let sdl_window_wrapper = GlfwWindow::new(&glfw_window);
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
            window: glfw_window,
            events,
            skulpin_renderer,
            renderer,
            title: String::from("Neovide"),
            previous_size: logical_size,
        }
    }

    pub fn synchronize_settings(&mut self) {
        let editor_title = { EDITOR.lock().title.clone() };

        if self.title != editor_title {
            self.title = editor_title;
            self.window.set_title(&self.title);
        }

        let _transparency = { SETTINGS.get::<WindowSettings>().transparency };

        let _fullscreen = { SETTINGS.get::<WindowSettings>().fullscreen };
    }

    pub fn handle_quit(&mut self) {
        BRIDGE.queue_command(UiCommand::Quit);
    }

    pub fn handle_keyboard_input(
        &mut self,
        keycode: Option<Keycode>,
        text: Option<String>,
        modifiers: Option<Mod>,
    ) {
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

    pub fn draw_frame(&mut self) -> bool {
        if !BRIDGE.running.load(Ordering::Relaxed) {
            return false;
        }

        let sdl_window_wrapper = GlfwWindow::new(&self.window);
        let new_size = sdl_window_wrapper.logical_size();
        if self.previous_size != new_size {
            handle_new_grid_size(new_size, &self.renderer);
            self.previous_size = new_size;
        }

        // debug!("Render Triggered");

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

    loop {
        let frame_start = Instant::now();

        window.synchronize_settings();

        let mut keyboard_inputs = Vec::new();

        let mut keycode = None;
        let mut keytext = None;
        let mut modifiers = None;
        let mut close = false;

        window.context.poll_events();

        for (_, event) in glfw::flush_messages(&window.events) {
            match event {
                WindowEvent::Close => {
                    close = true;
                }
                WindowEvent::Key(key, _scancode, action, mods) => {
                    if action == Action::Press || action == Action::Repeat {
                        // If keycode has a value, add it to the list as the new keycode supercedes
                        // this one.
                        if keycode.is_some() {
                            keyboard_inputs.push((keycode, None));
                        }

                        modifiers = Some(mods);

                        keycode = Some(key);
                    }
                }
                WindowEvent::Char(char) => {
                    // If keytext has a value, add it to the list as the new keytext supercedes
                    // this one.
                    if keytext.is_some() {
                        keyboard_inputs.push((None, keytext));
                    }

                    keytext = Some(char.to_string());
                }
                _ => {}
            }

            // If both keycode and keytext have values, then add them to the list and reset the
            // variables.
            if keycode.is_some() && keytext.is_some() {
                keyboard_inputs.push((keycode, keytext));
                keycode = None;
                keytext = None;
            }
        }

        if close {
            window.handle_quit();
        }

        keyboard_inputs.push((keycode, keytext));

        for (keycode, keytext) in keyboard_inputs.into_iter() {
            window.handle_keyboard_input(keycode, keytext, modifiers);
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
