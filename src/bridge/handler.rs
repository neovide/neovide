use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use log::trace;
#[cfg(target_os = "macos")]
use log::warn;
use nvim_rs::{Handler, Neovim};
use rmpv::Value;
use tokio::sync::mpsc::UnboundedSender;
use winit::event_loop::EventLoopProxy;

#[cfg(target_os = "macos")]
use crate::window::ForceClickKind;
use crate::{
    bridge::{
        clipboard::{get_clipboard_contents, set_clipboard_contents},
        events::parse_redraw_event,
        parse_progress_bar_event, send_ui, NeovimWriter, ParallelCommand, RedrawEvent,
    },
    error_handling::ResultPanicExplanation,
    running_tracker::RunningTracker,
    settings::Settings,
    window::{UserEvent, WindowCommand},
    LoggingSender,
};

#[derive(Clone)]
pub struct NeovimHandler {
    // The EventLoopProxy is not sync on all platforms, so wrap it in a mutex
    proxy: Arc<Mutex<EventLoopProxy<UserEvent>>>,
    sender: LoggingSender<RedrawEvent>,
    running_tracker: RunningTracker,
    #[allow(dead_code)]
    settings: Arc<Settings>,
}

impl NeovimHandler {
    pub fn new(
        sender: UnboundedSender<RedrawEvent>,
        proxy: EventLoopProxy<UserEvent>,
        running_tracker: RunningTracker,
        settings: Arc<Settings>,
    ) -> Self {
        Self {
            proxy: Arc::new(Mutex::new(proxy)),
            sender: LoggingSender::attach(sender, "neovim_handler"),
            running_tracker,
            settings,
        }
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
            "neovide.get_clipboard" => get_clipboard_contents(&arguments[0])
                .map_err(|_| Value::from("cannot get clipboard contents")),
            "neovide.set_clipboard" => set_clipboard_contents(&arguments[0], &arguments[1])
                .map_err(|_| Value::from("cannot set clipboard contents")),
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
                        let _ = self.sender.send(parsed_event);
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
                let _ = self
                    .proxy
                    .lock()
                    .unwrap()
                    .send_event(WindowCommand::RegisterRightClick.into());
            }
            #[cfg(windows)]
            "neovide.unregister_right_click" => {
                let _ = self
                    .proxy
                    .lock()
                    .unwrap()
                    .send_event(WindowCommand::UnregisterRightClick.into());
            }
            "neovide.focus_window" => {
                let _ = self
                    .proxy
                    .lock()
                    .unwrap()
                    .send_event(WindowCommand::FocusWindow.into());
            }
            #[cfg(target_os = "macos")]
            "neovide.force_click" => match parse_force_click_args(&arguments) {
                Some((col, row, entity, guifont, kind)) => {
                    let _ = self.proxy.lock().unwrap().send_event(
                        WindowCommand::TouchpadPressure {
                            col,
                            row,
                            entity,
                            guifont,
                            kind,
                        }
                        .into(),
                    );
                }
                None => warn!("neovide.force_click called with invalid arguments: {arguments:?}"),
            },
            "neovide.exec_detach_handler" => {
                send_ui(ParallelCommand::Quit);
            }
            "neovide.set_redraw" => {
                if let Some(value) = arguments.first() {
                    let value = value.as_bool().unwrap_or(true);
                    let _ = self.sender.send(RedrawEvent::NeovideSetRedraw(value));
                }
            }
            "neovide.intro_banner_allowed" => {
                if let Some(value) = arguments.first() {
                    if let Some(allowed) = value.as_bool() {
                        let _ = self
                            .sender
                            .send(RedrawEvent::NeovideIntroBannerAllowed(allowed));
                    }
                }
            }
            "neovide.progress_bar" => {
                parse_progress_bar_event(arguments.first())
                    .map(|event| {
                        let _ = self.proxy.lock().unwrap().send_event(event);
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
