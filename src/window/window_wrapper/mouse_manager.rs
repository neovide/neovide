use glutin::{
    self, 
    WindowedContext, 
    dpi::{
        LogicalPosition, 
        PhysicalPosition,
    }, 
    event::{
        ElementState, 
        Event, 
        MouseButton, 
        MouseScrollDelta, 
        WindowEvent,
    }, 
    PossiblyCurrent
};

use crate::channel_utils::LoggingTx;
use crate::bridge::UiCommand;
use crate::renderer::{Renderer, WindowDrawDetails};

pub struct MouseManager {
    command_sender: LoggingTx<UiCommand>,
    dragging: bool,
    position: LogicalPosition<u32>,
    window_details_under_mouse: Option<WindowDrawDetails>,
    pub enabled: bool,
}

impl MouseManager {
    pub fn new(command_sender: LoggingTx<UiCommand>) -> MouseManager {
        MouseManager {
            command_sender,
            dragging: false,
            position: LogicalPosition::new(0, 0),
            window_details_under_mouse: None,
            enabled: true,
        }
    }
    
    fn handle_pointer_motion(&mut self, x: i32, y: i32, renderer: &Renderer, windowed_context: &WindowedContext<PossiblyCurrent>) {
        let size = windowed_context.window().inner_size();
        if x < 0 || x as u32 >= size.width || y < 0 || y as u32 >= size.height {
            return;
        }

        let logical_position: LogicalPosition<u32> = PhysicalPosition::new(x as u32, y as u32)
            .to_logical(windowed_context.window().scale_factor());

        // If dragging, the relevant window (the one which we send all commands to) is the one
        // which the mouse drag started on. Otherwise its the top rendered window
        let relevant_window_details = if self.dragging {
            renderer.window_regions.iter()
                .find(|details| details.id == self.window_details_under_mouse.as_ref().expect("If dragging, there should be a window details recorded").id)
        } else {
            // the rendered window regions are sorted by draw order, so the earlier windows in the
            // list are drawn under the later ones
            renderer.window_regions.iter().filter(|details| {
                logical_position.x >= details.region.left as u32 && 
                logical_position.x < details.region.right as u32 && 
                logical_position.y >= details.region.top as u32 && 
                logical_position.y < details.region.bottom as u32
            }).last()
        };

        if let Some(relevant_window_details) = relevant_window_details {
            let previous_position = self.position;
            // Until https://github.com/neovim/neovim/pull/12667 is merged, we have to special
            // case non floating windows. Floating windows correctly transform mouse positions
            // into grid coordinates, but non floating windows do not.
            self.position = if relevant_window_details.floating_order.is_some() {
                // Floating windows handle relative grid coordinates just fine
                LogicalPosition::new(
                    (logical_position.x - relevant_window_details.region.left as u32) / renderer.font_width as u32,
                    (logical_position.y - relevant_window_details.region.top as u32) / renderer.font_height as u32,
                )
            } else {
                // Non floating windows need global coordinates
                LogicalPosition::new(
                    logical_position.x / renderer.font_width as u32,
                    logical_position.y / renderer.font_height as u32
                )
            };

            // If dragging and we haven't already sent a position, send a drag command
            if self.dragging && self.position != previous_position {
                let window_id_to_send_to = self.window_details_under_mouse.as_ref().map(|details| details.id).unwrap_or(0);
                self.command_sender
                    .send(UiCommand::Drag {
                        grid_id: window_id_to_send_to,
                        position: self.position.into(),
                    })
                    .ok();
            } else {
                // otherwise, update the window_id_under_mouse to match the one selected
                self.window_details_under_mouse = Some(relevant_window_details.clone());
            }
        }
    }

    fn handle_pointer_down(&mut self, renderer: &Renderer) {
        // For some reason pointer down is handled differently from pointer up and drag.
        // Floating windows: relative coordinates are great.
        // Non floating windows: rather than global coordinates, relative are needed
        if self.enabled  {
            if let Some(details) = &self.window_details_under_mouse {
                if details.floating_order.is_some() {
                    self.command_sender
                        .send(UiCommand::MouseButton {
                            action: String::from("press"),
                            grid_id: details.id,
                            position: (self.position.x, self.position.y),
                        })
                        .ok();
                } else {
                    let relative_position = (
                        self.position.x - (details.region.left as u64 / renderer.font_width) as u32,
                        self.position.y - (details.region.top as u64 / renderer.font_height) as u32,
                    );
                    self.command_sender
                        .send(UiCommand::MouseButton {
                            action: String::from("press"),
                            grid_id: details.id,
                            position: relative_position,
                        })
                        .ok();
                }
            }
        }
        self.dragging = true;
    }

    fn handle_pointer_up(&mut self) {
        if self.enabled {
            self.command_sender
                .send(UiCommand::MouseButton {
                    action: String::from("release"),
                    grid_id: self.window_details_under_mouse.as_ref().map(|details| details.id).unwrap_or(0),
                    position: (self.position.x, self.position.y),
                })
                .ok();
        }
        self.dragging = false;
    }

    fn handle_mouse_wheel(&mut self, x: f32, y: f32) {
        if !self.enabled {
            return;
        }

        let vertical_input_type = match y {
            _ if y > 1.8 => Some("up"),
            _ if y < -1.8 => Some("down"),
            _ => None,
        };

        if let Some(input_type) = vertical_input_type {
            self.command_sender
                .send(UiCommand::Scroll {
                    direction: input_type.to_string(),
                    grid_id: self.window_details_under_mouse.as_ref().map(|details| details.id).unwrap_or(0),
                    position: (self.position.x, self.position.y),
                })
                .ok();
        }

        let horizontal_input_type = match x {
            _ if x > 1.8 => Some("right"),
            _ if x < -1.8 => Some("left"),
            _ => None,
        };

        if let Some(input_type) = horizontal_input_type {
            self.command_sender
                .send(UiCommand::Scroll {
                    direction: input_type.to_string(),
                    grid_id: self.window_details_under_mouse.as_ref().map(|details| details.id).unwrap_or(0),
                    position: (self.position.x, self.position.y),
                })
                .ok();
        }
    }

    pub fn handle_event(&mut self, event: &Event<()>, renderer: &Renderer, windowed_context: &WindowedContext<PossiblyCurrent>) {
        match event {
            Event::WindowEvent {
                event: WindowEvent::CursorMoved { position, .. },
                ..
            } => self.handle_pointer_motion(
                position.x as i32, position.y as i32, 
                renderer, windowed_context),
            Event::WindowEvent {
                event:
                    WindowEvent::MouseWheel {
                        delta: MouseScrollDelta::LineDelta(x, y),
                        ..
                    },
                ..
            } => self.handle_mouse_wheel(*x as f32, *y as f32),
            Event::WindowEvent {
                event:
                    WindowEvent::MouseWheel {
                        delta: MouseScrollDelta::PixelDelta(logical_position),
                        ..
                    },
                ..
            } => self.handle_mouse_wheel(logical_position.x as f32, logical_position.y as f32),
            Event::WindowEvent {
                event:
                    WindowEvent::MouseInput {
                        button: MouseButton::Left,
                        state,
                        ..
                    },
                ..
            } => {
                if state == &ElementState::Pressed {
                    self.handle_pointer_down(renderer);
                } else {
                    self.handle_pointer_up();
                }
            },
            _ => {}
        }
    }
}
