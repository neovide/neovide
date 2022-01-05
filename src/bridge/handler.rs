use async_trait::async_trait;
use log::trace;
use nvim_rs::{Handler, Neovim};
use rmpv::Value;
use tokio::task;

use super::events::parse_redraw_event;
#[cfg(windows)]
use super::ui_commands::{ParallelCommand, UiCommand};
use crate::bridge::TxWrapper;
use crate::editor::EditorCommand;
use crate::error_handling::ResultPanicExplanation;
use crate::event_aggregator::EVENT_AGGREGATOR;
use crate::settings::SETTINGS;

#[derive(Clone)]
pub struct NeovimHandler {}

impl NeovimHandler {
    pub fn new() -> Self {
        Self {}
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

        task::spawn_blocking(move || match event_name.as_ref() {
            "redraw" => {
                for events in arguments {
                    let parsed_events = parse_redraw_event(events)
                        .unwrap_or_explained_panic("Could not parse event from neovim");

                    for parsed_event in parsed_events {
                        EVENT_AGGREGATOR.send(EditorCommand::NeovimRedrawEvent(parsed_event));
                    }
                }
            }
            "setting_changed" => {
                SETTINGS.handle_changed_notification(arguments);
            }
            #[cfg(windows)]
            "neovide.register_right_click" => {
                EVENT_AGGREGATOR.send(UiCommand::Parallel(ParallelCommand::RegisterRightClick));
            }
            #[cfg(windows)]
            "neovide.unregister_right_click" => {
                EVENT_AGGREGATOR.send(UiCommand::Parallel(ParallelCommand::UnregisterRightClick));
            }
            _ => {}
        });
    }
}
