use std::time::{Duration, Instant};

use winit::{
    event::{Event, WindowEvent},
    event_loop::ControlFlow,
};

#[cfg(target_os = "macos")]
use super::draw_background;
use super::{WindowSettings, WinitWindowWrapper};
use crate::{
    profiling::{tracy_create_gpu_context, tracy_zone},
    running_tracker::*,
    settings::{save_window_size, SETTINGS},
};

enum FocusedState {
    Focused,
    UnfocusedNotDrawn,
    Unfocused,
}

pub struct UpdateLoop {
    previous_frame_start: Instant,
    focused: FocusedState,
}

impl UpdateLoop {
    pub fn new() -> Self {
        tracy_create_gpu_context("main_render_context");

        let previous_frame_start = Instant::now();
        let focused = FocusedState::Focused;

        Self {
            previous_frame_start,
            focused,
        }
    }

    fn get_event_deadline(&self) -> Instant {
        let refresh_rate = match self.focused {
            FocusedState::Focused | FocusedState::UnfocusedNotDrawn => {
                SETTINGS.get::<WindowSettings>().refresh_rate as f32
            }
            FocusedState::Unfocused => SETTINGS.get::<WindowSettings>().refresh_rate_idle as f32,
        }
        .max(1.0);

        let expected_frame_duration = Duration::from_secs_f32(1.0 / refresh_rate);
        self.previous_frame_start + expected_frame_duration
    }

    pub fn step(
        &mut self,
        window_wrapper: &mut WinitWindowWrapper,
        event: Event<()>,
    ) -> ControlFlow {
        tracy_zone!("render loop", 0);

        // Window focus changed
        if let Event::WindowEvent {
            event: WindowEvent::Focused(focused_event),
            ..
        } = event
        {
            self.focused = if focused_event {
                FocusedState::Focused
            } else {
                FocusedState::UnfocusedNotDrawn
            };
        }

        let deadline = self.get_event_deadline();

        if !RUNNING_TRACKER.is_running() {
            let window = window_wrapper.windowed_context.window();
            save_window_size(
                window.is_maximized(),
                window.inner_size(),
                window.outer_position().ok(),
            );

            std::process::exit(RUNNING_TRACKER.exit_code());
        }

        let frame_start = Instant::now();

        window_wrapper.handle_window_commands();
        window_wrapper.synchronize_settings();
        window_wrapper.handle_event(event);

        let refresh_rate = match self.focused {
            FocusedState::Focused | FocusedState::UnfocusedNotDrawn => {
                SETTINGS.get::<WindowSettings>().refresh_rate as f32
            }
            FocusedState::Unfocused => SETTINGS.get::<WindowSettings>().refresh_rate_idle as f32,
        }
        .max(0.0);

        let expected_frame_length_seconds = 1.0 / refresh_rate;
        let frame_duration = Duration::from_secs_f32(expected_frame_length_seconds);

        if frame_start - self.previous_frame_start > frame_duration {
            let dt = self.previous_frame_start.elapsed().as_secs_f32();
            window_wrapper.draw_frame(dt);
            if let FocusedState::UnfocusedNotDrawn = self.focused {
                self.focused = FocusedState::Unfocused;
            }
            self.previous_frame_start = frame_start;
            #[cfg(target_os = "macos")]
            draw_background(window_wrapper.windowed_context.window());
        }

        ControlFlow::WaitUntil(deadline)
    }
}
