use async_trait::async_trait;
use log::trace;
use nvim_rs::{Handler, Neovim};
use rmpv::Value;

#[cfg(windows)]
use crate::bridge::ui_commands::{ParallelCommand, UiCommand};
use crate::{
    bridge::clipboard::{get_clipboard_contents, set_clipboard_contents},
    window::WindowCommand,
};
use crate::{
    bridge::{events::parse_redraw_event, NeovimWriter},
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
            "neovide.focus_window" => {
                EVENT_AGGREGATOR.send(WindowCommand::FocusWindow);
            }
            _ => {}
        }
    }
}
