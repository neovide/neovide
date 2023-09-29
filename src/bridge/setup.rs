use log::{info, warn};
use nvim_rs::Neovim;
use rmpv::Value;

use super::setup_intro_message_autocommand;
use crate::{bridge::NeovimWriter, error_handling::ResultPanicExplanation};

const INIT_LUA: &str = include_str!("../../lua/init.lua");

pub async fn setup_neovide_specific_state(
    nvim: &Neovim<NeovimWriter>,
    should_handle_clipboard: bool,
) {
    // Set variable indicating to user config that neovide is being used.
    nvim.set_var("neovide", Value::Boolean(true))
        .await
        .unwrap_or_explained_panic("Could not communicate with neovim process");

    if let Err(command_error) = nvim.command("runtime! ginit.vim").await {
        nvim.command(&format!(
            "echomsg \"error encountered in ginit.vim {command_error:?}\""
        ))
        .await
        .ok();
    }

    // Set details about the neovide version.
    nvim.set_client_info(
        "neovide",
        vec![
            (
                Value::from("major"),
                Value::from(env!("CARGO_PKG_VERSION_MAJOR")),
            ),
            (
                Value::from("minor"),
                Value::from(env!("CARGO_PKG_VERSION_MINOR")),
            ),
        ],
        "ui",
        vec![],
        vec![],
    )
    .await
    .ok();

    // Retrieve the channel number for communicating with neovide.
    let neovide_channel: Option<u64> = nvim
        .get_api_info()
        .await
        .ok()
        .and_then(|info| info[0].as_u64());

    let neovide_channel = if let Some(neovide_channel) = neovide_channel {
        // Record the channel to the log.
        info!(
            "Neovide registered to nvim with channel id {}",
            neovide_channel
        );

        Value::from(neovide_channel)
    } else {
        warn!("Neovide could not find the correct channel id. Some functionality may be disabled.");
        Value::Nil
    };

    let register_clipboard = should_handle_clipboard;
    let register_right_click = cfg!(target_os = "windows");

    let args = Value::from(vec![
        (Value::from("neovide_channel_id"), neovide_channel),
        (
            Value::from("register_clipboard"),
            Value::from(register_clipboard),
        ),
        (
            Value::from("register_right_click"),
            Value::from(register_right_click),
        ),
    ]);

    nvim.execute_lua(INIT_LUA, vec![args])
        .await
        .unwrap_or_explained_panic("Error when running Neovide init.lua");

    setup_intro_message_autocommand(nvim)
        .await
        .unwrap_or_explained_panic("Error setting up intro message");
}
