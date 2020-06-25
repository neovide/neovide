use async_trait::async_trait;
use log::trace;
use nvim_rs::{compat::tokio::Compat, Handler, Neovim};
use rmpv::Value;
use tokio::process::ChildStdin;
use tokio::task;

use super::events::handle_redraw_event_group;
use crate::settings::SETTINGS;

#[cfg(windows)]
use crate::settings::windows_registry::{
    /* is_elevated, */ unregister_rightclick,
    register_rightclick_directory, register_rightclick_file
};

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
            "neovide.reg_right_click" => {
                // let elevated = is_elevated();
                if cfg!(windows) /* && elevated */ {
                    if  !(unregister_rightclick() &&
                          register_rightclick_directory() &&
                          register_rightclick_file()) {
                        // _neovim.err_write("Registering Windows registry can only be done when \"run as administrator\"")
                        //     .await
                        //     .ok();
                    }
                }
            }
            "neovide.unreg_right_click" => {
                // let elevated = is_elevated();
                if cfg!(windows) /* && elevated */ {
                    if !unregister_rightclick() {
                        // _neovim.err_write("Unregistering Windows registry can only be done when \"run as administrator\"")
                        //     .await
                        //     .ok();
                    }
                }
            }
            _ => {}
        })
        .await
        .ok();
    }
}
