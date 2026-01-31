use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, RwLock,
};

use async_trait::async_trait;
use log::trace;
#[cfg(target_os = "macos")]
use log::warn;
use nvim_rs::{Handler, Neovim};
use rmpv::Value;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use winit::{event_loop::EventLoopProxy, window::WindowId};

#[cfg(target_os = "macos")]
use crate::window::ForceClickKind;
use crate::{
    bridge::{
        clipboard::{get_clipboard_contents, set_clipboard_contents},
        events::parse_redraw_event,
        parse_progress_bar_event, send_ui, NeovimWriter, ParallelCommand, RedrawEvent,
        HANDLER_REGISTRY,
    },
    clipboard::ClipboardHandle,
    error_handling::ResultPanicExplanation,
    running_tracker::RunningTracker,
    settings::Settings,
    window::{EventPayload, UserEvent, WindowCommand},
    LoggingReceiver, LoggingSender,
};

use super::ui_commands::UiCommand;

#[derive(Clone)]
pub struct NeovimHandler {
    // The EventLoopProxy is not sync on all platforms, so wrap it in a mutex
    proxy: Arc<Mutex<EventLoopProxy<EventPayload>>>,
    redraw_event_sender: LoggingSender<RedrawEvent>,
    ui_command_sender: LoggingSender<UiCommand>,
    ui_command_receiver: LoggingReceiver<UiCommand>,
    current_neovim: Arc<RwLock<Option<Neovim<NeovimWriter>>>>,
    ui_command_started: Arc<AtomicBool>,
    running_tracker: RunningTracker,
    window_id: WindowId,
    #[allow(dead_code)]
    settings: Arc<Settings>,
    clipboard: ClipboardHandle,
}

impl std::fmt::Debug for NeovimHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NeovimHandler").finish()
    }
}

impl NeovimHandler {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        redraw_event_sender: UnboundedSender<RedrawEvent>,
        ui_command_sender: UnboundedSender<UiCommand>,
        ui_command_receiver: UnboundedReceiver<UiCommand>,
        proxy: EventLoopProxy<EventPayload>,
        running_tracker: RunningTracker,
        window_id: WindowId,
        settings: Arc<Settings>,
        clipboard: ClipboardHandle,
    ) -> Self {
        Self {
            proxy: Arc::new(Mutex::new(proxy)),
            redraw_event_sender: LoggingSender::attach(redraw_event_sender, "neovim_handler"),
            ui_command_sender: LoggingSender::attach(ui_command_sender, "UICommand"),
            ui_command_receiver: LoggingReceiver::attach(ui_command_receiver, "UICommand"),
            current_neovim: Arc::new(RwLock::new(None)),
            ui_command_started: Arc::new(AtomicBool::new(false)),
            running_tracker,
            window_id,
            settings,
            clipboard,
        }
    }

    fn send_window_command(&self, command: WindowCommand) {
        let payload = EventPayload::new(UserEvent::WindowCommand(command), self.window_id);
        let _ = self.proxy.lock().unwrap().send_event(payload);
    }

    pub fn get_ui_command_channel(&self) -> (LoggingSender<UiCommand>, LoggingReceiver<UiCommand>) {
        (
            self.ui_command_sender.clone(),
            self.ui_command_receiver.clone(),
        )
    }

    pub(crate) fn update_current_neovim(&self, neovim: Neovim<NeovimWriter>) {
        if let Ok(mut guard) = self.current_neovim.write() {
            *guard = Some(neovim);
        }
    }

    pub(crate) fn clone_current_neovim(&self) -> Option<Neovim<NeovimWriter>> {
        self.current_neovim
            .read()
            .ok()
            .and_then(|guard| guard.as_ref().cloned())
    }

    pub(crate) fn mark_ui_command_started(&self) -> bool {
        self.ui_command_started.swap(true, Ordering::SeqCst)
    }
}

#[async_trait]
impl Handler for NeovimHandler {
    type Writer = NeovimWriter;

    async fn handle_request(
        &self,
        event_name: String,
        arguments: Vec<Value>,
        _neovim: Neovim<Self::Writer>,
    ) -> Result<Value, Value> {
        trace!("Neovim request: {:?}", &event_name);

        match event_name.as_ref() {
            "neovide.get_clipboard" => self
                .clipboard
                .upgrade()
                .ok_or(Value::from("clipboard unavailable"))
                .and_then(|clipboard| {
                    let mut clipboard = clipboard.lock().unwrap();
                    get_clipboard_contents(&mut clipboard, &arguments[0])
                        .map_err(|_| Value::from("cannot get clipboard contents"))
                }),
            "neovide.set_clipboard" => self
                .clipboard
                .upgrade()
                .ok_or(Value::from("clipboard unavailable"))
                .and_then(|clipboard| {
                    let mut clipboard = clipboard.lock().unwrap();
                    set_clipboard_contents(&mut clipboard, &arguments[0], &arguments[1])
                        .map_err(|_| Value::from("cannot set clipboard contents"))
                }),
            "neovide.quit" => {
                let error_code = arguments[0]
                    .as_i64()
                    .expect("Could not parse error code from neovim");
                self.running_tracker
                    .quit_with_code(error_code as u8, "Quit from neovim");
                Ok(Value::Nil)
            }
            _ => Ok(Value::from("rpcrequest not handled")),
        }
    }

    async fn handle_notify(
        &self,
        event_name: String,
        arguments: Vec<Value>,
        _neovim: Neovim<Self::Writer>,
    ) {
        trace!("Neovim notification: {:?}", &event_name);

        match event_name.as_ref() {
            "redraw" => {
                for events in arguments {
                    let parsed_events = parse_redraw_event(events)
                        .unwrap_or_explained_panic("Could not parse event from neovim");

                    for parsed_event in parsed_events {
                        match parsed_event {
                            RedrawEvent::Restart { details } => {
                                let payload = EventPayload::new(
                                    UserEvent::NeovimRestart(details),
                                    self.window_id,
                                );
                                let _ = self.proxy.lock().unwrap().send_event(payload);
                            }
                            _ => {
                                let _ = self.redraw_event_sender.send(parsed_event);
                            }
                        }
                    }
                }
            }
            "setting_changed" => {
                self.settings
                    .handle_setting_changed_notification(arguments, &self.proxy.lock().unwrap());
            }
            "option_changed" => {
                self.settings
                    .handle_option_changed_notification(arguments, &self.proxy.lock().unwrap());
            }
            #[cfg(windows)]
            "neovide.register_right_click" => {
                self.send_window_command(WindowCommand::RegisterRightClick);
            }
            #[cfg(windows)]
            "neovide.unregister_right_click" => {
                self.send_window_command(WindowCommand::UnregisterRightClick);
            }
            "neovide.focus_window" => {
                self.send_window_command(WindowCommand::FocusWindow);
            }
            #[cfg(target_os = "macos")]
            "neovide.force_click" => match parse_force_click_args(&arguments) {
                Some((col, row, entity, guifont, kind)) => {
                    self.send_window_command(WindowCommand::TouchpadPressure {
                        col,
                        row,
                        entity,
                        guifont,
                        kind,
                    });
                }
                None => warn!("neovide.force_click called with invalid arguments: {arguments:?}"),
            },
            "neovide.exec_detach_handler" => {
                let handler = {
                    let handler_lock = HANDLER_REGISTRY.lock().unwrap();
                    handler_lock
                        .clone()
                        .expect("NeovimHandler has not been initialized")
                };
                send_ui(ParallelCommand::Quit, &handler);
            }
            "neovide.set_redraw" => {
                if let Some(value) = arguments.first() {
                    let value = value.as_bool().unwrap_or(true);
                    let _ = self
                        .redraw_event_sender
                        .send(RedrawEvent::NeovideSetRedraw(value));
                }
            }
            "neovide.intro_banner_allowed" => {
                if let Some(value) = arguments.first() {
                    if let Some(allowed) = value.as_bool() {
                        let _ = self
                            .redraw_event_sender
                            .send(RedrawEvent::NeovideIntroBannerAllowed(allowed));
                    }
                }
            }
            "neovide.progress_bar" => {
                parse_progress_bar_event(arguments.first())
                    .map(|event| {
                        let _ = self
                            .proxy
                            .lock()
                            .unwrap()
                            .send_event(EventPayload::new(event, WindowId::from(0)));
                    })
                    .unwrap_or_else(|| {
                        log::info!(
                            "Failed to parse neovide.progress_bar event data: {:?}",
                            arguments
                        );
                    });
            }
            _ => {}
        }
    }
}

#[cfg(target_os = "macos")]
fn parse_force_click_args(
    arguments: &[Value],
) -> Option<(i64, i64, String, String, ForceClickKind)> {
    let (col, row, entity, guifont, kind_value) = match arguments {
        [col, row, entity, guifont, kind, ..] => (col, row, entity, guifont, Some(kind)),
        [col, row, entity, guifont] => (col, row, entity, guifont, None),
        _ => return None,
    };

    let col = col.as_i64()?;
    let row = row.as_i64()?;
    let entity = entity.as_str().unwrap_or("").to_string();
    let guifont = guifont.as_str().unwrap_or("").to_string();
    let kind_str = kind_value.and_then(Value::as_str).unwrap_or("text");
    let kind = ForceClickKind::from(kind_str);

    Some((col, row, entity, guifont, kind))
}
