use std::sync::{Arc, Mutex};

use rmpv::Value;
use nvim_rs::{Neovim, Handler, compat::tokio::Compat};
use async_trait::async_trait;
use tokio::process::ChildStdin;

use crate::error_handling::ResultPanicExplanation;
use crate::editor::Editor;
use super::events::parse_neovim_event;

#[derive(Clone)]
pub struct NeovimHandler(pub Arc<Mutex<Editor>>);

#[async_trait]
impl Handler for NeovimHandler {
    type Writer = Compat<ChildStdin>;

    async fn handle_notify(&self, event_name: String, arguments: Vec<Value>, _neovim: Neovim<Compat<ChildStdin>>) {
        let parsed_events = parse_neovim_event(event_name, arguments)
            .unwrap_or_explained_panic("Could not parse event", "Could not parse event from neovim");
        for event in parsed_events {
            let mut editor = self.0.lock().unwrap();
            editor.handle_redraw_event(event);
        }
    }
}
