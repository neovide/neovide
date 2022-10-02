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

const MAX_ANIMATION_DT: f32 = 1.0 / 120.0;

pub struct UpdateLoop {
    idle: bool,
    previous_frame_start: Instant,
    dt: f32,
    should_render: bool,
    num_consecutive_rendered: u32,
    focused: FocusedState,
}

impl UpdateLoop {
    pub fn new(idle: bool) -> Self {
        tracy_create_gpu_context("main_render_context");

        let previous_frame_start = Instant::now();
        let dt = 0.0;
        let should_render = true;
        let num_consecutive_rendered = 0;
        let focused = FocusedState::Focused;

        Self {
            idle,
            previous_frame_start,
            dt,
            should_render,
            num_consecutive_rendered,
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
                self.should_render |= window_wrapper.prepare_frame();
                let num_steps = (self.dt / MAX_ANIMATION_DT).ceil();
                let step = self.dt / num_steps;
                for _ in 0..num_steps as usize {
                    self.should_render |= window_wrapper.animate_frame(step);
                }
                if self.should_render || !self.idle {
                    window_wrapper.draw_frame(self.dt);
                    self.should_render = false;
                    self.num_consecutive_rendered += 1;
                } else {
                    self.num_consecutive_rendered = 0;
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
        self.should_render |= window_wrapper.handle_event(event);

        if self.num_consecutive_rendered > 0 {
            ControlFlow::Poll
        } else {
            ControlFlow::WaitUntil(self.get_event_deadline())
        }
    }
}
