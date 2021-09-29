use std::cmp::Ordering;

use glutin::{
    self,
    dpi::PhysicalPosition,
    event::{ElementState, Event, MouseButton, MouseScrollDelta, WindowEvent},
    PossiblyCurrent, WindowedContext,
};
use skia_safe::Rect;

<<<<<<< Updated upstream
use super::keyboard_manager::KeyboardManager;
use crate::bridge::UiCommand;
=======
use crate::bridge::{SerialCommand, UiCommand};
>>>>>>> Stashed changes
use crate::channel_utils::LoggingTx;
use crate::renderer::{Renderer, WindowDrawDetails};

fn clamp_position(
    position: PhysicalPosition<f32>,
    region: Rect,
    (font_width, font_height): (u64, u64),
) -> PhysicalPosition<f32> {
    PhysicalPosition::new(
        position
            .x
            .min(region.right - font_width as f32)
            .max(region.left),
        position
            .y
            .min(region.bottom - font_height as f32)
            .max(region.top),
    )
}

fn to_grid_coords(
    position: PhysicalPosition<f32>,
    (font_width, font_height): (u64, u64),
) -> PhysicalPosition<u32> {
    PhysicalPosition::new(
        (position.x as u64 / font_width) as u32,
        (position.y as u64 / font_height) as u32,
    )
}

fn mouse_button_to_button_text(mouse_button: &MouseButton) -> Option<String> {
    match mouse_button {
        MouseButton::Left => Some("left".to_owned()),
        MouseButton::Right => Some("right".to_owned()),
        MouseButton::Middle => Some("middle".to_owned()),
        _ => None,
    }
}

pub struct MouseManager {
    command_sender: LoggingTx<UiCommand>,

    dragging: Option<String>,
    drag_position: PhysicalPosition<u32>,

    has_moved: bool,
    position: PhysicalPosition<u32>,
    relative_position: PhysicalPosition<u32>,

    scroll_position: PhysicalPosition<f32>,

    window_details_under_mouse: Option<WindowDrawDetails>,
    pub enabled: bool,
}

impl MouseManager {
    pub fn new(command_sender: LoggingTx<UiCommand>) -> MouseManager {
        MouseManager {
            command_sender,
            dragging: None,
            has_moved: false,
            position: PhysicalPosition::new(0, 0),
            relative_position: PhysicalPosition::new(0, 0),
            drag_position: PhysicalPosition::new(0, 0),
            scroll_position: PhysicalPosition::new(0.0, 0.0),
            window_details_under_mouse: None,
            enabled: true,
        }
    }

    fn handle_pointer_motion(
        &mut self,
        x: i32,
        y: i32,
        keyboard_manager: &KeyboardManager,
        renderer: &Renderer,
        windowed_context: &WindowedContext<PossiblyCurrent>,
    ) {
        let size = windowed_context.window().inner_size();
        if x < 0 || x as u32 >= size.width || y < 0 || y as u32 >= size.height {
            return;
        }

        let position: PhysicalPosition<f32> = PhysicalPosition::new(x as f32, y as f32);

        // If dragging, the relevant window (the one which we send all commands to) is the one
        // which the mouse drag started on. Otherwise its the top rendered window
        let relevant_window_details = if self.dragging.is_some() {
            renderer.window_regions.iter().find(|details| {
                details.id
                    == self
                        .window_details_under_mouse
                        .as_ref()
                        .expect("If dragging, there should be a window details recorded")
                        .id
            })
        } else {
            // the rendered window regions are sorted by draw order, so the earlier windows in the
            // list are drawn under the later ones
            renderer
                .window_regions
                .iter()
                .filter(|details| {
                    position.x >= details.region.left
                        && position.x < details.region.right
                        && position.y >= details.region.top
                        && position.y < details.region.bottom
                })
                .last()
        };

        let global_bounds = relevant_window_details
            .map(|details| details.region)
            .unwrap_or_else(|| Rect::from_wh(size.width as f32, size.height as f32));
        let clamped_position = clamp_position(
            position,
            global_bounds,
            renderer.grid_renderer.font_dimensions.into(),
        );

        self.position = to_grid_coords(
            clamped_position,
            renderer.grid_renderer.font_dimensions.into(),
        );

        if let Some(relevant_window_details) = relevant_window_details {
            let relative_position = PhysicalPosition::new(
                clamped_position.x - relevant_window_details.region.left,
                clamped_position.y - relevant_window_details.region.top,
            );
            self.relative_position = to_grid_coords(
                relative_position,
                renderer.grid_renderer.font_dimensions.into(),
            );

            let previous_position = self.drag_position;
            // Until https://github.com/neovim/neovim/pull/12667 is merged, we have to special
            // case non floating windows. Floating windows correctly transform mouse positions
            // into grid coordinates, but non floating windows do not.
            self.drag_position = if relevant_window_details.floating_order.is_some() {
                // Floating windows handle relative grid coordinates just fine
                self.relative_position
            } else {
                // Non floating windows need global coordinates
                self.position
            };

            let has_moved = self.drag_position != previous_position;

            // If dragging and we haven't already sent a position, send a drag command
            if self.dragging.is_some() && has_moved {
                self.command_sender
<<<<<<< Updated upstream
                    .send(UiCommand::Drag {
                        button: self.dragging.as_ref().unwrap().to_owned(),
                        grid_id: relevant_window_details.id,
                        position: self.drag_position.into(),
                        modifier_string: keyboard_manager.format_modifier_string(true),
                    })
=======
                    .send(SerialCommand::Drag {
                        grid_id: relevant_window_details.id,
                        position: self.drag_position.into(),
                    }.into())
>>>>>>> Stashed changes
                    .ok();
            } else {
                // otherwise, update the window_id_under_mouse to match the one selected
                self.window_details_under_mouse = Some(relevant_window_details.clone());
            }

            self.has_moved = self.dragging.is_some() && (self.has_moved || has_moved);
        }
    }

    fn handle_pointer_transition(
        &mut self,
        mouse_button: &MouseButton,
        down: bool,
        keyboard_manager: &KeyboardManager,
    ) {
        // For some reason pointer down is handled differently from pointer up and drag.
        // Floating windows: relative coordinates are great.
        // Non floating windows: rather than global coordinates, relative are needed
        if self.enabled {
            if let Some(button_text) = mouse_button_to_button_text(mouse_button) {
                if let Some(details) = &self.window_details_under_mouse {
                    let action = if down {
                        "press".to_owned()
                    } else {
                        "release".to_owned()
                    };

                    let position = if !down && self.has_moved {
                        self.drag_position
                    } else {
                        self.relative_position
                    };

                    self.command_sender
<<<<<<< Updated upstream
                        .send(UiCommand::MouseButton {
                            button: button_text.clone(),
                            action,
                            grid_id: details.id,
                            position: position.into(),
                            modifier_string: keyboard_manager.format_modifier_string(true),
                        })
=======
                        .send(SerialCommand::MouseButton {
                            button: button_text.to_string(),
                            action,
                            grid_id: details.id,
                            position: position.into(),
                        }.into())
>>>>>>> Stashed changes
                        .ok();
                }

                self.dragging = Some(button_text);

                if self.dragging.is_none() {
                    self.has_moved = false;
                }
            }
        }
    }

    fn handle_line_scroll(&mut self, x: f32, y: f32, keyboard_manager: &KeyboardManager) {
        if !self.enabled {
            return;
        }

        let previous_y = self.scroll_position.y as i64;
        self.scroll_position.y += y;
        let new_y = self.scroll_position.y as i64;

        let vertical_input_type = match new_y.partial_cmp(&previous_y) {
            Some(Ordering::Greater) => Some("up"),
            Some(Ordering::Less) => Some("down"),
            _ => None,
        };

        if let Some(input_type) = vertical_input_type {
            let scroll_command: UiCommand = SerialCommand::Scroll {
                direction: input_type.to_string(),
                grid_id: self
                    .window_details_under_mouse
                    .as_ref()
                    .map(|details| details.id)
                    .unwrap_or(0),
                position: self.drag_position.into(),
<<<<<<< Updated upstream
                modifier_string: keyboard_manager.format_modifier_string(true),
            };
=======
            }.into();
>>>>>>> Stashed changes
            for _ in 0..(new_y - previous_y).abs() {
                self.command_sender.send(scroll_command.clone()).ok();
            }
        }

        let previous_x = self.scroll_position.x as i64;
        self.scroll_position.x += x;
        let new_x = self.scroll_position.x as i64;

        let horizontal_input_type = match new_x.partial_cmp(&previous_x) {
            Some(Ordering::Greater) => Some("right"),
            Some(Ordering::Less) => Some("left"),
            _ => None,
        };

        if let Some(input_type) = horizontal_input_type {
            let scroll_command: UiCommand = SerialCommand::Scroll {
                direction: input_type.to_string(),
                grid_id: self
                    .window_details_under_mouse
                    .as_ref()
                    .map(|details| details.id)
                    .unwrap_or(0),
                position: self.drag_position.into(),
<<<<<<< Updated upstream
                modifier_string: keyboard_manager.format_modifier_string(true),
            };
=======
            }.into();
>>>>>>> Stashed changes
            for _ in 0..(new_x - previous_x).abs() {
                self.command_sender.send(scroll_command.clone()).ok();
            }
        }
    }

    fn handle_pixel_scroll(
        &mut self,
        (font_width, font_height): (u64, u64),
        (pixel_x, pixel_y): (f32, f32),
        keyboard_manager: &KeyboardManager,
    ) {
        self.handle_line_scroll(
            pixel_x / font_width as f32,
            pixel_y / font_height as f32,
            keyboard_manager,
        );
    }

    pub fn handle_event(
        &mut self,
        event: &Event<()>,
        keyboard_manager: &KeyboardManager,
        renderer: &Renderer,
        windowed_context: &WindowedContext<PossiblyCurrent>,
    ) {
        match event {
            Event::WindowEvent {
                event: WindowEvent::CursorMoved { position, .. },
                ..
            } => self.handle_pointer_motion(
                position.x as i32,
                position.y as i32,
                keyboard_manager,
                renderer,
                windowed_context,
            ),
            Event::WindowEvent {
                event:
                    WindowEvent::MouseWheel {
                        delta: MouseScrollDelta::LineDelta(x, y),
                        ..
                    },
                ..
            } => self.handle_line_scroll(*x, *y, keyboard_manager),
            Event::WindowEvent {
                event:
                    WindowEvent::MouseWheel {
                        delta: MouseScrollDelta::PixelDelta(delta),
                        ..
                    },
                ..
            } => self.handle_pixel_scroll(
                renderer.grid_renderer.font_dimensions.into(),
                (delta.x as f32, delta.y as f32),
                keyboard_manager,
            ),
            Event::WindowEvent {
                event: WindowEvent::MouseInput { button, state, .. },
                ..
            } => self.handle_pointer_transition(
                button,
                state == &ElementState::Pressed,
                keyboard_manager,
            ),
            _ => {}
        }
    }
}
