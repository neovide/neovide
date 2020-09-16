use async_trait::async_trait;
use log::trace;
use nvim_rs::{compat::tokio::Compat, Handler, Neovim};
use rmpv::Value;
use tokio::process::ChildStdin;
use tokio::task;

use super::events::handle_redraw_event_group;
#[cfg(windows)]
use super::ui_commands::UiCommand;
#[cfg(windows)]
use super::BRIDGE;
use crate::settings::SETTINGS;

#[derive(Clone)]
pub struct NeovimHandler();

#[async_trait]
impl Handler for NeovimHandler {
    type Writer = Compat<ChildStdin>;

    async fn handle_notify(
        &self,
        event_name: String,
        arguments: Vec<Value>,
        _neovim: Neovim<Compat<ChildStdin>>,
    ) {
        trace!("Neovim notification: {:?}", &event_name);
        task::spawn_blocking(move || match event_name.as_ref() {
            "redraw" => {
                handle_redraw_event_group(arguments);
            }
            "setting_changed" => {
                SETTINGS.handle_changed_notification(arguments);
            }
            #[cfg(windows)]
            "neovide.register_right_click" => {
                BRIDGE.queue_command(UiCommand::RegisterRightClick);
            }
            #[cfg(windows)]
            "neovide.unregister_right_click" => {
                BRIDGE.queue_command(UiCommand::UnregisterRightClick);
            }
            _ => {}
        })
        .await
        .ok();
    }
}
