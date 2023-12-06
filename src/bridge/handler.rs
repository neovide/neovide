use async_trait::async_trait;
use log::trace;
use nvim_rs::{Handler, Neovim};
use rmpv::Value;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::sync::mpsc::UnboundedSender;
use winit::event_loop::EventLoopProxy;

use crate::{
    bridge::clipboard::{get_clipboard_contents, set_clipboard_contents},
    bridge::{events::parse_redraw_event, NeovimWriter, RedrawEvent},
    error_handling::ResultPanicExplanation,
    running_tracker::*,
    settings::SETTINGS,
    window::{UserEvent, WindowCommand},
    LoggingSender,
};

#[derive(Clone)]
pub struct NeovimHandler {
    // The EventLoopProxy is not sync on all platforms, so wrap it in a mutex
    proxy: Arc<Mutex<EventLoopProxy<UserEvent>>>,
    sender: LoggingSender<RedrawEvent>,
}

impl NeovimHandler {
    pub fn new(sender: UnboundedSender<RedrawEvent>, proxy: EventLoopProxy<UserEvent>) -> Self {
        Self {
            proxy: Arc::new(Mutex::new(proxy)),
            sender: LoggingSender::attach(sender, "neovim_handler"),
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
        neovim: Neovim<Self::Writer>,
    ) -> Result<Value, Value> {
        trace!("Neovim request: {:?}", &event_name);

        match event_name.as_ref() {
            "neovide.get_clipboard" => {
                let endline_type = neovim
                    .command_output("set ff")
                    .await
                    .ok()
                    .and_then(|format| {
                        let mut s = format.split('=');
                        s.next();
                        s.next().map(String::from)
                    });

                get_clipboard_contents(endline_type.as_deref())
                    .map_err(|_| Value::from("cannot get clipboard contents"))
            }
            "neovide.set_clipboard" => set_clipboard_contents(&arguments[0])
                .map_err(|_| Value::from("cannot set clipboard contents")),
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
                SETTINGS
                    .handle_setting_changed_notification(arguments, &self.proxy.lock().unwrap());
            }
            "option_changed" => {
                SETTINGS.handle_option_changed_notification(arguments, &self.proxy.lock().unwrap());
            }
            "neovide.quit" => {
                let error_code = arguments[0]
                    .as_i64()
                    .expect("Could not parse error code from neovim");
                RUNNING_TRACKER.quit_with_code(error_code as i32, "Quit from neovim");
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
            _ => {}
        }
    }
}
