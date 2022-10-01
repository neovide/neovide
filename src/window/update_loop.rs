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
    redraw_scheduler::REDRAW_SCHEDULER,
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
    dt: f32,
    focused: FocusedState,
}

impl UpdateLoop {
    pub fn new() -> Self {
        tracy_create_gpu_context("main_render_context");

        let previous_frame_start = Instant::now();
        let dt = 0.0;
        let focused = FocusedState::Focused;

        Self {
            previous_frame_start,
            dt,
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

        let mut skipped_rendering = false;
        let deadline = self.get_event_deadline();

        match event {
            // Window focus changed
            Event::WindowEvent {
                event: WindowEvent::Focused(focused_event),
                ..
            } => {
                self.focused = if focused_event {
                    FocusedState::Focused
                } else {
                    FocusedState::UnfocusedNotDrawn
                };
            }
            Event::MainEventsCleared => {
                window_wrapper.prepare_frame();
                window_wrapper.animate_frame(self.dt);
                if REDRAW_SCHEDULER.should_draw() || !SETTINGS.get::<WindowSettings>().idle {
                    window_wrapper.draw_frame(self.dt)
                } else {
                    skipped_rendering = true;
                }
                self.dt = self.previous_frame_start.elapsed().as_secs_f32();
                self.previous_frame_start = Instant::now();

                if let FocusedState::UnfocusedNotDrawn = self.focused {
                    self.focused = FocusedState::Unfocused;
                }

                #[cfg(target_os = "macos")]
                draw_background(window_wrapper.windowed_context.window());
            }
            _ => (),
        }

        if !RUNNING_TRACKER.is_running() {
            let window = window_wrapper.windowed_context.window();
            save_window_size(
                window.is_maximized(),
                window.inner_size(),
                window.outer_position().ok(),
            );

            std::process::exit(RUNNING_TRACKER.exit_code());
        }

        window_wrapper.handle_window_commands();
        window_wrapper.synchronize_settings();
        window_wrapper.handle_event(event);

        if !skipped_rendering {
            ControlFlow::Poll
        } else {
            ControlFlow::WaitUntil(deadline)
        }
    }
}
