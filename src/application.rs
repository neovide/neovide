use std::{
    io::{stdout, IsTerminal},
    sync::{Arc, Mutex},
};

use winit::{
    application::ApplicationHandler,
    event::StartCause,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
};

use crate::{
    bridge::NeovimRuntime,
    clipboard::Clipboard,
    error_handling::format_and_log_error_message,
    profiling::tracy_zone,
    settings::{Config, Settings},
    window::{ErrorWindow, NeovimWindow, UpdateLoop, UserEvent},
};

pub struct NeovideApplication {
    pub exit_code: u8,
    initial_config: Option<Config>,
    current_window: Option<UpdateLoop>,
    proxy: EventLoopProxy<UserEvent>,
    settings: Arc<Settings>,
    runtime: Option<NeovimRuntime>,
    clipboard: Option<Arc<Mutex<Clipboard>>>,
}

impl NeovideApplication {
    pub fn new(
        initial_config: Config,
        event_loop: &EventLoop<UserEvent>,
        settings: Arc<Settings>,
    ) -> Self {
        let clipboard = Arc::new(Mutex::new(Clipboard::new(event_loop)));
        let runtime =
            Some(NeovimRuntime::new(clipboard.clone()).expect("Failed to create runtime"));
        let proxy = event_loop.create_proxy();
        Self {
            exit_code: 0,
            initial_config: Some(initial_config),
            current_window: None,
            proxy,
            settings,
            runtime,
            clipboard: Some(clipboard),
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
        self.exit_code = 1;
        if stdout().is_terminal() {
            // The logger already writes to stderr
            log::error!("{}", &format_and_log_error_message(err));
            event_loop.exit();
        } else {
            let window = ErrorWindow::new(
                format_and_log_error_message(err),
                event_loop,
                self.settings.clone(),
                self.proxy.clone(),
                self.clipboard.as_ref().unwrap().clone(),
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
            UserEvent::SetExitCode(code) => {
                self.exit_code = code;
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
        // SAFETY: It's important that all rendering resources are cleaned up here before the EventLoop is destroyed
        self.current_window = None;

        // SAFETY: It's important that the runtime is cleaned up here, since it might contain references to the clipboard
        // And that indirectly uses the event loop, so it has to be destroyed before that.
        self.runtime = None;
        // SAFETY: It's important that the clipboard, which uses the event loop is deleted here
        self.clipboard = None;
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
