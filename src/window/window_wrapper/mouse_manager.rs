use glutin::{
    self,
    dpi::{LogicalPosition, PhysicalPosition},
    event::{ElementState, Event, MouseButton, MouseScrollDelta, WindowEvent},
    PossiblyCurrent, WindowedContext,
};
use skia_safe::Rect;

use crate::bridge::UiCommand;
use crate::channel_utils::LoggingTx;
use crate::renderer::{Renderer, WindowDrawDetails};
use crate::settings::SETTINGS;
use crate::window::WindowSettings;

fn clamp_position(
    position: LogicalPosition<f32>,
    region: Rect,
    font_width: u64,
    font_height: u64,
) -> LogicalPosition<f32> {
    LogicalPosition::new(
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
    position: LogicalPosition<f32>,
    font_width: u64,
    font_height: u64,
) -> LogicalPosition<u32> {
    LogicalPosition::new(
        (position.x as u64 / font_width) as u32,
        (position.y as u64 / font_height) as u32,
    )
}

pub struct MouseManager {
    command_sender: LoggingTx<UiCommand>,
    dragging: bool,
    has_moved: bool,
    position: LogicalPosition<u32>,
    relative_position: LogicalPosition<u32>,
    drag_position: LogicalPosition<u32>,
    window_details_under_mouse: Option<WindowDrawDetails>,
    pub enabled: bool,
}

impl MouseManager {
    pub fn new(command_sender: LoggingTx<UiCommand>) -> MouseManager {
        MouseManager {
            command_sender,
            dragging: false,
            has_moved: false,
            position: LogicalPosition::new(0, 0),
            relative_position: LogicalPosition::new(0, 0),
            drag_position: LogicalPosition::new(0, 0),
            window_details_under_mouse: None,
            enabled: true,
        }
    }

    fn handle_pointer_motion(
        &mut self,
        x: i32,
        y: i32,
        renderer: &Renderer,
        windowed_context: &WindowedContext<PossiblyCurrent>,
    ) {
        let size = windowed_context.window().inner_size();
        if x < 0 || x as u32 >= size.width || y < 0 || y as u32 >= size.height {
            return;
        }

        let logical_position: LogicalPosition<f32> = PhysicalPosition::new(x as u32, y as u32)
            .to_logical(windowed_context.window().scale_factor());

        // If dragging, the relevant window (the one which we send all commands to) is the one
        // which the mouse drag started on. Otherwise its the top rendered window
        let relevant_window_details = if self.dragging {
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
                    logical_position.x >= details.region.left
                        && logical_position.x < details.region.right
                        && logical_position.y >= details.region.top
                        && logical_position.y < details.region.bottom
                })
                .last()
        };

        let global_bounds = relevant_window_details
            .map(|details| details.region)
            .unwrap_or(Rect::from_wh(size.width as f32, size.height as f32));
        let clamped_position = clamp_position(
            logical_position,
            global_bounds,
            renderer.font_width,
            renderer.font_height,
        );

        self.position = to_grid_coords(clamped_position, renderer.font_width, renderer.font_height);

        if let Some(relevant_window_details) = relevant_window_details {
            let relative_position = LogicalPosition::new(
                clamped_position.x - relevant_window_details.region.left,
                clamped_position.y - relevant_window_details.region.top,
            );
            self.relative_position =
                to_grid_coords(relative_position, renderer.font_width, renderer.font_height);

            let previous_position = self.drag_position;
            // Until https://github.com/neovim/neovim/pull/12667 is merged, we have to special
            // case non floating windows. Floating windows correctly transform mouse positions
            // into grid coordinates, but non floating windows do not.
            self.drag_position = if relevant_window_details.floating_order.is_some() {
                // Floating windows handle relative grid coordinates just fine
                self.relative_position.clone()
            } else {
                // Non floating windows need global coordinates
                self.position.clone()
            };

            let has_moved = self.drag_position != previous_position;

            // If dragging and we haven't already sent a position, send a drag command
            if self.dragging && has_moved {
                self.command_sender
                    .send(UiCommand::Drag {
                        grid_id: relevant_window_details.id,
                        position: self.drag_position.into(),
                    })
                    .ok();
            } else {
                // otherwise, update the window_id_under_mouse to match the one selected
                self.window_details_under_mouse = Some(relevant_window_details.clone());
            }

            self.has_moved = self.dragging && (self.has_moved || has_moved);
        }
    }

    fn handle_pointer_transition(&mut self, down: bool) {
        // For some reason pointer down is handled differently from pointer up and drag.
        // Floating windows: relative coordinates are great.
        // Non floating windows: rather than global coordinates, relative are needed
        if self.enabled {
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
                    .send(UiCommand::MouseButton {
                        action,
                        grid_id: details.id,
                        position: position.into(),
                    })
                    .ok();
            }
        }

        self.dragging = down;

        if !self.dragging {
            self.has_moved = false;
        }
    }

    fn handle_mouse_wheel(&mut self, x: f32, y: f32) {
        if !self.enabled {
            return;
        }

        let scroll_dead_zone = SETTINGS.get::<WindowSettings>().scroll_dead_zone;

        let vertical_input_type = match y {
            _ if y > scroll_dead_zone => Some("up"),
            _ if y < -scroll_dead_zone => Some("down"),
            _ => None,
        };

        if let Some(input_type) = vertical_input_type {
            self.command_sender
                .send(UiCommand::Scroll {
                    direction: input_type.to_string(),
                    grid_id: self
                        .window_details_under_mouse
                        .as_ref()
                        .map(|details| details.id)
                        .unwrap_or(0),
                    position: self.drag_position.into(),
                })
                .ok();
        }

        let horizontal_input_type = match x {
            _ if x > scroll_dead_zone => Some("right"),
            _ if x < -scroll_dead_zone => Some("left"),
            _ => None,
        };

        if let Some(input_type) = horizontal_input_type {
            self.command_sender
                .send(UiCommand::Scroll {
                    direction: input_type.to_string(),
                    grid_id: self
                        .window_details_under_mouse
                        .as_ref()
                        .map(|details| details.id)
                        .unwrap_or(0),
                    position: self.drag_position.into(),
                })
                .ok();
        }
    }

    pub fn handle_event(
        &mut self,
        event: &Event<()>,
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
            } => self.handle_pointer_transition(state == &ElementState::Pressed),
            _ => {}
        }
    }
}
