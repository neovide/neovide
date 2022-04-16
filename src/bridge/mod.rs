mod clipboard;
mod command;
pub mod create;
mod events;
mod handler;
mod setup;
mod tx_wrapper;
mod ui_commands;

use std::{process::exit, sync::Arc, thread};

use log::{error, info};
use nvim_rs::UiAttachOptions;

use crate::{
    cmd_line::CmdLineSettings, error_handling::ResultPanicExplanation, running_tracker::*,
    settings::*,
};

pub use command::create_nvim_command;
pub use events::*;
use handler::NeovimHandler;
use setup::setup_neovide_specific_state;
pub use tx_wrapper::{TxWrapper, WrapTx};
pub use ui_commands::{start_ui_command_handler, ParallelCommand, SerialCommand, UiCommand};

enum ConnectionMode {
    Child,
    RemoteTcp(String),
}

fn connection_mode() -> ConnectionMode {
    if let Some(arg) = SETTINGS.get::<CmdLineSettings>().remote_tcp {
        ConnectionMode::RemoteTcp(arg)
    } else {
        ConnectionMode::Child
    }
}

pub fn start_bridge() {
    thread::spawn(|| {
        start_neovim_runtime();
    });
}

#[tokio::main]
async fn start_neovim_runtime() {
    let handler = NeovimHandler::new();
    let (nvim, io_handler) = match connection_mode() {
        ConnectionMode::Child => create::new_child_cmd(&mut create_nvim_command(), handler).await,
        ConnectionMode::RemoteTcp(address) => create::new_tcp(address, handler).await,
    }
    .unwrap_or_explained_panic("Could not locate or start neovim process");

    // Check the neovim version to ensure its high enough
    match nvim.command_output("echo has('nvim-0.4')").await.as_deref() {
        Ok("1") => {} // This is just a guard
        _ => {
            error!("Neovide requires nvim version 0.4 or higher. Download the latest version here https://github.com/neovim/neovim/wiki/Installing-Neovim");
            exit(0);
        }
    }

    let settings = SETTINGS.get::<CmdLineSettings>();

    let mut is_remote = settings.wsl;
    if let ConnectionMode::RemoteTcp(_) = connection_mode() {
        is_remote = true;
    }
    setup_neovide_specific_state(&nvim, is_remote).await;

    let geometry = settings.geometry;
    let mut options = UiAttachOptions::new();
    options.set_linegrid_external(true);
    options.set_multigrid_external(settings.multi_grid);
    options.set_rgb(true);

    // Triggers loading the user's config
    nvim.ui_attach(geometry.width as i64, geometry.height as i64, &options)
        .await
        .unwrap_or_explained_panic("Could not attach ui to neovim process");

    info!("Neovim process attached");

    let nvim = Arc::new(nvim);

    start_ui_command_handler(nvim.clone());

    // Open the files into new tabs
    for file in settings.target_files.iter().skip(1) {
        nvim.command(format!("tabnew {}", file).as_str())
            .await
            .unwrap_or_explained_panic("Could not create new tab");
    }
    SETTINGS.read_initial_values(&nvim).await;
    SETTINGS.setup_changed_listeners(&nvim).await;

    match io_handler.await {
        Err(join_error) => error!("Error joining IO loop: '{}'", join_error),
        Ok(Err(error)) => {
            if !error.is_channel_closed() {
                error!("Error: '{}'", error);
            }
        }
        Ok(Ok(())) => {}
    };
    RUNNING_TRACKER.quit("neovim processed failed");
}
