use rmpv::Value;
use nvim_rs::{Neovim, Handler, compat::tokio::Compat};
use async_trait::async_trait;
use tokio::process::ChildStdin;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use log::trace;

use crate::error_handling::ResultPanicExplanation;
use crate::editor::EDITOR;
use super::events::{RedrawEvent, parse_neovim_event};

#[derive(Clone)]
pub struct NeovimHandler();

#[async_trait]
impl Handler for NeovimHandler {
    type Writer = Compat<ChildStdin>;

    async fn handle_notify(&self, event_name: String, arguments: Vec<Value>, _neovim: Neovim<Compat<ChildStdin>>) {
        trace!("Neovim notification: {:?}", &event_name);
        let parsed_events = parse_neovim_event(&event_name, arguments)
            .unwrap_or_explained_panic("Could not parse event from neovim");
        for event in parsed_events {
            let mut editor = EDITOR.lock();
            editor.handle_redraw_event(event);
        }
    }
}
