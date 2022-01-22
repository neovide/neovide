mod clipboard;
pub mod create;
mod events;
mod handler;
mod setup;
mod tx_wrapper;
mod ui_commands;

use std::{path::Path, process::Stdio, sync::Arc, thread};

use log::{error, info, warn};
use nvim_rs::UiAttachOptions;
use tokio::process::Command;

use crate::{
    cmd_line::CmdLineSettings, error_handling::ResultPanicExplanation, running_tracker::*,
    settings::*,
};

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
            std::process::exit(0);
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

#[cfg(windows)]
fn set_windows_creation_flags(cmd: &mut Command) {
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
}

fn build_nvim_cmd_with_args(bin: &str) -> Command {
    let mut args = vec!["--embed".to_string()];
    args.extend(SETTINGS.get::<CmdLineSettings>().neovim_args);

    #[cfg(windows)]
    if SETTINGS.get::<CmdLineSettings>().wsl {
        let mut cmd = Command::new("wsl");
        let argstring = format!("{} {}", bin.trim(), args.join(" "));
        cmd.args(&["$SHELL", "-lc", &argstring]);
        return cmd;
    }
    let mut cmd = Command::new(bin);
    cmd.args(args);
    cmd
}

fn platform_exists(bin: &str) -> bool {
    #[cfg(windows)]
    if SETTINGS.get::<CmdLineSettings>().wsl {
        if let Ok(output) = std::process::Command::new("wsl")
            .args(&["$SHELL", "-lic"])
            .arg(format!("exists -x {}", bin))
            .output()
        {
            return output.status.success();
        } else {
            error!("wsl exists failed");
            std::process::exit(1);
        }
    }
    Path::new(&bin).exists()
}

fn platform_which(bin: &str) -> Option<String> {
    #[cfg(windows)]
    if SETTINGS.get::<CmdLineSettings>().wsl {
        if let Ok(output) = std::process::Command::new("wsl")
            .args(&["$SHELL", "-lic"])
            .arg(format!("which {}", bin))
            .output()
        {
            if output.status.success() {
                return Some(String::from_utf8(output.stdout).unwrap());
            } else {
                return None;
            }
        }
    }
    if let Ok(path) = which::which(bin) {
        path.into_os_string().into_string().ok()
    } else {
        None
    }
}

fn build_nvim_cmd() -> Command {
    if let Some(path) = SETTINGS.get::<CmdLineSettings>().neovim_bin {
        if platform_exists(&path) {
            return build_nvim_cmd_with_args(&path);
        } else {
            warn!("NEOVIM_BIN is invalid falling back to first bin in PATH");
        }
    }
    if let Some(path) = platform_which("nvim") {
        build_nvim_cmd_with_args(&path)
    } else {
        error!("nvim not found!");
        std::process::exit(1);
    }
}

pub fn create_nvim_command() -> Command {
    let mut cmd = build_nvim_cmd();

    info!("Starting neovim with: {:?}", cmd);

    #[cfg(not(debug_assertions))]
    cmd.stderr(Stdio::piped());

    #[cfg(debug_assertions)]
    cmd.stderr(Stdio::inherit());

    #[cfg(windows)]
    set_windows_creation_flags(&mut cmd);

    cmd
}
