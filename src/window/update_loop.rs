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
    renderer::VSync,
    settings::SETTINGS,
};

enum FocusedState {
    Focused,
    UnfocusedNotDrawn,
    Unfocused,
}

#[derive(Debug, PartialEq)]
pub enum ShouldRender {
    Immediately,
    Wait,
    Deadline(Instant),
}

impl ShouldRender {
    pub fn update(&mut self, rhs: ShouldRender) {
        let lhs = &self;
        match (lhs, rhs) {
            (ShouldRender::Immediately, _) => {}
            (_, ShouldRender::Immediately) => {
                *self = ShouldRender::Immediately;
            }
            (ShouldRender::Deadline(lhs), ShouldRender::Deadline(rhs)) => {
                if rhs < *lhs {
                    *self = ShouldRender::Deadline(rhs);
                }
            }
            (ShouldRender::Deadline(_), ShouldRender::Wait) => {}
            (ShouldRender::Wait, ShouldRender::Deadline(instant)) => {
                *self = ShouldRender::Deadline(instant);
            }
            (ShouldRender::Wait, ShouldRender::Wait) => {}
        }
    }
}

const MAX_ANIMATION_DT: f32 = 1.0 / 120.0;

pub struct UpdateLoop {
    idle: bool,
    previous_frame_start: Instant,
    last_dt: f32,
    frame_dt_avg: NoSumSMA<f64, f64, 10>,
    should_render: ShouldRender,
    num_consecutive_rendered: u32,
    focused: FocusedState,
}

impl UpdateLoop {
    pub fn new(idle: bool) -> Self {
        tracy_create_gpu_context("main_render_context");

        let previous_frame_start = Instant::now();
        let last_dt = 0.0;
        let frame_dt_avg = NoSumSMA::new();
        let should_render = ShouldRender::Immediately;
        let num_consecutive_rendered = 0;
        let focused = FocusedState::Focused;

        Self {
            idle,
            previous_frame_start,
            last_dt,
            frame_dt_avg,
            should_render,
            num_consecutive_rendered,
            focused,
        }
    }

    pub fn get_event_wait_time(&self, vsync: &VSync) -> (Duration, Instant) {
        let refresh_rate = match self.focused {
            // NOTE: Always wait for the idle refresh rate when winit throttling is used to avoid waking up too early
            // The winit redraw request will likely happen much before that and wake it up anyway
            FocusedState::Focused | FocusedState::UnfocusedNotDrawn
                if !vsync.uses_winit_throttling() =>
            {
                SETTINGS.get::<WindowSettings>().refresh_rate as f32
            }
            _ => SETTINGS.get::<WindowSettings>().refresh_rate_idle as f32,
        }
        .max(1.0);

        let expected_frame_duration = Duration::from_secs_f32(1.0 / refresh_rate);
        if self.should_render == ShouldRender::Immediately && !vsync.uses_winit_throttling() {
            // Only poll when using native vsync
            (Duration::from_nanos(0), Instant::now())
        } else {
            let mut deadline = self.previous_frame_start + expected_frame_duration;
            deadline = match self.should_render {
                ShouldRender::Deadline(should_render_deadline) => {
                    should_render_deadline.min(deadline)
                }
                _ => deadline,
            };
            (deadline.saturating_duration_since(Instant::now()), deadline)
        }
    }

    pub fn render(&mut self, window_wrapper: &mut WinitWindowWrapper) {
        let dt = if self.num_consecutive_rendered > 0 && self.frame_dt_avg.get_num_samples() > 0 {
            self.frame_dt_avg.get_average() as f32
        } else {
            self.last_dt
        }
        .min(1.0);
        self.should_render = window_wrapper.prepare_frame();
        let num_steps = (dt / MAX_ANIMATION_DT).ceil();
        let step = dt / num_steps;
        for _ in 0..num_steps as usize {
            if window_wrapper.animate_frame(step) {
                self.should_render = ShouldRender::Immediately;
            }
        }
        window_wrapper.draw_frame(self.last_dt);

        if self.num_consecutive_rendered > 2 {
            self.frame_dt_avg
                .add_sample(self.previous_frame_start.elapsed().as_secs_f64());
        }

        if let FocusedState::UnfocusedNotDrawn = self.focused {
            self.focused = FocusedState::Unfocused;
        }

        #[cfg(target_os = "macos")]
        draw_background(window_wrapper.windowed_context.window());

        self.num_consecutive_rendered += 1;
        self.last_dt = self.previous_frame_start.elapsed().as_secs_f32();
        self.previous_frame_start = Instant::now();
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
            Ok(Event::AboutToWait) | Err(false) => {
                self.should_render.update(window_wrapper.prepare_frame());
                if self.should_render == ShouldRender::Immediately || !self.idle {
                    if window_wrapper.vsync.uses_winit_throttling() {
                        window_wrapper.windowed_context.window().request_redraw();
                    } else {
                        self.render(window_wrapper);
                    }
                } else {
                    self.num_consecutive_rendered = 0;
                    self.last_dt = self.previous_frame_start.elapsed().as_secs_f32();
                    self.previous_frame_start = Instant::now();
                }
            }
            Ok(Event::WindowEvent {
                event: WindowEvent::RedrawRequested,
                ..
            }) => {
                self.render(window_wrapper);
            }
            _ => {}
        }

        if let Ok(event) = event {
            if window_wrapper.handle_event(event) {
                self.should_render = ShouldRender::Immediately;
            }
        }

        let (_, deadline) = self.get_event_wait_time(&window_wrapper.vsync);
        Ok(ControlFlow::WaitUntil(deadline))
    }
}
