use std::sync::Arc;

use async_trait::async_trait;
use crossfire::mpsc::TxUnbounded;
use log::trace;
use nvim_rs::{compat::tokio::Compat, Handler, Neovim};
use parking_lot::Mutex;
use rmpv::Value;
use tokio::task;

use super::events::{parse_redraw_event, RedrawEvent};
use super::ui_commands::UiCommand;
use crate::bridge::TxWrapper;
use crate::error_handling::ResultPanicExplanation;
use crate::settings::SETTINGS;

#[derive(Clone)]
pub struct NeovimHandler {
    ui_command_sender: Arc<Mutex<TxUnbounded<UiCommand>>>,
    redraw_event_sender: Arc<Mutex<TxUnbounded<RedrawEvent>>>,
}

impl NeovimHandler {
    pub fn new(
        ui_command_sender: TxUnbounded<UiCommand>,
        redraw_event_sender: TxUnbounded<RedrawEvent>,
    ) -> NeovimHandler {
        NeovimHandler {
            ui_command_sender: Arc::new(Mutex::new(ui_command_sender)),
            redraw_event_sender: Arc::new(Mutex::new(redraw_event_sender)),
        }
    }
}

#[async_trait]
impl Handler for NeovimHandler {
    type Writer = Compat<TxWrapper>;

    async fn handle_notify(
        &self,
        event_name: String,
        arguments: Vec<Value>,
        _neovim: Neovim<Compat<TxWrapper>>,
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
                ui_command_sender.send(UiCommand::RegisterRightClick).ok();
            }
            #[cfg(windows)]
            "neovide.unregister_right_click" => {
                let ui_command_sender = ui_command_sender.lock();
                ui_command_sender.send(UiCommand::UnregisterRightClick).ok();
            }
            _ => {}
        })
        .await
        .ok();
    }
}
