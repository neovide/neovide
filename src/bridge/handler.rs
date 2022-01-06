use async_trait::async_trait;
use futures::lock::Mutex;
use log::trace;
use nvim_rs::{Handler, Neovim};
use rmpv::Value;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;

#[cfg(windows)]
use crate::bridge::ui_commands::{ParallelCommand, UiCommand};
use crate::{
    bridge::{events::parse_redraw_event, TxWrapper},
    editor::EditorCommand,
    error_handling::ResultPanicExplanation,
    event_aggregator::EVENT_AGGREGATOR,
    settings::SETTINGS,
};

struct NotificationEvent {
    event_name: String,
    arguments: Vec<Value>,
}

#[derive(Clone)]
pub struct NeovimHandler {
    sender: Arc<Mutex<Sender<NotificationEvent>>>,
}

impl NeovimHandler {
    pub fn new() -> Self {
        let (sender, receiver): (Sender<NotificationEvent>, Receiver<NotificationEvent>) =
            channel();
        thread::spawn(|| {
            for event in receiver {
                match event.event_name.as_ref() {
                    "redraw" => {
                        for events in event.arguments {
                            let parsed_events = parse_redraw_event(events)
                                .unwrap_or_explained_panic("Could not parse event from neovim");

                            for parsed_event in parsed_events {
                                EVENT_AGGREGATOR
                                    .send(EditorCommand::NeovimRedrawEvent(parsed_event));
                            }
                        }
                    }
                    "setting_changed" => {
                        SETTINGS.handle_changed_notification(event.arguments);
                    }
                    #[cfg(windows)]
                    "neovide.register_right_click" => {
                        EVENT_AGGREGATOR
                            .send(UiCommand::Parallel(ParallelCommand::RegisterRightClick));
                    }
                    #[cfg(windows)]
                    "neovide.unregister_right_click" => {
                        EVENT_AGGREGATOR
                            .send(UiCommand::Parallel(ParallelCommand::UnregisterRightClick));
                    }
                    _ => {}
                }
            }
        });
        Self {
            sender: Arc::new(Mutex::new(sender)),
        }
    }
}

#[async_trait]
impl Handler for NeovimHandler {
    type Writer = TxWrapper;

    async fn handle_notify(
        &self,
        event_name: String,
        arguments: Vec<Value>,
        _neovim: Neovim<TxWrapper>,
    ) {
        trace!("Neovim notification: {:?}", &event_name);
        let sender = self.sender.lock().await;
        sender
            .send(NotificationEvent {
                event_name,
                arguments,
            })
            .ok();
    }
}
