pub mod create;
mod events;
mod handler;
mod tx_wrapper;
mod ui_commands;

use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;

use log::{error, info, warn};
use nvim_rs::UiAttachOptions;
use rmpv::Value;
use tokio::process::Command;
use tokio::runtime::Runtime;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::channel_utils::*;
use crate::running_tracker::*;
use crate::settings::*;
use crate::{cmd_line::CmdLineSettings, error_handling::ResultPanicExplanation};
pub use events::*;
use handler::NeovimHandler;
pub use tx_wrapper::{TxWrapper, WrapTx};
pub use ui_commands::{start_ui_command_handler, ParallelCommand, SerialCommand, UiCommand};

#[cfg(windows)]
fn set_windows_creation_flags(cmd: &mut Command) {
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
}

#[cfg(windows)]
fn platform_build_nvim_cmd(bin: &str) -> Option<Command> {
    if SETTINGS.get::<CmdLineSettings>().wsl {
        let mut cmd = Command::new("wsl");
        cmd.args(&[
            bin.trim(),
            "-c",
            "let \\$PATH=system(\"\\$SHELL -lic 'echo \\$PATH' 2>/dev/null\")",
        ]);
        Some(cmd)
    } else if Path::new(&bin).exists() {
        Some(Command::new(bin))
    } else {
        None
    }
}

#[cfg(unix)]
fn platform_build_nvim_cmd(bin: &str) -> Option<Command> {
    if Path::new(&bin).exists() {
        Some(Command::new(bin))
    } else {
        None
    }
}

fn build_nvim_cmd() -> Command {
    if let Some(path) = SETTINGS.get::<CmdLineSettings>().neovim_bin {
        if let Some(cmd) = platform_build_nvim_cmd(&path) {
            return cmd;
        } else {
            warn!("NEOVIM_BIN is invalid falling back to first bin in PATH");
        }
    }
    #[cfg(windows)]
    if SETTINGS.get::<CmdLineSettings>().wsl {
        if let Ok(output) = std::process::Command::new("wsl")
            .args(&["$SHELL", "-lic", "which nvim"])
            .output()
        {
            if output.status.success() {
                let path = String::from_utf8(output.stdout).unwrap();
                let mut cmd = Command::new("wsl");
                cmd.args(&[
                    path.trim(),
                    "-c",
                    "let \\$PATH=system(\"\\$SHELL -lic 'echo \\$PATH' 2>/dev/null\")",
                ]);
                return cmd;
            } else {
                error!("nvim not found in WSL path");
                std::process::exit(1);
            }
        } else {
            error!("wsl which nvim failed");
            std::process::exit(1);
        }
    }
    if let Ok(path) = which::which("nvim") {
        if let Some(cmd) = platform_build_nvim_cmd(path.to_str().unwrap()) {
            cmd
        } else {
            error!("nvim does not have proper permissions!");
            std::process::exit(1);
        }
    } else {
        error!("nvim not found!");
        std::process::exit(1);
    }
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
            "command! -nargs={} -complete=expression {} call rpcnotify({}, 'neovide.{}')",
            nargs, command, channel, event
        );
    } else {
        return format!(
            "command! -nargs={} -complete=expression {} call rpcnotify({}, 'neovide.{}', <args>)",
            nargs, command, channel, event
        );
    };
}

pub fn create_nvim_command() -> Command {
    let mut cmd = build_nvim_cmd();

    cmd.arg("--embed")
        .args(SETTINGS.get::<CmdLineSettings>().neovim_args.iter())
        .args(SETTINGS.get::<CmdLineSettings>().files_to_open.iter());

    info!("Starting neovim with: {:?}", cmd);

    #[cfg(not(debug_assertions))]
    cmd.stderr(Stdio::piped());

    #[cfg(debug_assertions)]
    cmd.stderr(Stdio::inherit());

    #[cfg(windows)]
    set_windows_creation_flags(&mut cmd);

    cmd
}

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

async fn start_neovim_runtime(
    ui_command_sender: LoggingTx<UiCommand>,
    ui_command_receiver: UnboundedReceiver<UiCommand>,
    redraw_event_sender: LoggingTx<RedrawEvent>,
) {
    let handler = NeovimHandler::new(ui_command_sender.clone(), redraw_event_sender.clone());
    let (nvim, io_handler) = match connection_mode() {
        ConnectionMode::Child => create::new_child_cmd(&mut create_nvim_command(), handler).await,
        ConnectionMode::RemoteTcp(address) => create::new_tcp(address, handler).await,
    }
    .unwrap_or_explained_panic("Could not locate or start neovim process");

    if nvim.get_api_info().await.is_err() {
        error!("Cannot get neovim api info, either neovide is launched with an unknown command line option or neovim version not supported!");
        std::process::exit(-1);
    }

    tokio::spawn(async move {
        info!("Close watcher started");
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
    });

    match nvim.command_output("echo has('nvim-0.4')").await.as_deref() {
        Ok("1") => {} // This is just a guard
        _ => {
            error!("Neovide requires nvim version 0.4 or higher. Download the latest version here https://github.com/neovim/neovim/wiki/Installing-Neovim");
            std::process::exit(0);
        }
    }

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

    nvim.set_client_info(
        "neovide",
        vec![
            (Value::from("major"), Value::from(0u64)),
            (Value::from("minor"), Value::from(6u64)),
        ],
        "ui",
        vec![],
        vec![],
    )
    .await
    .ok();

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

    info!(
        "Neovide registered to nvim with channel id {}",
        neovide_channel
    );

    #[cfg(windows)]
    nvim.command(&build_neovide_command(
        neovide_channel,
        0,
        "NeovideRegisterRightClick",
        "register_right_click",
    ))
    .await
    .ok();

    #[cfg(windows)]
    nvim.command(&build_neovide_command(
        neovide_channel,
        0,
        "NeovideUnregisterRightClick",
        "unregister_right_click",
    ))
    .await
    .ok();

    nvim.set_option("lazyredraw", Value::Boolean(false))
        .await
        .ok();
    nvim.set_option("termguicolors", Value::Boolean(true))
        .await
        .ok();

    let settings = SETTINGS.get::<CmdLineSettings>();
    let geometry = settings.geometry;
    let mut options = UiAttachOptions::new();
    options.set_linegrid_external(true);
    options.set_multigrid_external(settings.multi_grid);
    options.set_rgb(true);
    nvim.ui_attach(geometry.width as i64, geometry.height as i64, &options)
        .await
        .unwrap_or_explained_panic("Could not attach ui to neovim process");

    info!("Neovim process attached");

    let nvim = Arc::new(nvim);

    start_ui_command_handler(ui_command_receiver, nvim.clone());
    SETTINGS.read_initial_values(&nvim).await;
    SETTINGS.setup_changed_listeners(&nvim).await;
}

pub struct Bridge {
    _runtime: Runtime, // Necessary to keep runtime running
}

pub fn start_bridge(
    ui_command_sender: LoggingTx<UiCommand>,
    ui_command_receiver: UnboundedReceiver<UiCommand>,
    redraw_event_sender: LoggingTx<RedrawEvent>,
) -> Bridge {
    let runtime = Runtime::new().unwrap();
    runtime.spawn(start_neovim_runtime(
        ui_command_sender,
        ui_command_receiver,
        redraw_event_sender,
    ));
    Bridge { _runtime: runtime }
}
