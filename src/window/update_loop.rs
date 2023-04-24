use simple_moving_average::{NoSumSMA, SMA};
use std::time::{Duration, Instant};

use winit::{
    event::{Event, WindowEvent},
    event_loop::ControlFlow,
};

#[cfg(target_os = "macos")]
use super::draw_background;
use super::{UserEvent, WindowSettings, WinitWindowWrapper};
use crate::{
    profiling::{tracy_create_gpu_context, tracy_zone},
    renderer::{VSync, WindowedContext},
    settings::SETTINGS,
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
    last_dt: f32,
    frame_dt_avg: NoSumSMA<f64, f64, 10>,
    should_render: bool,
    num_consecutive_rendered: u32,
    focused: FocusedState,
    vsync: VSync,
}

impl UpdateLoop {
    pub fn new(vsync_enabled: bool, idle: bool, context: &WindowedContext) -> Self {
        tracy_create_gpu_context("main_render_context");

        let previous_frame_start = Instant::now();
        let last_dt = 0.0;
        let frame_dt_avg = NoSumSMA::new();
        let should_render = true;
        let num_consecutive_rendered = 0;
        let focused = FocusedState::Focused;
        let vsync = VSync::new(vsync_enabled, context);

        Self {
            idle,
            previous_frame_start,
            last_dt,
            frame_dt_avg,
            should_render,
            num_consecutive_rendered,
            focused,
            vsync,
        }
    }

    pub fn get_event_wait_time(&self) -> (Duration, Instant) {
        let refresh_rate = match self.focused {
            FocusedState::Focused | FocusedState::UnfocusedNotDrawn => {
                SETTINGS.get::<WindowSettings>().refresh_rate as f32
            }
            FocusedState::Unfocused => SETTINGS.get::<WindowSettings>().refresh_rate_idle as f32,
        }
        .max(1.0);

        let expected_frame_duration = Duration::from_secs_f32(1.0 / refresh_rate);
        if self.num_consecutive_rendered > 0 {
            (Duration::from_nanos(0), Instant::now())
        } else {
            let deadline = self.previous_frame_start + expected_frame_duration;
            (deadline.saturating_duration_since(Instant::now()), deadline)
        }
    }

    pub fn step(
        &mut self,
        window_wrapper: &mut WinitWindowWrapper,
        event: Result<Event<UserEvent>, bool>,
    ) -> Result<ControlFlow, ()> {
        tracy_zone!("render loop", 0);

        match event {
            // Window focus changed
            Ok(Event::WindowEvent {
                event: WindowEvent::Focused(focused_event),
                ..
            }) => {
                self.focused = if focused_event {
                    FocusedState::Focused
                } else {
                    FocusedState::UnfocusedNotDrawn
                };
            }
            Err(true) => {
                // Disconnected
                return Err(());
            }
            Err(false) | Ok(Event::MainEventsCleared) => {
                let dt = if self.num_consecutive_rendered > 0
                    && self.frame_dt_avg.get_num_samples() > 0
                {
                    self.frame_dt_avg.get_average() as f32
                } else {
                    self.last_dt
                }
                .min(1.0);
                self.should_render |= window_wrapper.prepare_frame();
                let num_steps = (dt / MAX_ANIMATION_DT).ceil();
                let step = dt / num_steps;
                for _ in 0..num_steps as usize {
                    self.should_render |= window_wrapper.animate_frame(step);
                }
                if self.should_render || !self.idle {
                    window_wrapper.draw_frame(&mut self.vsync, self.last_dt);

                    if self.num_consecutive_rendered > 2 {
                        self.frame_dt_avg
                            .add_sample(self.previous_frame_start.elapsed().as_secs_f64());
                    }
                    self.should_render = false;
                    self.num_consecutive_rendered += 1;
                } else {
                    self.num_consecutive_rendered = 0;
                }
                self.last_dt = self.previous_frame_start.elapsed().as_secs_f32();
                self.previous_frame_start = Instant::now();
                if let FocusedState::UnfocusedNotDrawn = self.focused {
                    self.focused = FocusedState::Unfocused;
                }

                #[cfg(target_os = "macos")]
                draw_background(window_wrapper.windowed_context.window());
            }
            _ => {}
        }
        window_wrapper.handle_window_commands();
        window_wrapper.synchronize_settings();

        if let Ok(event) = event {
            self.should_render |= window_wrapper.handle_event(event);
        }

        let (_, deadline) = self.get_event_wait_time();

        if self.num_consecutive_rendered > 0 {
            Ok(ControlFlow::Poll)
        } else {
            Ok(ControlFlow::WaitUntil(deadline))
        }
    }
}
