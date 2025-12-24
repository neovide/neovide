use anyhow::{Context, Result};
use nvim_rs::{call_args, rpc::IntoVal, Neovim};
use rmpv::Value;

use super::{
    api_info::{parse_api_info, ApiInformation},
    nvim_dict, nvim_exec_output,
};
use crate::{
    bridge::NeovimWriter,
    cmd_line::CmdLineSettings,
    settings::{config::config_path, SettingLocation, Settings},
};

const INIT_LUA: &str = include_str!("../../lua/init.lua");

pub async fn get_api_information(nvim: &Neovim<NeovimWriter>) -> Result<ApiInformation> {
    // Retrieve the channel number for communicating with neovide.
    let api_info = nvim
        .get_api_info()
        .await
        .context("Error getting API info")?;

    let version = nvim_exec_output(nvim, "version").await?;
    log::info!("Neovim version: {version:#?}");
    let version_str = version.lines().next().unwrap_or_default();
    parse_api_info(&api_info, version_str).context("Failed to parse Neovim api information")
}

pub async fn setup_neovide_specific_state(
    nvim: &Neovim<NeovimWriter>,
    remote: bool,
    api_information: &ApiInformation,
    settings: &Settings,
) -> Result<()> {
    // Set variable indicating to user config that neovide is being used.
    nvim.set_var("neovide", Value::from(true))
        .await
        .context("Could not communicate with neovim process")?;

    nvim.exec2("runtime! ginit.vim", nvim_dict!())
        .await
        .context("Error encountered in ginit.vim ")?;

    // Set details about the neovide version.
    nvim.set_client_info(
        "neovide",
        nvim_dict! {
            "major" =>env!("CARGO_PKG_VERSION_MAJOR"),
            "minor" =>env!("CARGO_PKG_VERSION_MINOR"),
            "patch" =>env!("CARGO_PKG_VERSION_PATCH")
        },
        "ui",
        nvim_dict! {},
        nvim_dict! {},
    )
    .await
    .context("Error setting client info")?;

    let register_clipboard = remote;
    let register_right_click = cfg!(target_os = "windows");

    let setting_locations = settings.setting_locations();
    let global_variable_settings = setting_locations
        .iter()
        .filter_map(|s| match s {
            SettingLocation::NeovideGlobal(setting) => Some(Value::from(setting.to_owned())),
            _ => None,
        })
        .collect::<Vec<_>>();
    let option_settings = setting_locations
        .iter()
        .filter_map(|s| match s {
            SettingLocation::NeovimOption(setting) => Some(Value::from(setting.to_owned())),
            _ => None,
        })
        .collect::<Vec<_>>();

    nvim.exec_lua(
        INIT_LUA,
        call_args![nvim_dict! {
            "neovide_channel_id" => api_information.channel,
            "neovide_version" => crate_version!(),
            "config_path" => config_path().to_string_lossy().into_owned(),
            "register_clipboard" => register_clipboard,
            "register_right_click" => register_right_click,
            "remote" => remote,
            "macos_tab_project_title" => settings.get::<CmdLineSettings>().macos_tab_project_title,
            "global_variable_settings" => global_variable_settings,
            "option_settings" => option_settings,
        }],
    )
    .await
    .context("Error when running Neovide init.lua")?;

    Ok(())
}
