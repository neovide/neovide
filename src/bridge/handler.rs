use async_trait::async_trait;
use log::trace;
use nvim_rs::{Handler, Neovim};
use rmpv::Value;

use clipboard::ClipboardProvider;
use clipboard::ClipboardContext;

#[cfg(windows)]
use crate::bridge::ui_commands::{ParallelCommand, UiCommand};
use crate::{
    bridge::{events::parse_redraw_event, TxWrapper},
    editor::EditorCommand,
    error_handling::ResultPanicExplanation,
    event_aggregator::EVENT_AGGREGATOR,
    running_tracker::*,
    settings::SETTINGS,
};

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

    async fn handle_request(
        &self,
        event_name: String,
        arguments: Vec<Value>,
        _neovim: Neovim<TxWrapper>,
    ) -> Result<Value, Value> {
        match event_name.as_ref() {
            "neovide.get_clipboard" => {
                let mut ctx: ClipboardContext = ClipboardProvider::new().unwrap();
                let lines = ctx
                    .get_contents()
                    .unwrap()
                    .replace("\r", "")
                    .split("\n")
                    .map(|line| Value::from(line))
                    .collect::<Vec<Value>>();
                // returns a [[String], RegType]
                Ok(Value::from(vec![
                    Value::from(lines),
                    Value::from("V") // default regtype as Line paste
                ]))
            }
            _ => {
                Ok(Value::from("rpcrequest not handled"))
            }
        }
    }

    async fn handle_notify(
        &self,
        event_name: String,
        arguments: Vec<Value>,
        _neovim: Neovim<TxWrapper>,
    ) {
        trace!("Neovim notification: {:?}", &event_name);

        match event_name.as_ref() {
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
            "neovide.quit" => {
                let error_code = arguments[0]
                    .as_i64()
                    .expect("Could not parse error code from neovim");
                RUNNING_TRACKER.quit_with_code(error_code as i32, "Quit from neovim");
            }
            #[cfg(windows)]
            "neovide.register_right_click" => {
                EVENT_AGGREGATOR.send(UiCommand::Parallel(ParallelCommand::RegisterRightClick));
            }
            #[cfg(windows)]
            "neovide.unregister_right_click" => {
                EVENT_AGGREGATOR.send(UiCommand::Parallel(ParallelCommand::UnregisterRightClick));
            }
            "neovide.set_clipboard" => {
                if (arguments.len() != 3) {
                    return;
                }
                let lines = arguments[0]
                    .as_array()
                    .map(|arr| arr
                        .iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect::<Vec<String>>()
                        .join("\n"))
                    .unwrap();
                let regtype = arguments[1].as_str().unwrap();
                let register = arguments[2].as_str().unwrap();
                if (register == "+") {
                    let mut ctx: ClipboardContext = ClipboardProvider::new().unwrap();
                    ctx.set_contents(lines).unwrap();
                }
            }
            _ => {}
        }
    }
}
