#[macro_use]
mod layouts;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::thread::sleep;
use std::time::{Duration, Instant};

use crossfire::mpsc::TxUnbounded;
use image::{load_from_memory, GenericImageView, Pixel};
use log::{debug, error, info, trace};
use skulpin::ash::prelude::VkResult;
use skulpin::winit;
use skulpin::winit::event::VirtualKeyCode as Keycode;
use skulpin::winit::event::{
    ElementState, Event, ModifiersState, MouseButton, MouseScrollDelta, StartCause, WindowEvent,
};
use skulpin::winit::event_loop::{ControlFlow, EventLoop};
use skulpin::winit::window::{Fullscreen, Icon};
use skulpin::{
    winit::dpi::LogicalSize, CoordinateSystem, PhysicalSize, PresentMode,
    Renderer as SkulpinRenderer, RendererBuilder, Window, WinitWindow,
};

use super::handle_new_grid_size;
pub use super::keyboard;
use super::settings::*;
use crate::bridge::UiCommand;
use crate::editor::WindowCommand;
use crate::error_handling::ResultPanicExplanation;
use crate::redraw_scheduler::REDRAW_SCHEDULER;
use crate::renderer::Renderer;
use crate::settings::*;
use crate::window::DrawCommand;
use layouts::produce_neovim_keybinding_string;

mod manager;
use manager::*;

#[derive(RustEmbed)]
#[folder = "assets/"]
struct Asset;

impl Default for NeovideHandle {
    fn default() -> NeovideHandle {
        let renderer = Renderer::new();
        NeovideHandle {
            window: None,
            renderer,
            mouse_down: false,
            mouse_position: LogicalSize {
                width: 0,
                height: 0,
            },
            mouse_enabled: true,
            grid_id_under_mouse: 0,
            title: String::from("Neovide"),
            previous_size: LogicalSize::new(0, 0),
            // transparency: 1.0,
            fullscreen: false,
            cached_size: LogicalSize::new(0, 0),
            cached_position: LogicalSize::new(0, 0),
            ui_command_sender: None,
            // window_command_receiver: None,
            running: None,
        }
    }
}

pub struct NeovideHandle {
    window: Option<winit::window::Window>,
    renderer: Renderer,
    mouse_down: bool,
    mouse_position: LogicalSize<u32>,
    mouse_enabled: bool,
    grid_id_under_mouse: u64,
    title: String,
    previous_size: LogicalSize<u32>,
    // transparency: f32,
    fullscreen: bool,
    cached_size: LogicalSize<u32>,
    cached_position: LogicalSize<u32>,
    ui_command_sender: Option<Arc<TxUnbounded<UiCommand>>>,
    // window_command_receiver: Option<Receiver<WindowCommand>>,
    running: Option<Arc<AtomicBool>>,
}

impl NeovideHandle {
    pub fn toggle_fullscreen(&mut self) {
        let window = self.window.as_ref().unwrap();
        if self.fullscreen {
            window.set_fullscreen(None);

            // Use cached size and position
            window.set_inner_size(winit::dpi::LogicalSize::new(
                self.cached_size.width,
                self.cached_size.height,
            ));
            window.set_outer_position(winit::dpi::LogicalPosition::new(
                self.cached_position.width,
                self.cached_position.height,
            ));
        } else {
            let current_size = window.inner_size();
            self.cached_size = LogicalSize::new(current_size.width, current_size.height);
            let current_position = window.outer_position().unwrap();
            self.cached_position =
                LogicalSize::new(current_position.x as u32, current_position.y as u32);
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

    pub fn handle_title_changed(&mut self, new_title: String) {
        self.title = new_title;
        if let Some(window) = self.window.as_ref() {
            window.set_title(&self.title);
        }
    }

    pub fn handle_quit(&mut self) {
        self.running
            .as_ref()
            .unwrap()
            .store(false, Ordering::Relaxed);
    }

    pub fn handle_keyboard_input(
        &mut self,
        keycode: Option<Keycode>,
        modifiers: Option<ModifiersState>,
    ) {
        if keycode.is_some() {
            trace!(
                "Keyboard Input Received: keycode-{:?} modifiers-{:?} ",
                keycode,
                modifiers
            );
        }

        if let Some(keybinding_string) = produce_neovim_keybinding_string(keycode, None, modifiers)
        {
            self.ui_command_sender
                .as_ref()
                .unwrap()
                .send(UiCommand::Keyboard(keybinding_string))
                .unwrap_or_explained_panic(
                    "Could not send UI command from the window system to the neovim process.",
                );
        }
    }

    pub fn handle_pointer_motion(&mut self, x: i32, y: i32) {
        let previous_position = self.mouse_position;
        let window = self.window.as_ref().unwrap();
        let winit_window_wrapper = WinitWindow::new(window);
        let logical_position =
            PhysicalSize::new(x as u32, y as u32).to_logical(winit_window_wrapper.scale_factor());

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
                    .as_ref()
                    .unwrap()
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
                .as_ref()
                .unwrap()
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
                .as_ref()
                .unwrap()
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
                .as_ref()
                .unwrap()
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
                .as_ref()
                .unwrap()
                .send(UiCommand::Scroll {
                    direction: input_type.to_string(),
                    grid_id: self.grid_id_under_mouse,
                    position: (self.mouse_position.width, self.mouse_position.height),
                })
                .ok();
        }
    }

    pub fn handle_focus_lost(&mut self) {
        self.ui_command_sender
            .as_ref()
            .unwrap()
            .send(UiCommand::FocusLost)
            .ok();
    }

    pub fn handle_focus_gained(&mut self) {
        self.ui_command_sender
            .as_ref()
            .unwrap()
            .send(UiCommand::FocusGained)
            .ok();
        REDRAW_SCHEDULER.queue_next_frame();
    }

    // pub fn draw_frame(&mut self, dt: f32) -> VkResult<bool> {
    //     let winit_window_wrapper = WinitWindow::new(&self.window);
    //     let new_size = winit_window_wrapper.logical_size();
    //     if self.previous_size != new_size {
    //         handle_new_grid_size(new_size, &self.renderer, &self.ui_command_sender.unwrap());
    //         self.previous_size = new_size;
    //     }

    //     let current_size = self.previous_size;
    //     let ui_command_sender = self.ui_command_sender.unwrap().clone();

    //     if REDRAW_SCHEDULER.should_draw() || SETTINGS.get::<WindowSettings>().no_idle {
    //         debug!("Render Triggered");

    //         let renderer = &mut self.renderer;
    //         self.skulpin_renderer.draw(
    //             &winit_window_wrapper,
    //             |canvas, coordinate_system_helper| {
    //                 if renderer.draw_frame(canvas, &coordinate_system_helper, dt) {
    //                     handle_new_grid_size(current_size, &renderer, &ui_command_sender);
    //                 }
    //             },
    //         )?;

    //         Ok(true)
    //     } else {
    //         Ok(false)
    //     }
    // }
}

impl WindowHandle for NeovideHandle {
    fn window(&mut self) -> skulpin::winit::window::Window {
        self.window.take().unwrap()
    }

    fn set_window(&mut self, window: skulpin::winit::window::Window) {
        self.window = Some(window);
    }

    fn set_title(&mut self, new_title: String) {
        self.handle_title_changed(new_title);
    }

    fn set_running(&mut self, running: Option<Arc<AtomicBool>>) {
        self.running = running;
    }

    fn set_sender(&mut self, ui_command_sender: Option<Arc<TxUnbounded<UiCommand>>>) {
        self.ui_command_sender = ui_command_sender;
    }

    fn set_receiver(
        &mut self,
        batched_draw_command_receiver: Option<Arc<Receiver<Vec<DrawCommand>>>>,
    ) {
        self.renderer
            .set_command_receiver(batched_draw_command_receiver);
    }

    fn logical_size(&self) -> LogicalSize<u32> {
        if let Some(window) = self.window.as_ref() {
            let scale_factor = window.scale_factor();
            window.inner_size().to_logical(scale_factor)
        } else {
            let (width, height) = super::window_geometry_or_default();
            LogicalSize {
                width: (width as f32 * self.renderer.font_width) as u32,
                height: (height as f32 * self.renderer.font_height) as u32,
            }
        }
    }

    fn update(&mut self) -> bool {
        self.synchronize_settings();
        // self.handle_keyboard_input();
        true
    }

    fn should_draw(&self) -> bool {
        REDRAW_SCHEDULER.should_draw() || SETTINGS.get::<WindowSettings>().no_idle
    }

    fn draw(&mut self, skulpin_renderer: &mut SkulpinRenderer) -> bool {
        if self.should_draw() {
            let renderer = &mut self.renderer;
            let window = WinitWindow::new(&self.window.as_ref().unwrap());
            let error = skulpin_renderer
                .draw(&window, |canvas, coordinate_system_helper| {
                    let dt = 1.0 / (SETTINGS.get::<WindowSettings>().refresh_rate as f32);
                    renderer.draw_frame(canvas, &coordinate_system_helper, dt);
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
    fn process_event(&mut self, event: WindowEvent) -> Option<ControlFlow> {
        let mut keycode = None;
        let mut ignore_text_this_frame = false;
        let mut current_modifiers = None;

        match event {
            WindowEvent::CloseRequested => {
                self.handle_quit();
                return Some(ControlFlow::Exit);
            }
            WindowEvent::DroppedFile(path) => {
                self.ui_command_sender
                    .as_ref()
                    .unwrap()
                    .send(UiCommand::FileDrop(
                        path.into_os_string().into_string().unwrap(),
                    ))
                    .ok();
            }

            WindowEvent::KeyboardInput { input, .. } => {
                if input.state == ElementState::Pressed {
                    keycode = input.virtual_keycode;
                }
            }
            WindowEvent::ModifiersChanged(m) => {
                current_modifiers = Some(m);
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.handle_pointer_motion(position.x as i32, position.y as i32)
            }
            WindowEvent::MouseWheel {
                delta: MouseScrollDelta::LineDelta(x, y),
                ..
            } => self.handle_mouse_wheel(x as i32, y as i32),
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
                    ignore_text_this_frame = true; // Ignore any text events on the first frame when focus is regained. https://github.com/Kethku/neovide/issues/193
                    self.handle_focus_gained();
                } else {
                    self.handle_focus_lost();
                }
            }
            _ => REDRAW_SCHEDULER.queue_next_frame(),
        }

        if !ignore_text_this_frame {
            self.handle_keyboard_input(keycode, current_modifiers);
        }
        None
    }
}

pub fn start_loop(
    window_command_receiver: Receiver<WindowCommand>,
    ui_command_sender: TxUnbounded<UiCommand>,
    batched_draw_command_receiver: Receiver<Vec<DrawCommand>>,
    running: Arc<AtomicBool>,
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
    info!("icon created");

    let event_loop = EventLoop::<NeovideEvent>::with_user_event();
    let event_loop_proxy = event_loop.create_proxy();
    let mut window_manager: WindowManager<NeovideEvent> = WindowManager::new(event_loop_proxy);

    let ui_command_sender = Arc::new(ui_command_sender);
    let batched_draw_command_receiver = Arc::new(batched_draw_command_receiver);

    // Ask Kethku

    // let mut was_animating = false;
    let previous_frame_start = Instant::now();

    event_loop.run(move |e, window_target, control_flow| {
        if !running.load(Ordering::Relaxed) {
            *control_flow = ControlFlow::Exit;
            return;
        }

        let frame_start = Instant::now();

        let refresh_rate = { SETTINGS.get::<WindowSettings>().refresh_rate as f32 };

        // Ask Kethku

        // let dt = if was_animating {
        //     previous_frame_start.elapsed().as_secs_f32()
        // } else {
        //     1.0 / refresh_rate
        // };

        match e {
            Event::NewEvents(StartCause::Init) => {
                window_manager.create_window::<NeovideHandle>(
                    "Neovide".to_string(),
                    window_target,
                    Some(icon.clone()),
                    ui_command_sender.to_owned(),
                    batched_draw_command_receiver.to_owned(),
                    Some(running.clone()), // Some(window_command_receiver),
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

        // Ask Kethku

        // let window_commands: Vec<WindowCommand> =
        //     window_wrapper.window_command_receiver.try_iter().collect();
        // for window_command in window_commands.into_iter() {
        //     match window_command {
        //         WindowCommand::TitleChanged(new_title) => {
        //             window_wrapper.handle_title_changed(new_title)
        //         }
        //         WindowCommand::SetMouseEnabled(mouse_enabled) => {
        //             window_wrapper.mouse_enabled = mouse_enabled
        //         }
        //     }
        // }

        // match window_wrapper.draw_frame(dt) {
        //     Ok(animating) => {
        //         was_animating = animating;
        //     }
        //     Err(error) => {
        //         error!("Render failed: {}", error);
        //         window_wrapper.running.store(false, Ordering::Relaxed);
        //         return;
        //     }
        // }

        if !window_manager.update_all() || !window_manager.render_all() {
            running.store(false, Ordering::Relaxed);
        }

        let elapsed = frame_start.elapsed();
        let frame_length = Duration::from_secs_f32(1.0 / refresh_rate);

        if elapsed < frame_length {
            *control_flow = ControlFlow::WaitUntil(Instant::now() + frame_length);
        }
    });
}
