use std::{
    cmp::Ordering,
    collections::HashMap,
    time::{Duration, Instant},
};

use skia_safe::Rect;
use winit::{
    dpi::PhysicalPosition,
    event::{
        DeviceId, ElementState, Event, MouseButton, MouseScrollDelta, Touch, TouchPhase,
        WindowEvent,
    },
    window::Window,
};

use crate::{
    bridge::{send_ui, SerialCommand},
    renderer::{Renderer, WindowDrawDetails},
    settings::SETTINGS,
    window::keyboard_manager::KeyboardManager,
    window::{UserEvent, WindowSettings},
};

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
        MouseButton::Back => Some("x1".to_owned()),
        MouseButton::Forward => Some("x2".to_owned()),
        _ => None,
    }
}

struct DragDetails {
    draw_details: WindowDrawDetails,
    button: String,
}

#[derive(Debug)]
struct TouchTrace {
    start_time: Instant,
    start: PhysicalPosition<f32>,
    last: PhysicalPosition<f32>,
    left_deadzone_once: bool,
}

pub struct MouseManager {
    drag_details: Option<DragDetails>,
    drag_position: PhysicalPosition<u32>,

    has_moved: bool,
    position: PhysicalPosition<f32>,

    scroll_position: PhysicalPosition<f32>,

    // the tuple allows to keep track of different fingers per device
    touch_position: HashMap<(DeviceId, u64), TouchTrace>,

    mouse_hidden: bool,
    pub enabled: bool,
}

impl MouseManager {
    pub fn new() -> MouseManager {
        MouseManager {
            drag_details: None,
            has_moved: false,
            position: PhysicalPosition::new(0.0, 0.0),
            drag_position: PhysicalPosition::new(0, 0),
            scroll_position: PhysicalPosition::new(0.0, 0.0),
            touch_position: HashMap::new(),
            mouse_hidden: false,
            enabled: true,
        }
    }

    fn get_window_details_under_mouse<'a>(
        &self,
        renderer: &'a Renderer,
    ) -> Option<&'a WindowDrawDetails> {
        let position = self.position;

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
    }

    fn get_relative_position(
        &self,
        draw_details: &WindowDrawDetails,
        renderer: &Renderer,
    ) -> PhysicalPosition<u32> {
        let global_bounds = draw_details.region;
        let clamped_position = clamp_position(
            self.position,
            global_bounds,
            renderer.grid_renderer.font_dimensions.into(),
        );
        let relative_position = PhysicalPosition::new(
            clamped_position.x - draw_details.region.left,
            clamped_position.y - draw_details.region.top,
        );
        to_grid_coords(
            relative_position,
            renderer.grid_renderer.font_dimensions.into(),
        )
    }

    fn handle_pointer_motion(
        &mut self,
        x: i32,
        y: i32,
        keyboard_manager: &KeyboardManager,
        renderer: &Renderer,
        window: &Window,
    ) {
        let size = window.inner_size();
        if x < 0 || x as u32 >= size.width || y < 0 || y as u32 >= size.height {
            return;
        }

        let position: PhysicalPosition<f32> = PhysicalPosition::new(x as f32, y as f32);
        self.position = position;

        // If dragging, the relevant window (the one which we send all commands to) is the one
        // which the mouse drag started on. Otherwise its the top rendered window
        let relevant_window_details = if let Some(drag_details) = &self.drag_details {
            renderer
                .window_regions
                .iter()
                .find(|details| details.id == drag_details.draw_details.id)
        } else {
            self.get_window_details_under_mouse(renderer)
        };

        if let Some(relevant_window_details) = relevant_window_details {
            let relative_position = self.get_relative_position(relevant_window_details, renderer);
            let previous_position = self.drag_position;
            self.drag_position = relative_position;

            let has_moved = self.drag_position != previous_position;

            if has_moved {
                if let Some(drag_details) = &self.drag_details {
                    send_ui(SerialCommand::Drag {
                        button: drag_details.button.clone(),
                        grid_id: relevant_window_details.event_grid_id(),
                        position: self.drag_position.into(),
                        modifier_string: keyboard_manager.format_modifier_string("", true),
                    });
                } else if SETTINGS.get::<WindowSettings>().mouse_move_event {
                    // Send a mouse move command
                    send_ui(SerialCommand::MouseButton {
                        button: "move".into(),
                        action: "".into(), // this is ignored by nvim
                        grid_id: relevant_window_details.event_grid_id(),
                        position: relative_position.into(),
                        modifier_string: keyboard_manager.format_modifier_string("", true),
                    })
                }
            }

            self.has_moved = self.drag_details.is_some() && (self.has_moved || has_moved);
        }
    }

    fn handle_pointer_transition(
        &mut self,
        mouse_button: &MouseButton,
        down: bool,
        keyboard_manager: &KeyboardManager,
        renderer: &Renderer,
    ) {
        // For some reason pointer down is handled differently from pointer up and drag.
        // Floating windows: relative coordinates are great.
        // Non floating windows: rather than global coordinates, relative are needed
        if self.enabled {
            if let Some(button_text) = mouse_button_to_button_text(mouse_button) {
                if let &Some(details) = &self.get_window_details_under_mouse(renderer) {
                    let action = if down {
                        "press".to_owned()
                    } else {
                        "release".to_owned()
                    };

                    let position = if !down && self.has_moved {
                        self.drag_position
                    } else {
                        self.get_relative_position(details, renderer)
                    };

                    send_ui(SerialCommand::MouseButton {
                        button: button_text.clone(),
                        action,
                        grid_id: details.event_grid_id(),
                        position: position.into(),
                        modifier_string: keyboard_manager.format_modifier_string("", true),
                    });

                    if down {
                        self.drag_details = Some(DragDetails {
                            button: button_text,
                            draw_details: details.clone(),
                        });
                    } else {
                        self.drag_details = None;
                    }
                } else {
                    self.drag_details = None;
                }

                if self.drag_details.is_none() {
                    self.has_moved = false;
                }
            }
        }
    }

    fn handle_line_scroll(
        &mut self,
        x: f32,
        y: f32,
        keyboard_manager: &KeyboardManager,
        renderer: &Renderer,
    ) {
        if !self.enabled {
            return;
        }

        let draw_details = self.get_window_details_under_mouse(renderer);
        let grid_id = draw_details.map(|details| details.id).unwrap_or(0);

        let previous_y = self.scroll_position.y as i64;
        self.scroll_position.y += y;
        let new_y = self.scroll_position.y as i64;

        let vertical_input_type = match new_y.partial_cmp(&previous_y) {
            Some(Ordering::Greater) => Some("up"),
            Some(Ordering::Less) => Some("down"),
            _ => None,
        };

        if let Some(input_type) = vertical_input_type {
            let scroll_command = SerialCommand::Scroll {
                direction: input_type.to_string(),
                grid_id,
                position: self.drag_position.into(),
                modifier_string: keyboard_manager.format_modifier_string("", true),
            };
            for _ in 0..(new_y - previous_y).abs() {
                send_ui(scroll_command.clone());
            }
        }

        let previous_x = self.scroll_position.x as i64;
        self.scroll_position.x += x;
        let new_x = self.scroll_position.x as i64;

        let horizontal_input_type = match new_x.partial_cmp(&previous_x) {
            Some(Ordering::Greater) => Some("left"),
            Some(Ordering::Less) => Some("right"),
            _ => None,
        };

        if let Some(input_type) = horizontal_input_type {
            let scroll_command = SerialCommand::Scroll {
                direction: input_type.to_string(),
                grid_id,
                position: self.drag_position.into(),
                modifier_string: keyboard_manager.format_modifier_string("", true),
            };
            for _ in 0..(new_x - previous_x).abs() {
                send_ui(scroll_command.clone());
            }
        }
    }

    fn handle_pixel_scroll(
        &mut self,
        (font_width, font_height): (u64, u64),
        (pixel_x, pixel_y): (f32, f32),
        keyboard_manager: &KeyboardManager,
        renderer: &Renderer,
    ) {
        self.handle_line_scroll(
            pixel_x / font_width as f32,
            pixel_y / font_height as f32,
            keyboard_manager,
            renderer,
        );
    }

    fn handle_touch(
        &mut self,
        keyboard_manager: &KeyboardManager,
        renderer: &Renderer,
        window: &Window,
        finger_id: (DeviceId, u64),
        location: PhysicalPosition<f32>,
        phase: &TouchPhase,
    ) {
        match phase {
            TouchPhase::Started => {
                let settings = SETTINGS.get::<WindowSettings>();
                let enable_deadzone = settings.touch_deadzone >= 0.0;

                self.touch_position.insert(
                    finger_id,
                    TouchTrace {
                        start_time: Instant::now(),
                        start: location,
                        last: location,
                        left_deadzone_once: !enable_deadzone,
                    },
                );
            }
            TouchPhase::Moved => {
                let mut dragging_just_now = false;

                if let Some(trace) = self.touch_position.get_mut(&finger_id) {
                    if !trace.left_deadzone_once {
                        let distance_to_start = ((trace.start.x - location.x).powi(2)
                            + (trace.start.y - location.y).powi(2))
                        .sqrt();

                        let settings = SETTINGS.get::<WindowSettings>();
                        if distance_to_start >= settings.touch_deadzone {
                            trace.left_deadzone_once = true;
                        }

                        let timeout_setting = Duration::from_micros(
                            (settings.touch_drag_timeout * 1_000_000.) as u64,
                        );
                        if self.drag_details.is_none()
                            && trace.start_time.elapsed() >= timeout_setting
                        {
                            dragging_just_now = true;
                        }
                    }

                    if self.drag_details.is_some() || dragging_just_now {
                        self.handle_pointer_motion(
                            location.x.round() as i32,
                            location.y.round() as i32,
                            keyboard_manager,
                            renderer,
                            window,
                        );
                    }
                    // the double check might seem useless, but the if branch above might set
                    // trace.left_deadzone_once - which urges to check again
                    else if trace.left_deadzone_once {
                        let delta = (trace.last.x - location.x, location.y - trace.last.y);

                        // not updating the position would cause the movement to "escalate" from the
                        // starting point
                        trace.last = location;

                        let font_size = renderer.grid_renderer.font_dimensions.into();
                        self.handle_pixel_scroll(font_size, delta, keyboard_manager, renderer);
                    }
                }

                if dragging_just_now {
                    self.handle_pointer_motion(
                        location.x.round() as i32,
                        location.y.round() as i32,
                        keyboard_manager,
                        renderer,
                        window,
                    );
                    self.handle_pointer_transition(
                        &MouseButton::Left,
                        true,
                        keyboard_manager,
                        renderer,
                    );
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                if let Some(trace) = self.touch_position.remove(&finger_id) {
                    if self.drag_details.is_some() {
                        self.handle_pointer_transition(
                            &MouseButton::Left,
                            false,
                            keyboard_manager,
                            renderer,
                        );
                    }
                    if !trace.left_deadzone_once {
                        self.handle_pointer_motion(
                            trace.start.x.round() as i32,
                            trace.start.y.round() as i32,
                            keyboard_manager,
                            renderer,
                            window,
                        );
                        self.handle_pointer_transition(
                            &MouseButton::Left,
                            true,
                            keyboard_manager,
                            renderer,
                        );
                        self.handle_pointer_transition(
                            &MouseButton::Left,
                            false,
                            keyboard_manager,
                            renderer,
                        );
                    }
                }
            }
        }
    }

    pub fn handle_event(
        &mut self,
        event: &Event<UserEvent>,
        keyboard_manager: &KeyboardManager,
        renderer: &Renderer,
        window: &Window,
    ) {
        match event {
            Event::WindowEvent {
                event: WindowEvent::CursorMoved { position, .. },
                ..
            } => {
                self.handle_pointer_motion(
                    position.x as i32,
                    position.y as i32,
                    keyboard_manager,
                    renderer,
                    window,
                );
                if self.mouse_hidden {
                    window.set_cursor_visible(true);
                    self.mouse_hidden = false;
                }
            }
            Event::WindowEvent {
                event:
                    WindowEvent::MouseWheel {
                        delta: MouseScrollDelta::LineDelta(x, y),
                        ..
                    },
                ..
            } => self.handle_line_scroll(*x, *y, keyboard_manager, renderer),
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
                renderer,
            ),
            Event::WindowEvent {
                event:
                    WindowEvent::Touch(Touch {
                        device_id,
                        id,
                        location,
                        phase,
                        ..
                    }),
                ..
            } => self.handle_touch(
                keyboard_manager,
                renderer,
                window,
                (*device_id, *id),
                location.cast(),
                phase,
            ),
            Event::WindowEvent {
                event: WindowEvent::MouseInput { button, state, .. },
                ..
            } => self.handle_pointer_transition(
                button,
                state == &ElementState::Pressed,
                keyboard_manager,
                renderer,
            ),
            Event::WindowEvent {
                event:
                    WindowEvent::KeyboardInput {
                        event: key_event, ..
                    },
                ..
            } => {
                if key_event.state == ElementState::Pressed {
                    let window_settings = SETTINGS.get::<WindowSettings>();
                    if window_settings.hide_mouse_when_typing && !self.mouse_hidden {
                        window.set_cursor_visible(false);
                        self.mouse_hidden = true;
                    }
                }
            }
            _ => {}
        }
    }
}
