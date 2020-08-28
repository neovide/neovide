use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use image::{load_from_memory, GenericImageView, Pixel};
use log::{debug, error, info, trace};
use skulpin::winit;
use skulpin::winit::event::VirtualKeyCode as Keycode;
use skulpin::winit::event::{
    ElementState, Event, ModifiersState, MouseButton, MouseScrollDelta, WindowEvent,
};
use skulpin::winit::event_loop::{ControlFlow, EventLoop};
use skulpin::winit::window::{Fullscreen, Icon};
use skulpin::{
    winit::dpi::{LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize},
    CoordinateSystem, PresentMode, Renderer as SkulpinRenderer, RendererBuilder, Window,
    WinitWindow,
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

fn handle_new_grid_size(new_size: LogicalSize<u32>, renderer: &Renderer) {
    let new_width = ((new_size.width + 1) as f32 / renderer.font_width) as u32;
    let new_height = ((new_size.height + 1) as f32 / renderer.font_height) as u32;
    // Add 1 here to make sure resizing doesn't change the grid size on startup
    BRIDGE.queue_command(UiCommand::Resize {
        width: new_width,
        height: new_height,
    });
}

struct WindowWrapper {
    window: winit::window::Window,
    skulpin_renderer: SkulpinRenderer,
    renderer: Renderer,
    mouse_down: bool,
    mouse_position: LogicalPosition<u32>,
    title: String,
    previous_size: PhysicalSize<u32>,
    fullscreen: bool,
    cached_size: PhysicalSize<u32>,
    cached_position: PhysicalPosition<i32>,
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
    pub fn new(event_loop: &EventLoop<()>) -> WindowWrapper {
        let renderer = Renderer::new();

        let (width, height) = window_geometry_or_default();
        let logical_size = LogicalSize {
            width: (width as f32 * renderer.font_width) as u32,
            height: (height as f32 * renderer.font_height + 1.0) as u32,
        };

        #[cfg(target_os = "windows")]
        windows_fix_dpi();

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
        info!("icon created");

        let winit_window = winit::window::WindowBuilder::new()
            .with_title("Neovide")
            .with_inner_size(logical_size)
            .with_window_icon(Some(icon))
            .build(event_loop)
            .expect("Failed to create window");
        info!("window created");

        let scale_factor = winit_window.scale_factor();

        let skulpin_renderer = {
            let winit_window_wrapper = WinitWindow::new(&winit_window);
            RendererBuilder::new()
                .prefer_integrated_gpu()
                .use_vulkan_debug_layer(false)
                .present_mode_priority(vec![PresentMode::Immediate])
                .coordinate_system(CoordinateSystem::Logical)
                .build(&winit_window_wrapper)
                .expect("Failed to create renderer")
        };

        info!("renderer created");

        let saved_position = LogicalPosition { x: 0, y: 0 };

        WindowWrapper {
            window: winit_window,
            skulpin_renderer,
            renderer,
            mouse_down: false,
            mouse_position: saved_position,
            title: String::from("Neovide"),
            previous_size: logical_size.to_physical(scale_factor),
            fullscreen: false,
            cached_size: PhysicalSize {
                width: 0,
                height: 0,
            },
            cached_position: saved_position.to_physical(scale_factor),
        }
    }

    pub fn toggle_fullscreen(&mut self) {
        if self.fullscreen {
            self.window.set_fullscreen(None);

            // Use cached size and position
            self.window.set_inner_size(self.cached_size);
            self.window.set_outer_position(self.cached_position);
        } else {
            self.cached_size = self.window.inner_size();
            self.cached_position = self.window.outer_position().unwrap();
            let handle = self.window.current_monitor();
            self.window
                .set_fullscreen(Some(Fullscreen::Borderless(handle)));
        }

        self.fullscreen = !self.fullscreen;
    }

    pub fn synchronize_settings(&mut self) {
        let editor_title = { EDITOR.lock().title.clone() };

        if self.title != editor_title {
            self.title = editor_title;
            self.window.set_title(&self.title);
        }

        let fullscreen = { SETTINGS.get::<WindowSettings>().fullscreen };

        if self.fullscreen != fullscreen {
            self.toggle_fullscreen();
        }
    }

    pub fn handle_quit(&mut self) {
        BRIDGE.queue_command(UiCommand::Quit);
    }

    pub fn handle_keyboard_input(
        &mut self,
        keycode: Option<Keycode>,
        modifiers: Option<ModifiersState>,
    ) {
        if keycode.is_some() {
            trace!("Keyboard Input Received: keycode-{:?}", keycode);
        }

        if let Some(keybinding_string) = produce_neovim_keybinding_string(keycode, None, modifiers)
        {
            BRIDGE.queue_command(UiCommand::Keyboard(keybinding_string));
        }
    }

    pub fn handle_pointer_motion(&mut self, position: PhysicalPosition<f64>) {
        let previous_position = self.mouse_position;
        let physical_position = PhysicalPosition::new(
            (position.x as f32 / self.renderer.font_width) as u32,
            (position.y as f32 / self.renderer.font_height) as u32,
        );

        let winit_window_wrapper = WinitWindow::new(&self.window);
        self.mouse_position = physical_position.to_logical(winit_window_wrapper.scale_factor());
        if self.mouse_down && previous_position != self.mouse_position {
            BRIDGE.queue_command(UiCommand::Drag(
                self.mouse_position.x,
                self.mouse_position.y,
            ));
        }
    }

    pub fn handle_pointer_down(&mut self) {
        BRIDGE.queue_command(UiCommand::MouseButton {
            action: String::from("press"),
            position: (self.mouse_position.x, self.mouse_position.y),
        });
        self.mouse_down = true;
    }

    pub fn handle_pointer_up(&mut self) {
        BRIDGE.queue_command(UiCommand::MouseButton {
            action: String::from("release"),
            position: (self.mouse_position.x, self.mouse_position.y),
        });
        self.mouse_down = false;
    }

    pub fn handle_mouse_wheel(&mut self, x: f32, y: f32) {
        let vertical_input_type = match y {
            _ if y > 0.0 => Some("up"),
            _ if y < 0.0 => Some("down"),
            _ => None,
        };

        if let Some(input_type) = vertical_input_type {
            BRIDGE.queue_command(UiCommand::Scroll {
                direction: input_type.to_string(),
                position: (self.mouse_position.x, self.mouse_position.y),
            });
        }

        let horizontal_input_type = match y {
            _ if x > 0.0 => Some("right"),
            _ if x < 0.0 => Some("left"),
            _ => None,
        };

        if let Some(input_type) = horizontal_input_type {
            BRIDGE.queue_command(UiCommand::Scroll {
                direction: input_type.to_string(),
                position: (self.mouse_position.x, self.mouse_position.y),
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

        let scale_factor = self.window.scale_factor();
        let new_size = self.window.inner_size();
        if self.previous_size != new_size {
            handle_new_grid_size(new_size.to_logical(scale_factor), &self.renderer);
            self.previous_size = new_size;
        }

        debug!("Render Triggered");

        let current_size = self.previous_size;

        if REDRAW_SCHEDULER.should_draw() || SETTINGS.get::<WindowSettings>().no_idle {
            let winit_window_wrapper = WinitWindow::new(&self.window);
            let renderer = &mut self.renderer;
            let error = self
                .skulpin_renderer
                .draw(&winit_window_wrapper, |canvas, coordinate_system_helper| {
                    let dt = 1.0 / (SETTINGS.get::<WindowSettings>().refresh_rate as f32);

                    if renderer.draw(canvas, &coordinate_system_helper, dt) {
                        handle_new_grid_size(current_size.to_logical(scale_factor), &renderer)
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
    let event_loop = EventLoop::<()>::with_user_event();
    let mut window = WindowWrapper::new(&event_loop);
    event_loop.run(move |e, _window_target, control_flow| {
        let frame_start = Instant::now();

        window.synchronize_settings();

        let mut keycode = None;
        let mut modifiers = None;
        let mut ignore_text_this_frame = false;

        match e {
            Event::LoopDestroyed => {
                std::process::exit(0);
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                window.handle_quit();
                *control_flow = ControlFlow::Exit;
            }
            Event::WindowEvent {
                event: WindowEvent::DroppedFile(path),
                ..
            } => {
                BRIDGE.queue_command(UiCommand::FileDrop(
                    path.into_os_string().into_string().unwrap(),
                ));
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
                modifiers = Some(m);
            }
            Event::WindowEvent {
                event: WindowEvent::CursorMoved { position, .. },
                ..
            } => window.handle_pointer_motion(position),
            Event::WindowEvent {
                event:
                    WindowEvent::MouseWheel {
                        delta: MouseScrollDelta::LineDelta(x, y),
                        ..
                    },
                ..
            } => window.handle_mouse_wheel(x, y),

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
                    window.handle_pointer_down();
                } else {
                    window.handle_pointer_up();
                }
            }
            Event::WindowEvent {
                event: WindowEvent::Focused(focus),
                ..
            } => {
                if focus {
                    ignore_text_this_frame = true; // Ignore any text events on the first frame when focus is regained. https://github.com/Kethku/neovide/issues/193
                    window.handle_focus_gained();
                } else {
                    window.handle_focus_lost();
                }
            }
            Event::WindowEvent { .. } => REDRAW_SCHEDULER.queue_next_frame(),
            _ => {}
        }

        if !ignore_text_this_frame {
            window.handle_keyboard_input(keycode, modifiers);
        }

        if !window.draw_frame() {
            *control_flow = ControlFlow::Exit;
        }

        if *control_flow != ControlFlow::Exit {
            let elapsed = frame_start.elapsed();
            let refresh_rate = { SETTINGS.get::<WindowSettings>().refresh_rate as f32 };
            let frame_length = Duration::from_secs_f32(1.0 / refresh_rate);

            if elapsed < frame_length {
                *control_flow = ControlFlow::WaitUntil(Instant::now() + frame_length);
            }
        }
    });
}
