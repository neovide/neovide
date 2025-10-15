use std::{
    io::{stdout, IsTerminal},
    sync::Arc,
    time::Duration,
};

use winit::{
    application::ApplicationHandler,
    event::StartCause,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy},
};

use crate::{
    bridge::NeovimRuntime,
    error_handling::format_and_log_error_message,
    profiling::tracy_zone,
    running_tracker::RunningTracker,
    settings::{Config, Settings},
    window::{ErrorWindow, NeovimWindow, UpdateLoop, UserEvent},
};

pub struct NeovideApplication {
    initial_config: Option<Config>,
    current_window: Option<UpdateLoop>,
    proxy: EventLoopProxy<UserEvent>,
    running_tracker: RunningTracker,
    settings: Arc<Settings>,
    runtime: Option<NeovimRuntime>,
}

impl NeovideApplication {
    pub fn new(
        initial_config: Config,
        proxy: EventLoopProxy<UserEvent>,
        settings: Arc<Settings>,
        running_tracker: RunningTracker,
    ) -> Self {
        let runtime = Some(NeovimRuntime::new().expect("Failed to create runtime"));
        Self {
            initial_config: Some(initial_config),
            current_window: None,
            proxy,
            running_tracker,
            settings,
            runtime,
        }
    }

    fn schedule_next_event(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.current_window {
            event_loop.set_control_flow(ControlFlow::WaitUntil(window.get_event_deadline()));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }

    fn handle_startup_errors(&mut self, err: anyhow::Error, event_loop: &ActiveEventLoop) {
        if stdout().is_terminal() {
            // The logger already writes to stderr
            log::error!("{}", &format_and_log_error_message(err));
            event_loop.exit();
            self.running_tracker.quit_with_code(1, "Startup Error");
        } else {
            let window = ErrorWindow::new(
                format_and_log_error_message(err),
                event_loop,
                self.settings.clone(),
            );
            self.current_window = Some(UpdateLoop::new(
                Box::new(window),
                self.proxy.clone(),
                self.settings.clone(),
            ));
        };
    }
}

impl ApplicationHandler<UserEvent> for NeovideApplication {
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        tracy_zone!("window_event");
        if let Some(window) = &mut self.current_window {
            window.window_event(event_loop, window_id, event);
        }
        self.schedule_next_event(event_loop);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        tracy_zone!("user_event");
        match event {
            UserEvent::LaunchFailure(err) => {
                self.handle_startup_errors(err, event_loop);
            }
            _ => {
                if let Some(window) = &mut self.current_window {
                    window.user_event(event_loop, event);
                }
            }
        }
        self.schedule_next_event(event_loop);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        tracy_zone!("about_to_wait");
        if let Some(window) = &mut self.current_window {
            window.about_to_wait(event_loop);
        }
        self.schedule_next_event(event_loop);
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        tracy_zone!("resumed");
        if let Some(window) = &mut self.current_window {
            window.resumed(event_loop);
        }
        self.schedule_next_event(event_loop);
    }

    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        tracy_zone!("exiting");
        self.current_window = None;
        // Wait a little bit more and force Nevoim to exit after that.
        // This should not be required, but Neovim through libuv spawns childprocesses that inherits all the handles
        // This means that the stdio and stderr handles are not properly closed, so the nvim-rs
        // read will hang forever, waiting for more data to read.
        // See https://github.com/neovide/neovide/issues/2182 (which includes links to libuv issues)
        if let Some(runtime) = self.runtime.take() {
            runtime.runtime.shutdown_timeout(Duration::from_millis(500));
        }
        self.schedule_next_event(event_loop);
    }

    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        if cause == StartCause::Init {
            tracy_zone!("init");
            let config = self.initial_config.take().unwrap();
            let window = NeovimWindow::new(
                config,
                self.settings.clone(),
                self.proxy.clone(),
                self.running_tracker.clone(),
                self.runtime.as_mut().unwrap(),
            );
            self.current_window = Some(UpdateLoop::new(
                Box::new(window),
                self.proxy.clone(),
                self.settings.clone(),
            ));
            self.schedule_next_event(event_loop);
        }
    }
}
