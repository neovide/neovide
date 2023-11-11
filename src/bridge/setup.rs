use anyhow::{Context, Result};
use log::info;
use nvim_rs::Neovim;
use rmpv::Value;

use super::setup_intro_message_autocommand;
use crate::{
    bridge::NeovimWriter,
    settings::{SettingLocation, SETTINGS},
};

const INIT_LUA: &str = include_str!("../../lua/init.lua");

pub async fn setup_neovide_specific_state(
    nvim: &Neovim<NeovimWriter>,
    should_handle_clipboard: bool,
) -> Result<()> {
    // Set variable indicating to user config that neovide is being used.
    nvim.set_var("neovide", Value::Boolean(true))
        .await
        .context("Could not communicate with neovim process")?;

    nvim.command("runtime! ginit.vim")
        .await
        .context("Error encountered in ginit.vim ")?;

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
    .context("Error setting client info")?;

    // Retrieve the channel number for communicating with neovide.
    let api_info = nvim
        .get_api_info()
        .await
        .context("Error getting API info")?;

    let neovide_channel = api_info[0]
        .as_u64()
        .context("Neovide could not find the correct channel id")?;

    info!(
        "Neovide registered to nvim with channel id {}",
        neovide_channel
    );
    let neovide_channel = Value::from(neovide_channel);

    let register_clipboard = should_handle_clipboard;
    let register_right_click = cfg!(target_os = "windows");

    let settings = SETTINGS.setting_locations();
    let global_variable_settings = settings
        .iter()
        .filter_map(|s| match s {
            SettingLocation::NeovideGlobal(setting) => Some(Value::from(setting.to_owned())),
            _ => None,
        })
        .collect::<Vec<_>>();
    let option_settings = settings
        .iter()
        .filter_map(|s| match s {
            SettingLocation::NeovimOption(setting) => Some(Value::from(setting.to_owned())),
            _ => None,
        })
        .collect::<Vec<_>>();

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
        (
            Value::from("global_variable_settings"),
            Value::from(global_variable_settings),
        ),
        (Value::from("option_settings"), Value::from(option_settings)),
    ]);

    nvim.execute_lua(INIT_LUA, vec![args])
        .await
        .context("Error when running Neovide init.lua")?;

    setup_intro_message_autocommand(nvim)
        .await
        .context("Error setting up intro message")?;

    Ok(())
}
