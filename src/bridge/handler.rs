use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use log::trace;
use nvim_rs::{Handler, Neovim};
use rmpv::Value;
use tokio::sync::mpsc::UnboundedSender;
use winit::event_loop::EventLoopProxy;

use crate::{
    bridge::{
        clipboard::{get_clipboard_contents, set_clipboard_contents},
        events::parse_redraw_event,
        send_ui, NeovimWriter, ParallelCommand, RedrawEvent,
    },
    error_handling::ResultPanicExplanation,
    settings::Settings,
    window::{UserEvent, WindowCommand},
    LoggingSender,
};

#[derive(Clone)]
pub struct NeovimHandler {
    // The EventLoopProxy is not sync on all platforms, so wrap it in a mutex
    proxy: Arc<Mutex<EventLoopProxy<UserEvent>>>,
    sender: LoggingSender<RedrawEvent>,
    #[allow(dead_code)]
    settings: Arc<Settings>,
}

impl NeovimHandler {
    pub fn new(
        sender: UnboundedSender<RedrawEvent>,
        proxy: EventLoopProxy<UserEvent>,
        settings: Arc<Settings>,
    ) -> Self {
        Self {
            proxy: Arc::new(Mutex::new(proxy)),
            sender: LoggingSender::attach(sender, "neovim_handler"),
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
                log::info!("Neovim normal quit with code {error_code}");
                let _ = self
                    .proxy
                    .lock()
                    .unwrap()
                    .send_event(UserEvent::SetExitCode(error_code as u8));
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
            "neovide.exec_detach_handler" => {
                send_ui(ParallelCommand::Quit);
            }
            "neovide.set_redraw" => {
                if let Some(value) = arguments.first() {
                    let value = value.as_bool().unwrap_or(true);
                    let _ = self.sender.send(RedrawEvent::NeovideSetRedraw(value));
                }
            }
            _ => {}
        }
    }
}
