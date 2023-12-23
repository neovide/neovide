use std::time::{Duration, Instant};

use winit::{
    event::{Event, WindowEvent},
    event_loop::ControlFlow,
};

#[cfg(target_os = "macos")]
use super::draw_background;
use super::{UserEvent, WindowSettings, WinitWindowWrapper};
use crate::{
    profiling::{tracy_create_gpu_context, tracy_plot, tracy_zone},
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

    #[cfg(feature = "profiling")]
    fn plot_tracy(&self) {
        match &self {
            ShouldRender::Immediately => {
                tracy_plot!("should_render", 0.0);
            }
            ShouldRender::Wait => {
                tracy_plot!("should_render", -1.0);
            }
            ShouldRender::Deadline(instant) => {
                tracy_plot!(
                    "should_render",
                    instant
                        .saturating_duration_since(Instant::now())
                        .as_secs_f64()
                );
            }
        }
    }
}

const MAX_ANIMATION_DT: f32 = 1.0 / 120.0;

pub struct UpdateLoop {
    idle: bool,
    previous_frame_start: Instant,
    last_dt: f32,
    should_render: ShouldRender,
    num_consecutive_rendered: u32,
    focused: FocusedState,
    pending_render: bool,
    pending_draw_commands: Vec<Event<UserEvent>>,
    animation_start: Instant,
    simulation_time: Duration,
}

impl UpdateLoop {
    pub fn new(idle: bool) -> Self {
        tracy_create_gpu_context("main_render_context");

        let previous_frame_start = Instant::now();
        let last_dt = 0.0;
        let should_render = ShouldRender::Immediately;
        let num_consecutive_rendered = 0;
        let focused = FocusedState::Focused;
        let pending_render = false;
        let pending_draw_commands = Vec::new();
        let animation_start = Instant::now();
        let simulation_time = Duration::from_millis(0);

        Self {
            idle,
            previous_frame_start,
            last_dt,
            should_render,
            num_consecutive_rendered,
            focused,
            pending_render,
            pending_draw_commands,
            animation_start,
            simulation_time,
        }
    }

    pub fn get_event_wait_time(&self) -> (Duration, Instant) {
        let refresh_rate = match self.focused {
            // NOTE: Always wait for the idle refresh rate when winit throttling is used to avoid waking up too early
            // The winit redraw request will likely happen much before that and wake it up anyway
            FocusedState::Focused | FocusedState::UnfocusedNotDrawn => {
                SETTINGS.get::<WindowSettings>().refresh_rate as f32
            }
            _ => SETTINGS.get::<WindowSettings>().refresh_rate_idle as f32,
        }
        .max(1.0);

        let expected_frame_duration = Duration::from_secs_f32(1.0 / refresh_rate);
        if self.should_render == ShouldRender::Immediately && !self.pending_render {
            (Duration::from_nanos(0), Instant::now())
        } else if self.pending_render {
            let deadline = self.animation_start + self.simulation_time;
            (deadline.saturating_duration_since(Instant::now()), deadline)
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

    pub fn animate(&mut self, window_wrapper: &mut WinitWindowWrapper) {
        let dt = window_wrapper
            .vsync
            .get_refresh_rate(&window_wrapper.windowed_context);

        let now = Instant::now();
        let animation_time = (now - self.animation_start).as_secs_f64();
        let delta = animation_time - self.simulation_time.as_secs_f64();
        // Catchup immediately if the delta is more than one frame, otherwise smooth it over 10 frames
        let catchup = if delta >= dt as f64 {
            delta
        } else {
            delta / 10.0
        };

        let dt = (dt + catchup as f32).max(0.0);
        tracy_plot!("Simulation dt", dt as f64);
        self.simulation_time += Duration::from_secs_f32(dt);

        let num_steps = (dt / MAX_ANIMATION_DT).ceil();
        let step = dt / num_steps;
        for _ in 0..num_steps as usize {
            if window_wrapper.animate_frame(step) {
                self.should_render = ShouldRender::Immediately;
            }
        }
    }

    pub fn render(&mut self, window_wrapper: &mut WinitWindowWrapper) {
        self.pending_render = false;
        window_wrapper.draw_frame(self.last_dt);

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
                // We will also animate, but not render when frames are skipped(or very late) to reduce visual artifacts
                let skipped_frame = self.pending_render
                    && Instant::now() > (self.animation_start + self.simulation_time);
                let should_prepare = !self.pending_render || skipped_frame;
                if should_prepare {
                    self.should_render.update(window_wrapper.prepare_frame());
                    if self.should_render == ShouldRender::Immediately
                        || !self.idle
                        || skipped_frame
                    {
                        self.should_render = ShouldRender::Wait;
                        if self.num_consecutive_rendered == 0 {
                            self.animation_start = Instant::now();
                            self.simulation_time = Duration::from_millis(0);
                        }
                        self.animate(window_wrapper);
                        // There's really no point in trying to render if the frame is skipped
                        // (most likely due to the compositor being busy). The animated frame will
                        // be rendered at an appropriate time anyway.
                        if !skipped_frame {
                            // Always draw immediately for reduced latency if we have been idling
                            if self.num_consecutive_rendered > 0
                                && window_wrapper.vsync.uses_winit_throttling()
                            {
                                window_wrapper
                                    .vsync
                                    .request_redraw(&window_wrapper.windowed_context);
                                self.pending_render = true;
                            } else {
                                self.render(window_wrapper);
                            }
                        }
                    } else {
                        self.num_consecutive_rendered = 0;
                        self.last_dt = self.previous_frame_start.elapsed().as_secs_f32();
                        self.previous_frame_start = Instant::now();
                    }
                }
            }
            Ok(Event::WindowEvent {
                event: WindowEvent::RedrawRequested,
                ..
            })
            | Ok(Event::UserEvent(UserEvent::RedrawRequested)) => {
                tracy_zone!("render (redraw requested)");
                self.render(window_wrapper);
            }
            _ => {}
        }

        if !self.pending_render {
            for e in self.pending_draw_commands.drain(..) {
                if window_wrapper.handle_event(e) {
                    self.should_render = ShouldRender::Immediately;
                }
            }
        }

        if let Ok(event) = event {
            if self.pending_render
                && matches!(&event, Event::UserEvent(UserEvent::DrawCommandBatch(_)))
            {
                self.pending_draw_commands.push(event);
            } else if window_wrapper.handle_event(event) {
                self.should_render = ShouldRender::Immediately;
            }
        }
        #[cfg(feature = "profiling")]
        self.should_render.plot_tracy();

        let (_, deadline) = self.get_event_wait_time();
        Ok(ControlFlow::WaitUntil(deadline))
    }
}
