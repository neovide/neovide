use rmpv::Value;
use nvim_rs::{Neovim, Handler, compat::tokio::Compat};
use async_trait::async_trait;
use tokio::process::ChildStdin;
use tokio::task;
use log::trace;

use crate::settings::SETTINGS;
use super::events::handle_redraw_event_group;

#[derive(Clone)]
pub struct NeovimHandler();

#[async_trait]
impl Handler for NeovimHandler {
    type Writer = Compat<ChildStdin>;

    async fn handle_notify(&self, event_name: String, arguments: Vec<Value>, _neovim: Neovim<Compat<ChildStdin>>) {
        trace!("Neovim notification: {:?}", &event_name);
        task::spawn_blocking(move || {
            match event_name.as_ref() {
                "redraw" => {
                    handle_redraw_event_group(arguments);
                },
                "setting_changed" => {
                    SETTINGS.handle_changed_notification(arguments);
                },
                _ => {}
            }
        }).await.ok();
    }
}
