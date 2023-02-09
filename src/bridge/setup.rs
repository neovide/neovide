use log::{info, warn};
use nvim_rs::Neovim;
use rmpv::Value;

use crate::{bridge::TxWrapper, error_handling::ResultPanicExplanation};

const REGISTER_CLIPBOARD_PROVIDER_LUA: &str = r"
    local function set_clipboard(register)
        return function(lines, regtype)
            vim.rpcrequest(vim.g.neovide_channel_id, 'neovide.set_clipboard', lines)
        end
    end

    local function get_clipboard(register)
        return function()
            return vim.rpcrequest(vim.g.neovide_channel_id, 'neovide.get_clipboard', register)
        end
    end

    vim.g.clipboard = {
        name = 'neovide',
        copy = {
            ['+'] = set_clipboard('+'),
            ['*'] = set_clipboard('*'),
        },
        paste = {
            ['+'] = get_clipboard('+'),
            ['*'] = get_clipboard('*'),
        },
        cache_enabled = 0
    }";

pub async fn setup_neovide_remote_clipboard(nvim: &Neovim<TxWrapper>, neovide_channel: u64) {
    // Users can opt-out with
    // vim: `let g:neovide_no_custom_clipboard = v:true`
    // lua: `vim.g.neovide_no_custom_clipboard = true`
    let no_custom_clipboard = nvim
        .get_var("neovide_no_custom_clipboard")
        .await
        .ok()
        .and_then(|v| v.as_bool());
    if Some(true) == no_custom_clipboard {
        info!("Neovide working remotely but custom clipboard is disabled");
        return;
    }

    nvim.set_var("neovide_channel_id", Value::from(neovide_channel))
        .await
        .ok();
    nvim.execute_lua(REGISTER_CLIPBOARD_PROVIDER_LUA, vec![])
        .await
        .ok();
}

pub async fn setup_neovide_specific_state(nvim: &Neovim<TxWrapper>, is_remote: bool) {
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

    if let Some(neovide_channel) = neovide_channel {
        // Record the channel to the log.
        info!(
            "Neovide registered to nvim with channel id {}",
            neovide_channel
        );

        // Create a command for registering right click context hooking.
        #[cfg(windows)]
        nvim.command(&build_neovide_command(
            neovide_channel,
            0,
            "NeovideRegisterRightClick",
            "register_right_click",
        ))
        .await
        .ok();

        // Create a command for unregistering the right click context hooking.
        #[cfg(windows)]
        nvim.command(&build_neovide_command(
            neovide_channel,
            0,
            "NeovideUnregisterRightClick",
            "unregister_right_click",
        ))
        .await
        .ok();

        if is_remote {
            setup_neovide_remote_clipboard(nvim, neovide_channel).await;
        }
    } else {
        warn!("Neovide could not find the correct channel id. Some functionality may be disabled.");
    }

    // Set some basic rendering options.
    nvim.set_option("lazyredraw", Value::Boolean(false))
        .await
        .ok();
    nvim.set_option("termguicolors", Value::Boolean(true))
        .await
        .ok();

    // Create auto command for retrieving exit code from neovim on quit.
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
        format!(
            "command! -nargs={} {} call rpcnotify({}, 'neovide.{}')",
            nargs, command, channel, event
        )
    } else {
        format!(
            "command! -nargs={} -complete=expression {} call rpcnotify({}, 'neovide.{}', <args>)",
            nargs, command, channel, event
        )
    }
}
