use log::info;
use nvim_rs::Neovim;
use rmpv::Value;

use crate::{
    bridge::{events::*, TxWrapper},
    error_handling::ResultPanicExplanation,
};

pub async fn setup_neovide_specific_state(nvim: &Neovim<TxWrapper>) {
    // Set variable indicating to user config that neovide is being used
    nvim.set_var("neovide", Value::Boolean(true))
        .await
        .unwrap_or_explained_panic("Could not communicate with neovim process");

    if let Err(command_error) = nvim.command("runtime! ginit.vim").await {
        nvim.command(&format!(
            "echomsg \"error encountered in ginit.vim {:?}\"",
            command_error
        ))
        .await
        .ok();
    }

    // Set details about the neovide version
    nvim.set_client_info(
        "neovide",
        vec![
            (Value::from("major"), Value::from(env!("CARGO_PKG_VERSION_MAJOR"))),
            (Value::from("minor"), Value::from(env!("CARGO_PKG_VERSION_MINOR"))),
        ],
        "ui",
        vec![],
        vec![],
    )
    .await
    .ok();

    // Retrieve the channel number for communicating with neovide
    let neovide_channel: u64 = nvim
        .list_chans()
        .await
        .ok()
        .and_then(|channel_values| parse_channel_list(channel_values).ok())
        .and_then(|channel_list| {
            channel_list.iter().find_map(|channel| match channel {
                ChannelInfo {
                    id,
                    client: Some(ClientInfo { name, .. }),
                    ..
                } if name == "neovide" => Some(*id),
                _ => None,
            })
        })
        .unwrap_or(0);

    // Record the channel to the log
    info!(
        "Neovide registered to nvim with channel id {}",
        neovide_channel
    );

    // Create a command for registering right click context hooking
    #[cfg(windows)]
    nvim.command(&build_neovide_command(
        neovide_channel,
        0,
        "NeovideRegisterRightClick",
        "register_right_click",
    ))
    .await
    .ok();

    // Create a command for unregistering the right click context hooking
    #[cfg(windows)]
    nvim.command(&build_neovide_command(
        neovide_channel,
        0,
        "NeovideUnregisterRightClick",
        "unregister_right_click",
    ))
    .await
    .ok();

    // Set some basic rendering options
    nvim.set_option("lazyredraw", Value::Boolean(false))
        .await
        .ok();
    nvim.set_option("termguicolors", Value::Boolean(true))
        .await
        .ok();

    // Create auto command for retrieving exit code from neovim on quit
    nvim.command("autocmd VimLeave * call rpcnotify(1, 'neovide.quit', v:exiting)")
        .await
        .ok();
}

#[cfg(windows)]
pub fn build_neovide_command(channel: u64, num_args: u64, command: &str, event: &str) -> String {
    let nargs: String = if num_args > 1 {
        "+".to_string()
    } else {
        num_args.to_string()
    };
    if num_args == 0 {
        return format!(
            "command! -nargs={} {} call rpcnotify({}, 'neovide.{}')",
            nargs, command, channel, event
        );
    } else {
        return format!(
            "command! -nargs={} -complete=expression {} call rpcnotify({}, 'neovide.{}', <args>)",
            nargs, command, channel, event
        );
    };
}
