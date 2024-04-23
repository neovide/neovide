use std::time::{Duration, Instant};

use winit::{
    event::{Event, WindowEvent},
    event_loop::ControlFlow,
};

use super::{UserEvent, WindowSettings, WinitWindowWrapper};
use crate::{
    profiling::{tracy_plot, tracy_zone},
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

const MAX_ANIMATION_DT: f64 = 1.0 / 120.0;

pub struct UpdateLoop {
    idle: bool,
    previous_frame_start: Instant,
    last_dt: f32,
    should_render: ShouldRender,
    num_consecutive_rendered: u32,
    focused: FocusedState,
    pending_render: bool, // We should render as soon as the compositor/vsync allows
    pending_draw_commands: Vec<Event<UserEvent>>,
    animation_start: Instant, // When the last animation started (went from idle to animating)
    animation_time: Duration, // How long the current animation has been simulated, will usually be in the future
}

impl UpdateLoop {
    pub fn new(idle: bool) -> Self {
        let previous_frame_start = Instant::now();
        let last_dt = 0.0;
        let should_render = ShouldRender::Immediately;
        let num_consecutive_rendered = 0;
        let focused = FocusedState::Focused;
        let pending_render = false;
        let pending_draw_commands = Vec::new();
        let animation_start = Instant::now();
        let animation_time = Duration::from_millis(0);

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
            animation_time,
        }
    }

    fn get_refresh_rate(&self) -> f32 {
        match self.focused {
            // NOTE: Always wait for the idle refresh rate when winit throttling is used to avoid waking up too early
            // The winit redraw request will likely happen much before that and wake it up anyway
            FocusedState::Focused | FocusedState::UnfocusedNotDrawn => {
                SETTINGS.get::<WindowSettings>().refresh_rate as f32
            }
            _ => SETTINGS.get::<WindowSettings>().refresh_rate_idle as f32,
        }
        .max(1.0)
    }

    fn get_frame_deadline(&self) -> Instant {
        let refresh_rate = self.get_refresh_rate();
        let expected_frame_duration = Duration::from_secs_f32(1.0 / refresh_rate);
        self.previous_frame_start + expected_frame_duration
    }

    fn get_event_deadline(&self) -> Instant {
        // When there's a pending render we don't need to wait for anything else than the render event
        if self.pending_render {
            return self.animation_start + self.animation_time;
        }

        match self.should_render {
            ShouldRender::Immediately => Instant::now(),
            ShouldRender::Deadline(old_deadline) => old_deadline.min(self.get_frame_deadline()),
            _ => self.get_frame_deadline(),
        }
    }

    fn animate(&mut self, window_wrapper: &mut WinitWindowWrapper) {
        let dt = Duration::from_secs_f32(
            window_wrapper
                .vsync
                .get_refresh_rate(window_wrapper.skia_renderer.window()),
        );

        let now = Instant::now();
        let target_animation_time = now - self.animation_start;
        let mut delta = target_animation_time.saturating_sub(self.animation_time);
        // Don't try to animate way too big deltas
        // Instead reset the animation times, and simulate a single frame
        if delta > Duration::from_millis(1000) {
            self.animation_start = now;
            self.animation_time = Duration::ZERO;
            delta = dt;
        }
        // Catchup immediately if the delta is more than one frame, otherwise smooth it over 10 frames
        let catchup = if delta >= dt {
            delta
        } else {
            delta.div_f64(10.0)
        };

        let dt = dt + catchup;
        tracy_plot!("Simulation dt", dt.as_secs_f64());
        self.animation_time += dt;

        let num_steps = (dt.as_secs_f64() / MAX_ANIMATION_DT).ceil() as u32;
        let step = dt / num_steps;
        for _ in 0..num_steps {
            if window_wrapper.animate_frame(step.as_secs_f32()) {
                self.should_render = ShouldRender::Immediately;
            }
        }
    }

    fn render(&mut self, window_wrapper: &mut WinitWindowWrapper) {
        self.pending_render = false;
        tracy_plot!("pending_render", self.pending_render as u8 as f64);
        window_wrapper.draw_frame(self.last_dt);

        if let FocusedState::UnfocusedNotDrawn = self.focused {
            self.focused = FocusedState::Unfocused;
        }

        self.num_consecutive_rendered += 1;
        tracy_plot!(
            "num_consecutive_rendered",
            self.num_consecutive_rendered as f64
        );
        self.last_dt = self.previous_frame_start.elapsed().as_secs_f32();
        self.previous_frame_start = Instant::now();
    }

    fn process_buffered_draw_commands(&mut self, window_wrapper: &mut WinitWindowWrapper) {
        for e in self.pending_draw_commands.drain(..) {
            if window_wrapper.handle_event(e) {
                self.should_render = ShouldRender::Immediately;
            }
        }
    }

    fn reset_animation_period(&mut self) {
        self.should_render = ShouldRender::Wait;
        if self.num_consecutive_rendered == 0 {
            self.animation_start = Instant::now();
            self.animation_time = Duration::ZERO;
        }
    }

    fn schedule_render(&mut self, skipped_frame: bool, window_wrapper: &mut WinitWindowWrapper) {
        // There's really no point in trying to render if the frame is skipped
        // (most likely due to the compositor being busy). The animated frame will
        // be rendered at an appropriate time anyway.
        if !skipped_frame {
            // When winit throttling is used, request a redraw and wait for the render event
            // Otherwise render immediately
            if window_wrapper.vsync.uses_winit_throttling() {
                window_wrapper
                    .vsync
                    .request_redraw(window_wrapper.skia_renderer.window());
                self.pending_render = true;
                tracy_plot!("pending_render", self.pending_render as u8 as f64);
            } else {
                self.render(window_wrapper);
            }
        }
    }

    fn prepare_and_animate(&mut self, window_wrapper: &mut WinitWindowWrapper) {
        // We will also animate, but not render when frames are skipped or a bit late, to reduce visual artifacts
        let skipped_frame =
            self.pending_render && Instant::now() > (self.animation_start + self.animation_time);
        let should_prepare = !self.pending_render || skipped_frame;
        if !should_prepare {
            window_wrapper
                .renderer
                .grid_renderer
                .shaper
                .cleanup_font_cache();
            return;
        }

        let res = window_wrapper.prepare_frame();
        self.should_render.update(res);

        let should_animate =
            self.should_render == ShouldRender::Immediately || !self.idle || skipped_frame;

        if should_animate {
            self.reset_animation_period();
            self.animate(window_wrapper);
            self.schedule_render(skipped_frame, window_wrapper);
        } else {
            self.num_consecutive_rendered = 0;
            tracy_plot!(
                "num_consecutive_rendered",
                self.num_consecutive_rendered as f64
            );
            self.last_dt = self.previous_frame_start.elapsed().as_secs_f32();
            self.previous_frame_start = Instant::now();
        }
    }

    pub fn step(
        &mut self,
        window_wrapper: &mut WinitWindowWrapper,
        event: Event<UserEvent>,
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
            Event::AboutToWait => {
                self.prepare_and_animate(window_wrapper);
            }
            Event::WindowEvent {
                event: WindowEvent::RedrawRequested,
                ..
            }
            | Event::UserEvent(UserEvent::RedrawRequested) => {
                if self.pending_render {
                    tracy_zone!("render (redraw requested)");
                    self.render(window_wrapper);
                    // We should process all buffered draw commands as soon as the rendering has finished
                    self.process_buffered_draw_commands(window_wrapper);
                } else {
                    tracy_zone!("redraw requested");
                    // The OS itself asks us to redraw, so we need to prepare first
                    self.should_render = ShouldRender::Immediately;
                }
            }
            _ => {}
        }

        if self.pending_render && matches!(&event, Event::UserEvent(UserEvent::DrawCommandBatch(_)))
        {
            // Buffer the draw commands if we have a pending render, we have already decided what to
            // draw, so it's not a good idea to process them now.
            // They will be processed immediately after the rendering.
            self.pending_draw_commands.push(event);
        } else if window_wrapper.handle_event(event) {
            // But we need to handle other events (in the if statement itself)
            // Also schedule a render as soon as possible
            self.should_render = ShouldRender::Immediately;
        }

        #[cfg(feature = "profiling")]
        self.should_render.plot_tracy();

        ControlFlow::WaitUntil(self.get_event_deadline())
    }
}
