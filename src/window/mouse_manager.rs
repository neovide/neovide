use std::{
    cmp::Ordering,
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use winit::{
    event::WindowEvent,
    event::{DeviceId, ElementState, MouseButton, MouseScrollDelta, Touch, TouchPhase},
    window::Window,
};

use glamour::{Contains, Point2};

use crate::{
    bridge::{send_ui, SerialCommand},
    renderer::{Renderer, WindowDrawDetails},
    settings::Settings,
    units::{GridPos, GridScale, GridSize, GridVec, PixelPos, PixelRect, PixelSize, PixelVec},
    window::keyboard_manager::KeyboardManager,
    window::WindowSettings,
};

fn mouse_button_to_button_text(mouse_button: MouseButton) -> Option<String> {
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
    button: MouseButton,
}

pub struct EditorState<'a> {
    pub grid_scale: &'a GridScale,
    pub window_regions: &'a Vec<WindowDrawDetails>,
    pub full_region: WindowDrawDetails,
    pub window: &'a Window,
    pub keyboard_manager: &'a KeyboardManager,
}

#[derive(Debug)]
struct TouchTrace {
    start_time: Instant,
    start: PixelPos<f32>,
    last: PixelPos<f32>,
    left_deadzone_once: bool,
}

pub struct MouseManager {
    drag_details: Option<DragDetails>,
    grid_position: GridPos<u32>,

    has_moved: bool,
    pub window_position: PixelPos<f32>,

    scroll_position: GridPos<f32>,

    // the tuple allows to keep track of different fingers per device
    touch_position: HashMap<(DeviceId, u64), TouchTrace>,

    mouse_hidden: bool,
    // tracks whether we need to force a cursor visibility resync once focus returns.
    // on macos, alt-tabbing while hidden keeps appkit in a cursor hidden mode that ignores
    // redundant show requests https://github.com/rust-windowing/winit/issues/1295 so we
    // remember to toggle visibility once we regain focus.
    cursor_resync_needed: bool,
    pub enabled: bool,

    settings: Arc<Settings>,
}

impl MouseManager {
    pub fn new(settings: Arc<Settings>) -> MouseManager {
        MouseManager {
            drag_details: None,
            has_moved: false,
            window_position: PixelPos::default(),
            grid_position: GridPos::default(),
            scroll_position: GridPos::default(),
            touch_position: HashMap::new(),
            mouse_hidden: false,
            cursor_resync_needed: false,
            enabled: true,
            settings,
        }
    }

    fn request_cursor_visible(&mut self, window: &Window) {
        window.set_cursor_visible(true);
        self.mouse_hidden = false;
        // remember to reapply on focus regain if we changed visibility while unfocused.
        self.cursor_resync_needed = !window.has_focus();
    }

    fn force_cursor_visible(&mut self, window: &Window) {
        #[cfg(target_os = "macos")]
        {
            // winit short-circuits duplicate visibility requests and AppKit won't repaint when the
            // window is unfocused https://github.com/rust-windowing/winit/issues/1295 so flip to
            // false first to ensure the following true call is seen.
            // TODO: move this workaround into winit so visibility resyncs automatically.
            window.set_cursor_visible(false);
        }
        window.set_cursor_visible(true);
        self.mouse_hidden = false;
        self.cursor_resync_needed = false;
    }

    fn hide_cursor(&mut self, window: &Window) {
        window.set_cursor_visible(false);
        self.mouse_hidden = true;
        self.cursor_resync_needed = false;
    }

    fn handle_focus_gain(&mut self, window: &Window) {
        if self.cursor_resync_needed {
            self.force_cursor_visible(window);
        }
    }

    fn handle_focus_loss(&mut self, window: &Window) {
        if self.mouse_hidden {
            self.request_cursor_visible(window);
            self.cursor_resync_needed = true;
        }
    }

    fn handle_focus_change(&mut self, window: &Window, focused: bool) {
        if focused {
            self.handle_focus_gain(window);
        } else {
            self.handle_focus_loss(window);
        }
    }

    pub fn get_window_details_under_mouse<'b>(
        &self,
        editor_state: &'b EditorState<'b>,
    ) -> Option<&'b WindowDrawDetails> {
        let position = self.window_position;
        if self
            .settings
            .get::<WindowSettings>()
            .has_mouse_grid_detection
        {
            Some(&editor_state.full_region)
        } else {
            // the rendered window regions are sorted by draw order, so the earlier windows in the
            // list are drawn under the later ones
            editor_state
                .window_regions
                .iter()
                .rfind(|details| details.region.contains(&position))
        }
    }

    pub fn get_relative_position(
        &self,
        window_details: &WindowDrawDetails,
        editor_state: &EditorState,
    ) -> GridPos<u32> {
        let relative_position = (self.window_position - window_details.region.min).to_point();
        (relative_position / *editor_state.grid_scale)
            .floor()
            .max((0.0, 0.0).into())
            .try_cast()
            .unwrap()
            .min(Point2::new(
                window_details.grid_size.width.max(1) - 1,
                window_details.grid_size.height.max(1) - 1,
            ))
    }

    fn handle_pointer_motion(&mut self, position: PixelPos<f32>, editor_state: &EditorState) {
        let window_size = editor_state.window.inner_size();
        let window_size = PixelSize::new(window_size.width as f32, window_size.height as f32);
        let relative_window_rect = PixelRect::from_size(window_size);

        self.window_position = position;

        // If dragging, the relevant window (the one which we send all commands to) is the one
        // which the mouse drag started on. Otherwise its the top rendered window
        let window_details = if let Some(drag_details) = &self.drag_details {
            if self
                .settings
                .get::<WindowSettings>()
                .has_mouse_grid_detection
            {
                Some(&editor_state.full_region)
            } else {
                editor_state
                    .window_regions
                    .iter()
                    .find(|details| details.id == drag_details.draw_details.id)
            }
        } else {
            if !relative_window_rect.contains(&position) {
                return;
            }

            self.get_window_details_under_mouse(editor_state)
        };

        if let Some(window_details) = window_details {
            let relative_position = self.get_relative_position(window_details, editor_state);
            let previous_position = self.grid_position;
            self.grid_position = relative_position;

            let has_moved = self.grid_position != previous_position;

            if has_moved {
                if let Some(drag_details) = &self.drag_details {
                    send_ui(SerialCommand::Drag {
                        button: mouse_button_to_button_text(drag_details.button).unwrap(),
                        grid_id: window_details.event_grid_id(&self.settings),
                        position: self.grid_position.to_tuple(),
                        modifier_string: editor_state
                            .keyboard_manager
                            .format_modifier_string("", true),
                    });
                } else if self.settings.get::<WindowSettings>().mouse_move_event {
                    // Send a mouse move command
                    send_ui(SerialCommand::MouseButton {
                        button: "move".into(),
                        action: "".into(), // this is ignored by nvim
                        grid_id: window_details.event_grid_id(&self.settings),
                        position: relative_position.to_tuple(),
                        modifier_string: editor_state
                            .keyboard_manager
                            .format_modifier_string("", true),
                    })
                }
            }

            self.has_moved = self.drag_details.is_some() && (self.has_moved || has_moved);
        }
    }

    fn handle_pointer_transition(
        &mut self,
        mouse_button: MouseButton,
        down: bool,
        editor_state: &EditorState,
    ) {
        // For some reason pointer down is handled differently from pointer up and drag.
        // Floating windows: relative coordinates are great.
        // Non floating windows: rather than global coordinates, relative are needed
        if self.enabled {
            if let Some(button_text) = mouse_button_to_button_text(mouse_button) {
                if let &Some(details) = &self.get_window_details_under_mouse(editor_state) {
                    let action = if down {
                        "press".to_owned()
                    } else {
                        "release".to_owned()
                    };

                    let position = if !down && self.has_moved {
                        self.grid_position
                    } else {
                        self.get_relative_position(details, editor_state)
                    };

                    send_ui(SerialCommand::MouseButton {
                        button: button_text.clone(),
                        action,
                        grid_id: details.event_grid_id(&self.settings),
                        position: position.to_tuple(),
                        modifier_string: editor_state
                            .keyboard_manager
                            .format_modifier_string("", true),
                    });

                    if down {
                        self.drag_details = Some(DragDetails {
                            button: mouse_button,
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

    fn handle_line_scroll(&mut self, amount: GridVec<f32>, editor_state: &EditorState) {
        if !self.enabled {
            return;
        }

        let draw_details = self.get_window_details_under_mouse(editor_state);
        let grid_id = draw_details
            .map(|details| details.event_grid_id(&self.settings))
            .unwrap_or(0);

        let previous: GridPos<i32> = self.scroll_position.floor().try_cast().unwrap();
        self.scroll_position += amount;
        let new: GridPos<i32> = self.scroll_position.floor().try_cast().unwrap();

        let vertical_input_type = match new.y.partial_cmp(&previous.y) {
            Some(Ordering::Greater) => Some("up"),
            Some(Ordering::Less) => Some("down"),
            _ => None,
        };

        if let Some(input_type) = vertical_input_type {
            let scroll_command = SerialCommand::Scroll {
                direction: input_type.to_string(),
                grid_id,
                position: self.grid_position.to_tuple(),
                modifier_string: editor_state
                    .keyboard_manager
                    .format_modifier_string("", true),
            };
            for _ in 0..(new.y - previous.y).abs() {
                send_ui(scroll_command.clone());
            }
        }

        let horizontal_input_type = match new.x.partial_cmp(&previous.x) {
            Some(Ordering::Greater) => Some("left"),
            Some(Ordering::Less) => Some("right"),
            _ => None,
        };

        if let Some(input_type) = horizontal_input_type {
            let scroll_command = SerialCommand::Scroll {
                direction: input_type.to_string(),
                grid_id,
                position: self.grid_position.to_tuple(),
                modifier_string: editor_state
                    .keyboard_manager
                    .format_modifier_string("", true),
            };
            for _ in 0..(new.x - previous.x).abs() {
                send_ui(scroll_command.clone());
            }
        }
    }

    fn handle_pixel_scroll(&mut self, amount: PixelVec<f32>, editor_state: &EditorState) {
        let amount = amount / *editor_state.grid_scale;
        self.handle_line_scroll(amount, editor_state);
    }

    fn handle_touch(
        &mut self,
        finger_id: (DeviceId, u64),
        location: PixelPos<f32>,
        phase: &TouchPhase,
        editor_state: &EditorState,
    ) {
        match phase {
            TouchPhase::Started => {
                let settings = self.settings.get::<WindowSettings>();
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

                        let settings = self.settings.get::<WindowSettings>();
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
                        self.handle_pointer_motion((location.x, location.y).into(), editor_state);
                    }
                    // the double check might seem useless, but the if branch above might set
                    // trace.left_deadzone_once - which urges to check again
                    else if trace.left_deadzone_once {
                        let delta = (trace.last.x - location.x, location.y - trace.last.y).into();

                        // not updating the position would cause the movement to "escalate" from the
                        // starting point
                        trace.last = location;

                        self.handle_pixel_scroll(delta, editor_state);
                    }
                }

                if dragging_just_now {
                    self.handle_pointer_motion((location.x, location.y).into(), editor_state);
                    self.handle_pointer_transition(MouseButton::Left, true, editor_state);
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                if let Some(trace) = self.touch_position.remove(&finger_id) {
                    if self.drag_details.is_some() {
                        self.handle_pointer_transition(MouseButton::Left, false, editor_state);
                    }
                    if !trace.left_deadzone_once {
                        self.handle_pointer_motion(
                            (trace.start.x, trace.start.y).into(),
                            editor_state,
                        );
                        self.handle_pointer_transition(MouseButton::Left, true, editor_state);
                        self.handle_pointer_transition(MouseButton::Left, false, editor_state);
                    }
                }
            }
        }
    }

    pub fn handle_event(
        &mut self,
        event: &WindowEvent,
        keyboard_manager: &KeyboardManager,
        renderer: &Renderer,
        window: &Window,
    ) {
        let full_region = WindowDrawDetails {
            id: 0,
            region: renderer
                .window_regions
                .first()
                .map_or(PixelRect::ZERO, |v| v.region),
            grid_size: renderer
                .window_regions
                .first()
                .map_or(GridSize::ZERO, |v| v.grid_size),
        };
        let editor_state = EditorState {
            grid_scale: &renderer.grid_renderer.grid_scale,
            window_regions: &renderer.window_regions,
            full_region,
            window,
            keyboard_manager,
        };
        let hide_mouse_when_typing = self.settings.get::<WindowSettings>().hide_mouse_when_typing;
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.handle_pointer_motion(
                    (position.x as f32, position.y as f32).into(),
                    &editor_state,
                );
                if self.mouse_hidden && window.has_focus() {
                    self.request_cursor_visible(window);
                } else if self.cursor_resync_needed && window.has_focus() {
                    self.force_cursor_visible(window);
                }
            }
            WindowEvent::CursorEntered { .. } => {
                if self.mouse_hidden {
                    self.request_cursor_visible(window);
                } else if self.cursor_resync_needed && window.has_focus() {
                    self.force_cursor_visible(window);
                }
            }
            WindowEvent::MouseWheel {
                delta: MouseScrollDelta::LineDelta(x, y),
                ..
            } => self.handle_line_scroll((*x, *y).into(), &editor_state),
            WindowEvent::MouseWheel {
                delta: MouseScrollDelta::PixelDelta(delta),
                ..
            } => self.handle_pixel_scroll((delta.x as f32, delta.y as f32).into(), &editor_state),
            WindowEvent::Touch(Touch {
                device_id,
                id,
                location,
                phase,
                ..
            }) => self.handle_touch(
                (*device_id, *id),
                PixelPos::new(location.x as f32, location.y as f32),
                phase,
                &editor_state,
            ),
            WindowEvent::MouseInput { button, state, .. } => self.handle_pointer_transition(
                *button,
                state == &ElementState::Pressed,
                &editor_state,
            ),

            WindowEvent::KeyboardInput {
                event: key_event, ..
            } if hide_mouse_when_typing
                && key_event.state == ElementState::Pressed
                && !self.mouse_hidden
                && window.has_focus() =>
            {
                self.hide_cursor(window);
            }
            WindowEvent::Focused(focused_event) if hide_mouse_when_typing => {
                self.handle_focus_change(window, *focused_event);
            }
            _ => {}
        }
    }
}
