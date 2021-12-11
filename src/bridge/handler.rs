use std::sync::Arc;

use async_trait::async_trait;
use log::trace;
use nvim_rs::{Handler, Neovim};
use parking_lot::Mutex;
use rmpv::Value;
use tokio::task;

use super::events::{parse_redraw_event, RedrawEvent};
#[cfg(windows)]
use super::ui_commands::{ParallelCommand, UiCommand};
use crate::bridge::TxWrapper;
use crate::channel_utils::*;
use crate::error_handling::ResultPanicExplanation;
use crate::settings::SETTINGS;

#[derive(Clone)]
pub struct NeovimHandler {
    #[cfg(windows)]
    ui_command_sender: Arc<Mutex<LoggingTx<UiCommand>>>,
    redraw_event_sender: Arc<Mutex<LoggingTx<RedrawEvent>>>,
}

impl NeovimHandler {
    pub fn new(
        #[cfg(windows)] ui_command_sender: LoggingTx<UiCommand>,
        redraw_event_sender: LoggingTx<RedrawEvent>,
    ) -> NeovimHandler {
        NeovimHandler {
            #[cfg(windows)]
            ui_command_sender: Arc::new(Mutex::new(ui_command_sender)),
            redraw_event_sender: Arc::new(Mutex::new(redraw_event_sender)),
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

        #[cfg(windows)]
        let ui_command_sender = self.ui_command_sender.clone();

        let redraw_event_sender = self.redraw_event_sender.clone();
        task::spawn_blocking(move || match event_name.as_ref() {
            "redraw" => {
                for events in arguments {
                    let parsed_events = parse_redraw_event(events)
                        .unwrap_or_explained_panic("Could not parse event from neovim");

                    for parsed_event in parsed_events {
                        let redraw_event_sender = redraw_event_sender.lock();
                        redraw_event_sender.send(parsed_event).ok();
                    }
                }
            }
            "setting_changed" => {
                SETTINGS.handle_changed_notification(arguments);
            }
            #[cfg(windows)]
            "neovide.register_right_click" => {
                let ui_command_sender = ui_command_sender.lock();
                ui_command_sender
                    .send(ParallelCommand::RegisterRightClick.into())
                    .ok();
            }
            #[cfg(windows)]
            "neovide.unregister_right_click" => {
                let ui_command_sender = ui_command_sender.lock();
                ui_command_sender
                    .send(ParallelCommand::UnregisterRightClick.into())
                    .ok();
            }
            _ => {}
        });
    }
}
