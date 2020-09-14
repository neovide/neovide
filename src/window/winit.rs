use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use image::{load_from_memory, GenericImageView, Pixel};
use log::{error, info, trace};
use skulpin::winit::event::VirtualKeyCode as Keycode;
use skulpin::winit::event::{
    ElementState, Event, ModifiersState, MouseButton, MouseScrollDelta, StartCause, WindowEvent,
};
use skulpin::winit::event_loop::{ControlFlow, EventLoop};
use skulpin::winit::window::{Icon, Window};
use skulpin::{
    winit::dpi::{LogicalPosition, LogicalSize, PhysicalPosition},
    Renderer as SkulpinRenderer, Window as OtherWindow, WinitWindow,
};

use super::manager::*;
use crate::bridge::{produce_neovim_keybinding_string, UiCommand, BRIDGE};
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
    let new_width = (new_size.width as f32 / renderer.font_width) as u32;
    let new_height = (new_size.height as f32 / renderer.font_height) as u32;
    // Add 1 here to make sure resizing doesn't change the grid size on startup
    BRIDGE.queue_command(UiCommand::Resize {
        width: new_width,
        height: new_height,
    });
}

struct NeovideHandle {
    window: Option<Window>,
    renderer: Renderer,
    mouse_down: bool,
    mouse_position: LogicalPosition<u32>,
    keycode: Option<Keycode>,
    modifiers: Option<ModifiersState>,
    ignore_text_this_frame: bool,
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

impl NeovideHandle {
    pub fn handle_quit(&mut self) {
        BRIDGE.queue_command(UiCommand::Quit);
    }

    pub fn handle_keyboard_input(&mut self) {
        if self.keycode.is_some() {
            trace!("Keyboard Input Received: keycode-{:?}", self.keycode);

            if let Some(keybinding_string) =
                produce_neovim_keybinding_string(self.keycode, None, self.modifiers)
            {
                BRIDGE.queue_command(UiCommand::Keyboard(keybinding_string));
                self.keycode = None;
                self.modifiers = None;
            }
        }
    }

    pub fn handle_pointer_motion(&mut self, position: PhysicalPosition<f64>) {
        let previous_position = self.mouse_position;
        let physical_position = PhysicalPosition::new(
            (position.x as f32 / self.renderer.font_width) as u32,
            (position.y as f32 / self.renderer.font_height) as u32,
        );

        let winit_window_wrapper = WinitWindow::new(&self.window.as_ref().unwrap());
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
        self.ignore_text_this_frame = true; // Ignore any text events on the first frame when focus is regained. https://github.com/Kethku/neovide/issues/193
        BRIDGE.queue_command(UiCommand::FocusGained);
        REDRAW_SCHEDULER.queue_next_frame();
    }
}

impl Default for NeovideHandle {
    fn default() -> NeovideHandle {
        let renderer = Renderer::new();

        NeovideHandle {
            window: None,
            renderer,
            mouse_down: false,
            mouse_position: LogicalPosition { x: 0, y: 0 },
            keycode: None,
            modifiers: None,
            ignore_text_this_frame: false,
        }
    }
}

impl WindowHandle for NeovideHandle {
    fn window(&mut self) -> &Window {
        self.window.as_ref().unwrap()
    }

    fn set_window(&mut self, window: Window) {
        self.window = Some(window);
    }

    fn logical_size(&self) -> LogicalSize<u32> {
        let (width, height) = window_geometry_or_default();
        LogicalSize {
            width: (width as f32 * self.renderer.font_width) as u32,
            height: (height as f32 * self.renderer.font_height) as u32,
        }
    }

    fn update(&mut self) -> bool {
        if !self.ignore_text_this_frame {
            self.handle_keyboard_input();
        }
        true
    }

    fn should_draw(&self) -> bool {
        REDRAW_SCHEDULER.should_draw() || SETTINGS.get::<WindowSettings>().no_idle
    }

    fn draw(&mut self, skulpin_renderer: &mut SkulpinRenderer) -> bool {
        if !BRIDGE.running.load(Ordering::Relaxed) {
            return false;
        }
        if self.should_draw() {
            let renderer = &mut self.renderer;
            let window = WinitWindow::new(&self.window.as_ref().unwrap());
            let error = skulpin_renderer
                .draw(&window, |canvas, coordinate_system_helper| {
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

impl EventProcessor for NeovideHandle {
    fn process_event(&mut self, e: WindowEvent) -> Option<ControlFlow> {
        self.ignore_text_this_frame = false;

        match e {
            WindowEvent::CloseRequested => {
                self.handle_quit();
                return Some(ControlFlow::Exit);
            }
            WindowEvent::DroppedFile(path) => {
                BRIDGE.queue_command(UiCommand::FileDrop(
                    path.into_os_string().into_string().unwrap(),
                ));
            }
            WindowEvent::KeyboardInput { input, .. } => {
                if input.state == ElementState::Pressed {
                    self.keycode = input.virtual_keycode;
                }
            }
            WindowEvent::ModifiersChanged(m) => {
                self.modifiers = Some(m);
            }
            WindowEvent::CursorMoved { position, .. } => self.handle_pointer_motion(position),
            WindowEvent::MouseWheel {
                delta: MouseScrollDelta::LineDelta(x, y),
                ..
            } => self.handle_mouse_wheel(x, y),
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } => {
                if state == ElementState::Pressed {
                    self.handle_pointer_down();
                } else {
                    self.handle_pointer_up();
                }
            }
            WindowEvent::Focused(focus) => {
                if focus {
                    self.handle_focus_gained();
                } else {
                    self.handle_focus_lost();
                }
            }
            WindowEvent::Resized(size) => {
                let scale_factor = self.window.as_ref().unwrap().scale_factor();
                handle_new_grid_size(size.to_logical(scale_factor), &self.renderer);
            }
            _ => REDRAW_SCHEDULER.queue_next_frame(),
        }

        None
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

    let event_loop = EventLoop::<NeovideEvent>::with_user_event();
    let event_loop_proxy = event_loop.create_proxy();
    let mut window_manager: WindowManager<NeovideEvent> = WindowManager::new(event_loop_proxy);

    info!("renderer created");

    event_loop.run(move |e, window_target, control_flow| {
        let frame_start = Instant::now();
        match e {
            Event::NewEvents(StartCause::Init) => {
                window_manager.create_window::<NeovideHandle>(
                    "Neovide",
                    window_target,
                    Some(icon.clone()),
                );
                if window_manager.noop().is_err() {
                    std::process::exit(0);
                }
            }
            Event::LoopDestroyed => std::process::exit(0),
            Event::WindowEvent { window_id, event } => {
                if let Some(cf) = window_manager.handle_event(window_id, event) {
                    *control_flow = cf;
                }
            }
            _ => {}
        }

        if !window_manager.render_all() || !window_manager.update_all() {
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
