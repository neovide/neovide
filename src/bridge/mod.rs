mod clipboard;
mod command;
mod events;
mod handler;
pub mod session;
mod setup;
mod ui_commands;

use std::{process::exit, sync::Arc, thread};

use log::{error, info};
use nvim_rs::{error::CallError, Neovim, UiAttachOptions, Value};

use crate::{
    cmd_line::CmdLineSettings, error_handling::ResultPanicExplanation, running_tracker::*,
    settings::*,
};

pub use command::create_nvim_command;
pub use events::*;
use handler::NeovimHandler;
pub use session::NeovimWriter;
use session::{NeovimInstance, NeovimSession};
use setup::setup_neovide_specific_state;
pub use ui_commands::{start_ui_command_handler, ParallelCommand, SerialCommand, UiCommand};

const INTRO_MESSAGE_LUA: &str = include_str!("../../lua/intro.lua");

fn neovim_instance() -> NeovimInstance {
    if let Some(address) = SETTINGS.get::<CmdLineSettings>().server {
        NeovimInstance::Server { address }
    } else {
        NeovimInstance::Embedded(create_nvim_command())
    }
}

pub fn start_bridge() {
    // hoisted out of the actual thread so error messages while trying to find nvim can be printed before forking
    let instance = neovim_instance();
    thread::spawn(|| {
        start_neovim_runtime(instance);
    });
}

pub async fn setup_intro_message_autocommand(
    nvim: &Neovim<NeovimWriter>,
) -> Result<Value, Box<CallError>> {
    let args = vec![Value::from("setup_autocommand")];
    nvim.exec_lua(INTRO_MESSAGE_LUA, args).await
}

pub async fn show_intro_message(
    nvim: &Neovim<NeovimWriter>,
    message: &[String],
) -> Result<Value, Box<CallError>> {
    let mut args = vec![Value::from("show_intro")];
    let lines = message.iter().map(|line| Value::from(line.as_str()));
    args.extend(lines);
    nvim.exec_lua(INTRO_MESSAGE_LUA, args).await
}

#[tokio::main]
async fn start_neovim_runtime(instance: NeovimInstance) {
    let handler = NeovimHandler::new();
    let session = NeovimSession::new(instance, handler)
        .await
        .unwrap_or_explained_panic("Could not locate or start neovim process");

    let nvim = Arc::new(session.neovim);

    // Check the neovim version to ensure its high enough
    match nvim.command_output("echo has('nvim-0.4')").await.as_deref() {
        Ok("1") => {} // This is just a guard
        _ => {
            error!("Neovide requires nvim version 0.4 or higher. Download the latest version here https://github.com/neovim/neovim/wiki/Installing-Neovim");
            exit(0);
        }
    }

    let settings = SETTINGS.get::<CmdLineSettings>();

    let should_handle_clipboard = settings.wsl || settings.server.is_some();
    setup_neovide_specific_state(&nvim, should_handle_clipboard).await;

    let mut options = UiAttachOptions::new();
    options.set_linegrid_external(true);
    options.set_multigrid_external(settings.multi_grid);
    options.set_rgb(true);

    // Triggers loading the user's config
    // Set to DEFAULT_WINDOW_GEOMETRY first, draw_frame will resize it later
    let geometry = DEFAULT_WINDOW_GEOMETRY;
    nvim.ui_attach(geometry.width as i64, geometry.height as i64, &options)
        .await
        .unwrap_or_explained_panic("Could not attach ui to neovim process");

    info!("Neovim process attached");

    start_ui_command_handler(nvim.clone());
    SETTINGS.read_initial_values(&nvim).await;
    SETTINGS.setup_changed_listeners(&nvim).await;

    match session.io_handle.await {
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
