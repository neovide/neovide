use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[cfg(target_os = "macos")]
use {
    crate::bridge::{OpenArgs, send_or_queue_file_drop, set_active_route_handler},
    crate::utils::resolve_relative_path,
    std::path::Path,
};

use glamour::Size2;
use rustc_hash::FxHashMap;
use winit::{
    application::ApplicationHandler,
    error::EventLoopError,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    window::WindowId,
};

use super::{
    CmdLineSettings, EventPayload, EventTarget, RouteId, WindowSettings, WindowSize,
    WinitWindowWrapper, error_window, save_window_size,
};
use crate::{
    clipboard::{Clipboard, ClipboardHandle},
    profiling::{tracy_plot, tracy_zone},
    renderer::DrawCommand,
    running_tracker::RunningTracker,
    settings::{AppHotReloadConfigs, HotReloadConfigs, Settings, font::FontSettings},
    units::Grid,
    window::UserEvent,
};

#[derive(Clone, Copy)]
enum FocusedState {
    Focused,
    UnfocusedNotDrawn,
    Unfocused,
}

#[derive(Debug, PartialEq, Clone, Copy)]
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
                    instant.saturating_duration_since(Instant::now()).as_secs_f64()
                );
            }
        }
    }
}

const MAX_ANIMATION_DT: f64 = 1.0 / 120.0;

struct RenderState {
    previous_frame_start: Instant,
    last_dt: f32,
    should_render: ShouldRender,
    num_consecutive_rendered: u32,
    focused: FocusedState,
    pending_render: bool, // we should render as soon as the compositor/vsync allows
    pending_draw_commands: Vec<Vec<DrawCommand>>,
    animation_start: Instant, // when the last animation started (went from idle to animating)
    animation_time: Duration, // how long the current animation has been simulated, will usually be in the future
}

impl RenderState {
    fn new(focused: FocusedState) -> Self {
        let now = Instant::now();
        Self {
            previous_frame_start: now,
            last_dt: 0.0,
            should_render: ShouldRender::Immediately,
            num_consecutive_rendered: 0,
            focused,
            pending_render: false,
            pending_draw_commands: Vec::new(),
            animation_start: now,
            animation_time: Duration::from_millis(0),
        }
    }
}

pub struct Application {
    idle: bool,
    #[allow(dead_code)]
    initial_grid_size: Option<Size2<Grid<u32>>>,
    render_states: FxHashMap<WindowId, RenderState>,
    error_windows: FxHashMap<WindowId, (error_window::State, String)>,

    pub window_wrapper: WinitWindowWrapper,
    create_window_allowed: bool,
    proxy: EventLoopProxy<EventPayload>,
    pub runtime_tracker: RunningTracker,

    settings: Arc<Settings>,
    clipboard: Option<Arc<Mutex<Clipboard>>>,
}

impl Application {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        _initial_window_size: WindowSize,
        initial_grid_size: Option<Size2<Grid<u32>>>,
        initial_font_settings: Option<FontSettings>,
        proxy: EventLoopProxy<EventPayload>,
        settings: Arc<Settings>,
        clipboard: Arc<Mutex<Clipboard>>,
        clipboard_handle: ClipboardHandle,
    ) -> Self {
        let cmd_line_settings = settings.get::<CmdLineSettings>();
        let idle = cmd_line_settings.idle;

        let runtime_tracker = RunningTracker::new();

        let window_wrapper = WinitWindowWrapper::new(
            initial_font_settings,
            settings.clone(),
            runtime_tracker.clone(),
            clipboard_handle,
        );

        Self {
            idle,
            initial_grid_size,
            render_states: FxHashMap::default(),
            error_windows: FxHashMap::default(),

            window_wrapper,
            create_window_allowed: false,
            proxy,
            runtime_tracker,

            settings,
            clipboard: Some(clipboard),
        }
    }

    pub fn run(&mut self, event_loop: EventLoop<EventPayload>) -> Result<(), EventLoopError> {
        event_loop.run_app(self)
    }

    fn focused_state_for_window(&self, window_id: WindowId) -> FocusedState {
        if self
            .window_wrapper
            .routes
            .get(&window_id)
            .map(|route| route.window.winit_window.has_focus())
            .unwrap_or(false)
        {
            FocusedState::Focused
        } else {
            FocusedState::UnfocusedNotDrawn
        }
    }

    fn route_id_for_target(&self, target: EventTarget) -> Option<RouteId> {
        match target {
            EventTarget::Route(route_id) => Some(route_id),
            EventTarget::Window(window_id) => self.window_wrapper.route_id_for_window(window_id),
            _ => None,
        }
    }

    fn sync_render_states(&mut self) {
        let window_ids: Vec<WindowId> = self.window_wrapper.routes.keys().copied().collect();
        self.render_states.retain(|id, _| window_ids.contains(id));
        for window_id in window_ids {
            if self.render_states.contains_key(&window_id) {
                continue;
            }

            let focused = self.focused_state_for_window(window_id);
            self.render_states.insert(window_id, RenderState::new(focused));
        }
    }

    fn ensure_render_state(&mut self, window_id: WindowId) {
        if self.render_states.contains_key(&window_id) {
            return;
        }

        if !self.window_wrapper.routes.contains_key(&window_id) {
            return;
        }

        let focused = self.focused_state_for_window(window_id);
        self.render_states.insert(window_id, RenderState::new(focused));
    }

    fn mark_should_render_for_window(&mut self, window_id: WindowId) {
        self.ensure_render_state(window_id);
        if let Some(state) = self.render_states.get_mut(&window_id) {
            state.should_render = ShouldRender::Immediately;
        }
    }

    fn mark_should_render_all(&mut self) {
        self.sync_render_states();
        let window_ids: Vec<WindowId> = self.render_states.keys().copied().collect();
        for window_id in window_ids {
            if let Some(state) = self.render_states.get_mut(&window_id) {
                state.should_render = ShouldRender::Immediately;
            }
        }
    }

    #[cfg(target_os = "macos")]
    fn activate_focused_route(&self) {
        let Some(window_id) = self.window_wrapper.get_focused_route() else {
            return;
        };

        if let Some(route_id) = self.window_wrapper.route_id_for_window(window_id) {
            set_active_route_handler(route_id);
        }

        self.window_wrapper.activate_and_focus_window(window_id);
    }

    #[cfg(target_os = "macos")]
    fn prepare_open_files(
        &mut self,
        event_loop: &ActiveEventLoop,
        new_window: bool,
        cwd: Option<&Path>,
        args: OpenArgs,
    ) {
        if !new_window {
            self.activate_focused_route();
            self.send_file_drops(args);
            return;
        }

        if self.settings.get::<CmdLineSettings>().server.is_some() {
            self.window_wrapper.try_create_window(event_loop, &self.proxy, cwd, None);
            self.mark_should_render_all();
            self.send_file_drops(args);
            return;
        }

        self.window_wrapper.try_create_window(event_loop, &self.proxy, cwd, Some(args));
        self.mark_should_render_all();
    }

    #[cfg(target_os = "macos")]
    fn send_file_drops(&self, args: OpenArgs) {
        for path in args.files_to_open {
            send_or_queue_file_drop(path, Some(args.tabs));
        }
    }

    fn handle_app_config_changed(&mut self, config: AppHotReloadConfigs) {
        match config {
            AppHotReloadConfigs::Idle(idle) => {
                let mut cmd_line_settings = self.settings.get::<CmdLineSettings>();
                if cmd_line_settings.idle == idle && self.idle == idle {
                    return;
                }

                cmd_line_settings.idle = idle;
                self.settings.set(&cmd_line_settings);
                self.idle = idle;
                self.mark_should_render_all();
            }
        }
    }

    fn handle_config_changed(&mut self, _target: EventTarget, config: HotReloadConfigs) {
        match config {
            HotReloadConfigs::App(config) => {
                self.handle_app_config_changed(config);
            }
            HotReloadConfigs::Window(config) => {
                self.window_wrapper.handle_window_config_changed(config);
                self.mark_should_render_all()
            }
            HotReloadConfigs::Renderer(config) => {
                self.window_wrapper.handle_renderer_config_changed(config);
                self.mark_should_render_all()
            }
        }
    }

    #[cfg(feature = "profiling")]
    fn aggregate_should_render(&self) -> ShouldRender {
        let mut aggregate = ShouldRender::Wait;
        for state in self.render_states.values() {
            aggregate.update(state.should_render);
        }
        aggregate
    }

    fn get_refresh_rate(&self, state: &RenderState) -> f32 {
        match state.focused {
            // NOTE: Always wait for the idle refresh rate when winit throttling is used to avoid waking up too early
            // The winit redraw request will likely happen much before that and wake it up anyway
            FocusedState::Focused | FocusedState::UnfocusedNotDrawn => {
                self.settings.get::<WindowSettings>().refresh_rate as f32
            }
            FocusedState::Unfocused => {
                self.settings.get::<WindowSettings>().refresh_rate_idle as f32
            }
        }
        .max(1.0)
    }

    fn get_frame_deadline(&self, state: &RenderState) -> Instant {
        let refresh_rate = self.get_refresh_rate(state);
        let expected_frame_duration = Duration::from_secs_f32(1.0 / refresh_rate);
        state.previous_frame_start + expected_frame_duration
    }

    fn get_event_deadline_for(&self, state: &RenderState) -> Instant {
        // When there's a pending render we don't need to wait for anything else than the render event
        if state.pending_render {
            return state.animation_start + state.animation_time;
        }

        match state.should_render {
            ShouldRender::Immediately => Instant::now(),
            ShouldRender::Deadline(old_deadline) => {
                old_deadline.min(self.get_frame_deadline(state))
            }
            ShouldRender::Wait => self.get_frame_deadline(state),
        }
    }

    fn get_event_deadline(&self) -> Option<Instant> {
        self.render_states.values().map(|state| self.get_event_deadline_for(state)).min()
    }

    fn next_control_flow(&self, now: Instant) -> ControlFlow {
        next_control_flow_for(self.get_event_deadline(), !self.error_windows.is_empty(), now)
    }

    fn schedule_next_event(&mut self, event_loop: &ActiveEventLoop) {
        self.sync_render_states();
        #[cfg(feature = "profiling")]
        self.aggregate_should_render().plot_tracy();
        if self.create_window_allowed && self.window_wrapper.has_pending_window_creation() {
            self.window_wrapper.try_create_window(event_loop, &self.proxy, None, None);
        }
        event_loop.set_control_flow(self.next_control_flow(Instant::now()));
    }

    fn handle_error_window_event(
        &mut self,
        window_id: WindowId,
        event: WindowEvent,
    ) -> Option<WindowEvent> {
        let Some((mut error_state, message)) = self.error_windows.remove(&window_id) else {
            return Some(event);
        };

        error_state.handle_window_event(event, &message);

        if !error_state.should_close {
            self.error_windows.insert(window_id, (error_state, message));
        }

        None
    }

    fn exit_if_no_windows_remain(&self, event_loop: &ActiveEventLoop) {
        if self.window_wrapper.is_empty() && self.error_windows.is_empty() {
            event_loop.exit();
        }
    }

    fn teardown(&mut self) {
        // Drop the clipboard while the event loop is still alive so Wayland handles are released
        // safely. see https://github.com/neovide/neovide/issues/3311
        self.clipboard.take();

        // Wait a little bit more and force Nevovim to exit after that.
        // This should not be required, but Neovim through libuv spawns childprocesses that inherits all the handles
        // This means that the stdio and stderr handles are not properly closed, so the nvim-rs
        // read will hang forever, waiting for more data to read.
        // See https://github.com/neovide/neovide/issues/2182 (which includes links to libuv issues)
        if let Some(runtime) = self.window_wrapper.runtime.take() {
            runtime.shutdown(std::time::Duration::from_millis(500));
        }
    }

    fn handle_animation_steps(&mut self, window_id: WindowId, dt: Duration) {
        let num_steps = (dt.as_secs_f64() / MAX_ANIMATION_DT).ceil() as u32;
        let step = dt / num_steps;
        for _ in 0..num_steps {
            if self.window_wrapper.animate_frame(window_id, step.as_secs_f32())
                && let Some(state) = self.render_states.get_mut(&window_id)
            {
                state.should_render = ShouldRender::Immediately;
            }
        }
    }

    fn animate(&mut self, window_id: WindowId) {
        if self.window_wrapper.routes.is_empty() {
            return;
        }

        let dt = match self.window_wrapper.refresh_rate_for_window(window_id, &self.settings) {
            Some(rate) => Duration::from_secs_f32(rate),
            None => return,
        };

        let now = Instant::now();
        let (mut animation_start, mut animation_time) = match self.render_states.get(&window_id) {
            Some(state) => (state.animation_start, state.animation_time),
            None => return,
        };
        let target_animation_time = now - animation_start;
        let mut delta = target_animation_time.saturating_sub(animation_time);

        // Don't try to animate way too big deltas
        // Instead reset the animation times, and simulate a single frame
        if delta > Duration::from_millis(1000) {
            animation_start = now;
            animation_time = Duration::ZERO;
            delta = dt;
        }
        // Catchup immediately if the delta is more than one frame, otherwise smooth it over 10 frames
        let catchup = if delta >= dt { delta } else { delta.div_f64(10.0) };

        let dt = dt + catchup;
        tracy_plot!("Simulation dt", dt.as_secs_f64());
        animation_time += dt;

        if let Some(state) = self.render_states.get_mut(&window_id) {
            state.animation_start = animation_start;
            state.animation_time = animation_time;
        }

        self.handle_animation_steps(window_id, dt);
    }

    fn render(&mut self, window_id: WindowId) {
        let (last_dt, was_unfocused_not_drawn) = match self.render_states.get_mut(&window_id) {
            Some(state) => {
                state.pending_render = false;
                tracy_plot!("pending_render", state.pending_render as u8 as f64);
                (state.last_dt, matches!(state.focused, FocusedState::UnfocusedNotDrawn))
            }
            None => return,
        };

        self.window_wrapper.draw_frame(window_id, last_dt);

        if let Some(state) = self.render_states.get_mut(&window_id) {
            if was_unfocused_not_drawn {
                state.focused = FocusedState::Unfocused;
            }

            state.num_consecutive_rendered += 1;
            tracy_plot!("num_consecutive_rendered", state.num_consecutive_rendered as f64);
            state.last_dt = state.previous_frame_start.elapsed().as_secs_f32();
            state.previous_frame_start = Instant::now();
        }
    }

    fn process_buffered_draw_commands(&mut self, window_id: WindowId) {
        let pending_batches = match self.render_states.get_mut(&window_id) {
            Some(state) => state.pending_draw_commands.drain(..).collect::<Vec<_>>(),
            None => return,
        };
        if !pending_batches.is_empty() {
            for batch in pending_batches {
                self.window_wrapper.handle_draw_commands(window_id, batch);
            }
            if let Some(state) = self.render_states.get_mut(&window_id) {
                state.should_render = ShouldRender::Immediately;
            }
        }
    }

    fn reset_animation_period(&mut self, window_id: WindowId) {
        let state = match self.render_states.get_mut(&window_id) {
            Some(state) => state,
            None => return,
        };
        state.should_render = ShouldRender::Wait;
        if state.num_consecutive_rendered == 0 {
            state.animation_start = Instant::now();
            state.animation_time = Duration::ZERO;
        }
    }

    fn schedule_render(&mut self, window_id: WindowId, skipped_frame: bool) {
        // There's really no point in trying to render if the frame is skipped
        // (most likely due to the compositor being busy). The animated frame will
        // be rendered at an appropriate time anyway.
        if skipped_frame || self.window_wrapper.routes.is_empty() {
            return;
        }

        let Some(throttled) = self.window_wrapper.request_redraw_for_window(window_id) else {
            return;
        };

        // When winit throttling is used, request a redraw and wait for the render event
        // Otherwise, render immediately
        if throttled {
            if let Some(state) = self.render_states.get_mut(&window_id) {
                state.pending_render = true;
                tracy_plot!("pending_render", state.pending_render as u8 as f64);
            }
        } else {
            self.render(window_id);
        }
    }

    fn prepare_and_animate(&mut self) {
        self.sync_render_states();
        // We will also animate, but not render when frames are skipped or a bit late, to reduce visual artifacts
        let window_ids: Vec<WindowId> = self.window_wrapper.routes.keys().copied().collect();
        let now = Instant::now();

        for window_id in window_ids {
            let skipped_frame = self
                .render_states
                .get(&window_id)
                .map(|state| {
                    state.pending_render && now > (state.animation_start + state.animation_time)
                })
                .unwrap_or(false);

            let should_prepare = self
                .render_states
                .get(&window_id)
                .map(|state| !state.pending_render || skipped_frame)
                .unwrap_or(true);

            if !should_prepare {
                continue;
            }

            let res = self.window_wrapper.prepare_frame(window_id);
            if let Some(state) = self.render_states.get_mut(&window_id) {
                state.should_render.update(res);
            }

            let should_animate = self
                .render_states
                .get(&window_id)
                .map(|state| {
                    state.should_render == ShouldRender::Immediately || !self.idle || skipped_frame
                })
                .unwrap_or(false);

            if should_animate {
                self.reset_animation_period(window_id);
                self.animate(window_id);
                self.schedule_render(window_id, skipped_frame);
            } else {
                // Cache purging should only happen once we become idle; doing it while throttling
                // for vsync caused Skia to evict glyphs mid-animation and re-upload them every
                // frame. See https://github.com/neovide/neovide/pull/3324
                let should_cleanup_cache = self
                    .render_states
                    .get(&window_id)
                    .map(|state| state.num_consecutive_rendered > 0)
                    .unwrap_or(false);

                if should_cleanup_cache
                    && let Some(route) = self.window_wrapper.routes.get(&window_id)
                {
                    route.window.renderer.borrow_mut().grid_renderer.shaper.cleanup_font_cache();
                }

                if let Some(state) = self.render_states.get_mut(&window_id) {
                    state.num_consecutive_rendered = 0;
                    tracy_plot!("num_consecutive_rendered", state.num_consecutive_rendered as f64);
                    state.last_dt = state.previous_frame_start.elapsed().as_secs_f32();
                    state.previous_frame_start = Instant::now();
                }
            }
        }
    }

    fn redraw_requested(&mut self, window_id: WindowId) {
        self.ensure_render_state(window_id);
        let pending_render =
            self.render_states.get(&window_id).map(|state| state.pending_render).unwrap_or(false);
        if pending_render {
            tracy_zone!("render (redraw requested)");
            self.render(window_id);
            // We should process all buffered draw commands as soon as the rendering has finished
            self.process_buffered_draw_commands(window_id);
        } else {
            tracy_zone!("redraw requested");
            // The OS itself asks us to redraw, so we need to prepare first
            if let Some(state) = self.render_states.get_mut(&window_id) {
                state.should_render = ShouldRender::Immediately;
            }
        }
    }
}

impl ApplicationHandler<EventPayload> for Application {
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        tracy_zone!("about_to_wait");
        self.prepare_and_animate();
        self.schedule_next_event(event_loop);
    }

    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: winit::event::StartCause) {
        match cause {
            winit::event::StartCause::Init => {
                self.schedule_next_event(event_loop);
            }
            winit::event::StartCause::ResumeTimeReached { .. } => {
                self.schedule_next_event(event_loop);
            }
            winit::event::StartCause::WaitCancelled { .. } => {
                self.schedule_next_event(event_loop);
            }
            winit::event::StartCause::Poll => {
                self.schedule_next_event(event_loop);
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: winit::event::WindowEvent,
    ) {
        tracy_zone!("window_event");

        let Some(event) = self.handle_error_window_event(window_id, event) else {
            self.exit_if_no_windows_remain(event_loop);
            return;
        };

        self.ensure_render_state(window_id);
        match event {
            WindowEvent::RedrawRequested => {
                self.redraw_requested(window_id);
            }
            WindowEvent::Focused(focused_event) => {
                if let Some(state) = self.render_states.get_mut(&window_id) {
                    state.focused = if focused_event {
                        FocusedState::Focused
                    } else {
                        FocusedState::UnfocusedNotDrawn
                    };
                }
                #[cfg(target_os = "macos")]
                {
                    if let Some(route) = self.window_wrapper.routes.get(&window_id)
                        && let Some(macos_feature) = route.window.macos_feature.as_ref()
                    {
                        macos_feature.borrow_mut().ensure_app_initialized();
                    }
                }
            }
            _ => {}
        }

        if self.window_wrapper.handle_window_event(window_id, event) {
            self.mark_should_render_for_window(window_id);
        }
        self.schedule_next_event(event_loop);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: EventPayload) {
        tracy_zone!("user_event");
        let EventPayload { payload, target } = event;
        match payload {
            UserEvent::ConfigsChanged(config) => self.handle_config_changed(target, *config),
            #[cfg(target_os = "macos")]
            UserEvent::OpenFiles {
                files,
                cwd,
                caller_cwd,
                tabs,
                new_window,
                neovim_bin,
                neovim_args,
            } => {
                let cwd = cwd.as_deref().map(Path::new);
                let caller_cwd = caller_cwd.as_deref().map(Path::new);
                let open_args = OpenArgs {
                    files_to_open: files
                        .into_iter()
                        .map(|path| resolve_relative_path(&path, caller_cwd))
                        .collect(),
                    tabs,
                    neovim_bin,
                    neovim_args,
                };

                self.prepare_open_files(event_loop, new_window, cwd, open_args);
            }
            UserEvent::NeovimExited => {
                let route_id = self.route_id_for_target(target);
                let Some(route_id) = route_id else {
                    log::warn!("NeovimExited event missing window/route target");
                    return;
                };
                let window_id = self.window_wrapper.window_id_for_route(route_id);
                let remaining_before = self.window_wrapper.routes.len();
                if remaining_before <= 1 && window_id.is_some() {
                    save_window_size(&self.window_wrapper, &self.settings);
                }
                self.window_wrapper.handle_neovim_exit_route(route_id, &self.proxy);
                if let Some(window_id) = window_id {
                    self.render_states.remove(&window_id);
                }
                self.exit_if_no_windows_remain(event_loop);
            }
            UserEvent::RedrawRequested => match target {
                EventTarget::Window(window_id) => {
                    self.redraw_requested(window_id);
                }
                EventTarget::Route(route_id) => {
                    if let Some(window_id) = self.window_wrapper.window_id_for_route(route_id) {
                        self.redraw_requested(window_id);
                    }
                }
                EventTarget::Focused => {
                    if let Some(window_id) = self.window_wrapper.get_focused_route() {
                        self.redraw_requested(window_id);
                    }
                }
                EventTarget::All => {
                    let window_ids: Vec<WindowId> =
                        self.window_wrapper.routes.keys().copied().collect();
                    for window_id in window_ids {
                        self.redraw_requested(window_id);
                    }
                }
            },
            UserEvent::DrawCommandBatch(batch) => {
                match target {
                    EventTarget::Window(window_id) => {
                        self.ensure_render_state(window_id);
                        let pending_render = self
                            .render_states
                            .get(&window_id)
                            .map(|state| state.pending_render)
                            .unwrap_or(false);
                        if pending_render {
                            // Buffer the draw commands if we have a pending render, we have already decided what to
                            // draw, so it's not a good idea to process them now.
                            // They will be processed immediately after the rendering.
                            if let Some(state) = self.render_states.get_mut(&window_id) {
                                state.pending_draw_commands.push(batch);
                            }
                        } else {
                            self.window_wrapper.handle_draw_commands(window_id, batch);
                            self.mark_should_render_for_window(window_id);
                        }
                    }
                    EventTarget::Route(route_id) => {
                        if let Some(window_id) = self.window_wrapper.window_id_for_route(route_id) {
                            self.ensure_render_state(window_id);
                            let pending_render = self
                                .render_states
                                .get(&window_id)
                                .map(|state| state.pending_render)
                                .unwrap_or(false);
                            if pending_render {
                                if let Some(state) = self.render_states.get_mut(&window_id) {
                                    state.pending_draw_commands.push(batch);
                                }
                            } else {
                                self.window_wrapper.handle_draw_commands(window_id, batch);
                                self.mark_should_render_for_window(window_id);
                            }
                        } else {
                            self.window_wrapper.handle_draw_commands_for_route(route_id, batch);
                        }
                    }
                    _ => {
                        log::warn!("DrawCommandBatch event missing window/route target");
                    }
                }
            }
            #[cfg(target_os = "macos")]
            UserEvent::CreateWindow => {
                let (cwd, args) = self
                    .window_wrapper
                    .focused_route_launch_context()
                    .map(|(cwd, args)| (cwd, Some(args)))
                    .unwrap_or((None, None));
                self.window_wrapper.try_create_window(
                    event_loop,
                    &self.proxy,
                    cwd.as_deref(),
                    args,
                );
                self.sync_render_states();
                self.mark_should_render_all();
            }
            #[cfg(target_os = "macos")]
            UserEvent::MacShortcut(command) => {
                self.window_wrapper.handle_mac_shortcut(command);
                self.mark_should_render_all();
            }
            UserEvent::NeovimLaunchError { message } => {
                let window_config = error_window::create_error_window(event_loop, &self.settings);
                let clipboard_handle = ClipboardHandle::new(self.clipboard.as_ref().unwrap());
                let state = error_window::State::new(
                    &message,
                    window_config,
                    self.settings.clone(),
                    clipboard_handle,
                );
                let window_id = state.window_id();
                self.error_windows.insert(window_id, (state, message));
            }
            UserEvent::NeovimRestart(details) => {
                let route_id = self.route_id_for_target(target);
                let Some(route_id) = route_id else {
                    log::warn!("NeovimRestart event missing window/route target");
                    return;
                };
                self.window_wrapper.queue_restart_route(route_id, details);
                if let Some(window_id) = self.window_wrapper.window_id_for_route(route_id)
                    && let Some(state) = self.render_states.get_mut(&window_id)
                {
                    state.pending_draw_commands.clear();
                    state.should_render = ShouldRender::Immediately;
                }
            }
            payload => {
                self.window_wrapper.handle_user_event(EventPayload { payload, target });
                match target {
                    EventTarget::Window(window_id) => self.mark_should_render_for_window(window_id),
                    EventTarget::Route(route_id) => {
                        if let Some(window_id) = self.window_wrapper.window_id_for_route(route_id) {
                            self.mark_should_render_for_window(window_id);
                        }
                    }
                    EventTarget::Focused => {
                        if let Some(window_id) = self.window_wrapper.get_focused_route() {
                            self.mark_should_render_for_window(window_id);
                        }
                    }
                    EventTarget::All => self.mark_should_render_all(),
                }
            }
        }
        self.schedule_next_event(event_loop);
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        tracy_zone!("resumed");
        self.create_window_allowed = true;
        self.window_wrapper.request_window_creation(&self.proxy);
        self.schedule_next_event(event_loop);
    }

    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        tracy_zone!("exiting");
        self.teardown();
        self.error_windows.clear();
        self.window_wrapper.exit();
        self.schedule_next_event(event_loop);
    }
}

impl Drop for Application {
    fn drop(&mut self) {
        self.teardown();
    }
}

fn next_control_flow_for(
    deadline: Option<Instant>,
    has_error_windows: bool,
    now: Instant,
) -> ControlFlow {
    match deadline {
        Some(deadline) => ControlFlow::WaitUntil(deadline),
        None if has_error_windows => ControlFlow::Wait,
        None => ControlFlow::WaitUntil(now),
    }
}
