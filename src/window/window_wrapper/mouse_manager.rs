use glutin::{
    self, 
    WindowedContext, 
    dpi::{
        LogicalPosition, 
        LogicalSize, 
        PhysicalSize
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
use crate::renderer::Renderer;

pub struct MouseManager {
    command_sender: LoggingTx<UiCommand>,
    button_down: bool,
    position: LogicalPosition<u32>,
    grid_id_under_mouse: u64,
    pub enabled: bool,
}

impl MouseManager {
    pub fn new(command_sender: LoggingTx<UiCommand>) -> MouseManager {
        MouseManager {
            command_sender,
            button_down: false,
            position: LogicalPosition::new(0, 0),
            grid_id_under_mouse: 0,
            enabled: true,
        }
    }
    
    fn handle_pointer_motion(&mut self, x: i32, y: i32, renderer: &Renderer, windowed_context: &WindowedContext<PossiblyCurrent>) {
        let size = windowed_context.window().inner_size();
        if x < 0 || x as u32 >= size.width || y < 0 || y as u32 >= size.height {
            return;
        }

        let previous_position = self.position;

        let logical_position: LogicalSize<u32> = PhysicalSize::new(x as u32, y as u32)
            .to_logical(windowed_context.window().scale_factor());

        let mut top_window_position = (0.0, 0.0);
        let mut top_grid_position = None;

        for details in renderer.window_regions.iter() {
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
            self.position = LogicalPosition::new(
                (grid_position.width as u64 / renderer.font_width) as u32,
                (grid_position.height as u64 / renderer.font_height) as u32,
            );

            if self.enabled && self.button_down && previous_position != self.position {
                let (window_left, window_top) = top_window_position;

                // Until https://github.com/neovim/neovim/pull/12667 is merged, we have to special
                // case non floating windows. Floating windows correctly transform mouse positions
                // into grid coordinates, but non floating windows do not.
                let position = if grid_floating {
                    (self.position.x, self.position.y)
                } else {
                    let adjusted_drag_left = self.position.x
                        + (window_left / renderer.font_width as f32) as u32;
                    let adjusted_drag_top = self.position.y
                        + (window_top / renderer.font_height as f32) as u32;
                    (adjusted_drag_left, adjusted_drag_top)
                };

                self.command_sender
                    .send(UiCommand::Drag {
                        grid_id: self.grid_id_under_mouse,
                        position,
                    })
                    .ok();
            }
        }
    }

    fn handle_pointer_down(&mut self) {
        if self.enabled {
            self.command_sender
                .send(UiCommand::MouseButton {
                    action: String::from("press"),
                    grid_id: self.grid_id_under_mouse,
                    position: (self.position.x, self.position.y),
                })
                .ok();
        }
        self.button_down = true;
    }

    fn handle_pointer_up(&mut self) {
        if self.enabled {
            self.command_sender
                .send(UiCommand::MouseButton {
                    action: String::from("release"),
                    grid_id: self.grid_id_under_mouse,
                    position: (self.position.x, self.position.y),
                })
                .ok();
        }
        self.button_down = false;
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
                    grid_id: self.grid_id_under_mouse,
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
                    grid_id: self.grid_id_under_mouse,
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
                    self.handle_pointer_down();
                } else {
                    self.handle_pointer_up();
                }
            },
            _ => {}
        }
    }
}
