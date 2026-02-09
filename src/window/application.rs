use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy},
};

use super::{save_window_size, CmdLineSettings, UserEvent, WindowSettings, WinitWindowWrapper};
use crate::{
    bridge::{NeovimRuntime, RestartDetails},
    clipboard,
    profiling::{tracy_plot, tracy_zone},
    renderer::DrawCommand,
    settings::{Config, Settings},
    units::GridSize,
    WindowSize,
};
use log::error;

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

#[derive(Clone)]
struct RestartRequest {
    details: RestartDetails,
    grid_size: GridSize<u32>,
}

pub struct Application {
    idle: bool,
    previous_frame_start: Instant,
    last_dt: f32,
    should_render: ShouldRender,
    num_consecutive_rendered: u32,
    focused: FocusedState,
    pending_render: bool, // We should render as soon as the compositor/vsync allows
    pending_draw_commands: Vec<Vec<DrawCommand>>,
    animation_start: Instant, // When the last animation started (went from idle to animating)
    animation_time: Duration, // How long the current animation has been simulated, will usually be in the future

    window_wrapper: WinitWindowWrapper,
    create_window_allowed: bool,
    proxy: EventLoopProxy<UserEvent>,

    settings: Arc<Settings>,
    runtime: Option<NeovimRuntime>,
    clipboard: Option<Arc<Mutex<clipboard::Clipboard>>>,
    pending_restart: Option<RestartRequest>,
}

impl Application {
    pub fn new(
        initial_window_size: WindowSize,
        initial_config: Config,
        proxy: EventLoopProxy<UserEvent>,
        settings: Arc<Settings>,
        runtime: NeovimRuntime,
        clipboard: Arc<Mutex<clipboard::Clipboard>>,
    ) -> Self {
        let previous_frame_start = Instant::now();
        let last_dt = 0.0;
        let should_render = ShouldRender::Immediately;
        let num_consecutive_rendered = 0;
        let focused = FocusedState::Focused;
        let pending_render = false;
        let pending_draw_commands = Vec::new();
        let animation_start = Instant::now();
        let animation_time = Duration::from_millis(0);

        let cmd_line_settings = settings.get::<CmdLineSettings>();
        let idle = cmd_line_settings.idle;

        let window_wrapper =
            WinitWindowWrapper::new(initial_window_size, initial_config, settings.clone());

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

            window_wrapper,
            create_window_allowed: false,
            proxy,

            settings,
            runtime: Some(runtime),
            clipboard: Some(clipboard),
            pending_restart: None,
        }
    }

    fn queue_restart(&mut self, details: RestartDetails) {
        let grid_size = self.window_wrapper.get_grid_size();
        self.pending_restart = Some(RestartRequest { details, grid_size });
        self.pending_draw_commands.clear();
        self.window_wrapper.clear_renderer();
        self.should_render = ShouldRender::Immediately;
    }

    fn exit_application(&mut self, event_loop: &ActiveEventLoop) {
        save_window_size(&self.window_wrapper, &self.settings);
        event_loop.exit();
    }

    fn handle_neovim_exit(&mut self, event_loop: &ActiveEventLoop) {
        match self.pending_restart.take() {
            Some(restart) => self.restart_or_exit(restart, event_loop),
            None => self.exit_application(event_loop),
        }
    }

    fn restart_or_exit(&mut self, restart: RestartRequest, event_loop: &ActiveEventLoop) {
        if self.restart(restart).is_err() {
            self.exit_application(event_loop);
        }
    }

    fn restart(&mut self, restart: RestartRequest) -> Result<(), ()> {
        let Some(runtime) = self.runtime.as_mut() else {
            return Err(());
        };

        runtime
            .restart(
                self.proxy.clone(),
                restart.grid_size,
                self.settings.clone(),
                restart.details,
            )
            .map_err(|error| {
                error!("Failed to restart Neovim: {error:?}");
            })
    }

    fn get_refresh_rate(&self) -> f32 {
        match self.focused {
            // NOTE: Always wait for the idle refresh rate when winit throttling is used to avoid waking up too early
            // The winit redraw request will likely happen much before that and wake it up anyway
            FocusedState::Focused | FocusedState::UnfocusedNotDrawn => {
                self.settings.get::<WindowSettings>().refresh_rate as f32
            }
            _ => self.settings.get::<WindowSettings>().refresh_rate_idle as f32,
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

    fn schedule_next_event(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(feature = "profiling")]
        self.should_render.plot_tracy();
        if self.create_window_allowed {
            self.window_wrapper
                .try_create_window(event_loop, &self.proxy);
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.get_event_deadline()));
    }

    fn animate(&mut self) {
        if self.window_wrapper.skia_renderer.is_none() {
            return;
        }
        let skia_renderer = self.window_wrapper.skia_renderer.as_ref().unwrap();
        let vsync = self.window_wrapper.vsync.as_ref().unwrap();

        let dt =
            Duration::from_secs_f32(vsync.get_refresh_rate(skia_renderer.window(), &self.settings));

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
            if self.window_wrapper.animate_frame(step.as_secs_f32()) {
                self.should_render = ShouldRender::Immediately;
            }
        }
    }

    fn render(&mut self) {
        self.pending_render = false;
        tracy_plot!("pending_render", self.pending_render as u8 as f64);
        self.window_wrapper.draw_frame(self.last_dt);

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

    fn process_buffered_draw_commands(&mut self) {
        if !self.pending_draw_commands.is_empty() {
            self.pending_draw_commands
                .drain(..)
                .for_each(|b| self.window_wrapper.handle_draw_commands(b));
            self.should_render = ShouldRender::Immediately;
        }
    }

    fn reset_animation_period(&mut self) {
        self.should_render = ShouldRender::Wait;
        if self.num_consecutive_rendered == 0 {
            self.animation_start = Instant::now();
            self.animation_time = Duration::ZERO;
        }
    }

    fn schedule_render(&mut self, skipped_frame: bool) {
        if self.window_wrapper.skia_renderer.is_none() {
            return;
        }
        let skia_renderer = self.window_wrapper.skia_renderer.as_ref().unwrap();
        let vsync = self.window_wrapper.vsync.as_mut().unwrap();

        // There's really no point in trying to render if the frame is skipped
        // (most likely due to the compositor being busy). The animated frame will
        // be rendered at an appropriate time anyway.
        if !skipped_frame {
            // When winit throttling is used, request a redraw and wait for the render event
            // Otherwise render immediately
            if vsync.uses_winit_throttling() {
                vsync.request_redraw(skia_renderer.window());
                self.pending_render = true;
                tracy_plot!("pending_render", self.pending_render as u8 as f64);
            } else {
                self.render();
            }
        }
    }

    fn prepare_and_animate(&mut self) {
        // We will also animate, but not render when frames are skipped or a bit late, to reduce visual artifacts
        let skipped_frame =
            self.pending_render && Instant::now() > (self.animation_start + self.animation_time);
        let should_prepare = !self.pending_render || skipped_frame;
        if !should_prepare {
            return;
        }

        let res = self.window_wrapper.prepare_frame();
        self.should_render.update(res);

        let should_animate =
            self.should_render == ShouldRender::Immediately || !self.idle || skipped_frame;

        if should_animate {
            self.reset_animation_period();
            self.animate();
            self.schedule_render(skipped_frame);
        } else {
            // Cache purging should only happen once we become idle; doing it while throttling for
            // vsync caused Skia to evict glyphs mid-animation and re-upload them every frame.
            // See https://github.com/neovide/neovide/pull/3324
            if self.num_consecutive_rendered > 0 {
                self.window_wrapper
                    .renderer
                    .grid_renderer
                    .shaper
                    .cleanup_font_cache();
            }
            self.num_consecutive_rendered = 0;
            tracy_plot!(
                "num_consecutive_rendered",
                self.num_consecutive_rendered as f64
            );
            self.last_dt = self.previous_frame_start.elapsed().as_secs_f32();
            self.previous_frame_start = Instant::now();
        }
    }

    fn redraw_requested(&mut self) {
        if self.pending_render {
            tracy_zone!("render (redraw requested)");
            self.render();
            // We should process all buffered draw commands as soon as the rendering has finished
            self.process_buffered_draw_commands();
        } else {
            tracy_zone!("redraw requested");
            // The OS itself asks us to redraw, so we need to prepare first
            self.should_render = ShouldRender::Immediately;
        }
    }

    fn teardown(&mut self) {
        // Drop the clipboard while the event loop is still alive soWayland handles are released
        // safely. see https://github.com/neovide/neovide/issues/3311
        self.clipboard.take();

        if let Some(mut runtime) = self.runtime.take() {
            // Wait a little bit more and force Neovim to exit after that.
            // This should not be required, but Neovim through libuv spawns child processes that inherit all the handles.
            // This means that the stdio and stderr handles are not properly closed, so the nvim-rs
            // read will hang forever, waiting for more data to read.
            // See https://github.com/neovide/neovide/issues/2182 (which includes links to libuv issues)
            runtime.shutdown_timeout(Duration::from_millis(500));
        }
    }
}

impl Drop for Application {
    fn drop(&mut self) {
        self.teardown();
    }
}

impl ApplicationHandler<UserEvent> for Application {
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        tracy_zone!("window_event");
        match event {
            WindowEvent::RedrawRequested => {
                self.redraw_requested();
            }
            WindowEvent::Focused(focused_event) => {
                self.focused = if focused_event {
                    FocusedState::Focused
                } else {
                    FocusedState::UnfocusedNotDrawn
                };
                #[cfg(target_os = "macos")]
                self.window_wrapper
                    .macos_feature
                    .as_mut()
                    .expect("MacosWindowFeature should already be created here.")
                    .ensure_app_initialized();
            }
            _ => {}
        }

        if self.window_wrapper.handle_window_event(event) {
            self.should_render = ShouldRender::Immediately;
        }
        self.schedule_next_event(event_loop);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        tracy_zone!("user_event");
        match event {
            UserEvent::NeovimExited => {
                self.handle_neovim_exit(event_loop);
            }
            UserEvent::RedrawRequested => {
                self.redraw_requested();
            }
            UserEvent::DrawCommandBatch(batch) if self.pending_render => {
                // Buffer the draw commands if we have a pending render, we have already decided what to
                // draw, so it's not a good idea to process them now.
                // They will be processed immediately after the rendering.
                self.pending_draw_commands.push(batch);
            }
            UserEvent::NeovimRestart(details) => {
                self.queue_restart(details);
            }
            _ => {
                self.window_wrapper.handle_user_event(event);
                self.should_render = ShouldRender::Immediately;
            }
        }
        self.schedule_next_event(event_loop);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        tracy_zone!("about_to_wait");
        self.prepare_and_animate();
        self.schedule_next_event(event_loop);
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        tracy_zone!("resumed");
        self.create_window_allowed = true;
        self.schedule_next_event(event_loop);
    }

    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        tracy_zone!("exiting");
        self.teardown();
        self.window_wrapper.exit();
        self.schedule_next_event(event_loop);
    }
}
