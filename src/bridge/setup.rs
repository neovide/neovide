use log::{info, warn};
use nvim_rs::Neovim;
use rmpv::Value;

use super::setup_intro_message_autocommand;
use crate::{
    bridge::NeovimWriter,
    error_handling::ResultPanicExplanation,
    settings::{DEFAULT_WINDOW_GEOMETRY, SETTINGS},
    CmdLineSettings,
};

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

const SETUP_GEOMETRY_LUA: &str = r"
    local initial_columns, initial_lines, force = ...
    -- We want to set the geometry in VimEnter, since after that the UI takes control over the size
    -- So UIEnter is too late
    vim.api.nvim_create_autocmd({ 'VimEnter' }, {
        pattern = '*',
        once = true,
        nested = true,
        callback = function()
            if force then
                vim.o.columns = initial_columns
                vim.o.lines = initial_lines
            else
                -- Just set the values again to trigger the OptionSet callback
                -- It's not automatically run when running init.vim/lua
                if vim.o.columns ~= initial_columns then
                    vim.o.columns = vim.o.columns
                end
                if vim.o.lines ~= initial_lines then
                    vim.o.lines = vim.o.lines
                end
            end
            vim.rpcnotify(vim.g.neovide_channel_id, 'neovide.ui_ready')
        end
    })";

pub async fn setup_neovide_remote_clipboard(nvim: &Neovim<NeovimWriter>, neovide_channel: u64) {
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

        if should_handle_clipboard {
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
    nvim.command(
        "autocmd VimLeave * call rpcnotify(g:neovide_channel_id, 'neovide.quit', v:exiting)",
    )
    .await
    .ok();

    nvim.command(
        "autocmd OptionSet columns call rpcnotify(g:neovide_channel_id, 'neovide.columns', str2nr(v:option_new))",
    )
    .await
    .ok();
    nvim.command(
        "autocmd OptionSet lines call rpcnotify(g:neovide_channel_id, 'neovide.lines', str2nr(v:option_new))",
    )
    .await
    .ok();

    let (geometry, force) = SETTINGS
        .get::<CmdLineSettings>()
        .geometry
        .map_or_else(|| (DEFAULT_WINDOW_GEOMETRY, false), |v| (v, true));
    nvim.execute_lua(
        SETUP_GEOMETRY_LUA,
        vec![geometry.width.into(), geometry.height.into(), force.into()],
    )
    .await
    .ok();

    setup_intro_message_autocommand(nvim).await.ok();
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
