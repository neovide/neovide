#[macro_use]
pub mod layouts;

mod events;
mod handler;
mod ui_commands;

use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use log::{error, info, trace, warn};
use nvim_rs::{create::tokio as create, UiAttachOptions};
use rmpv::Value;
use tokio::process::Command;
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use crate::error_handling::ResultPanicExplanation;
use crate::settings::*;
use crate::window::window_geometry_or_default;
pub use events::*;
use handler::NeovimHandler;
pub use layouts::*;
use std::env;
use std::path::Path;
pub use ui_commands::UiCommand;

lazy_static! {
    pub static ref BRIDGE: Bridge = Bridge::new();
}

#[cfg(windows)]
fn set_windows_creation_flags(cmd: &mut Command) {
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
}

#[cfg(windows)]
fn platform_build_nvim_cmd(bin: &str) -> Option<Command> {
    if !Path::new(&bin).exists() {
        return None;
    }

    if env::args().any(|arg| arg == "--wsl") {
        let mut cmd = Command::new("wsl");
        cmd.arg(bin);
        Some(cmd)
    } else {
        Some(Command::new(bin))
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
    if let Ok(path) = env::var("NEOVIM_BIN") {
        if let Some(cmd) = platform_build_nvim_cmd(&path) {
            return cmd;
        } else {
            warn!("NEOVIM_BIN is invalid falling back to first bin in PATH");
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
        .args(SETTINGS.neovim_arguments.iter().skip(1))
        .stderr(Stdio::inherit());

    #[cfg(windows)]
    set_windows_creation_flags(&mut cmd);

    cmd
}

async fn drain(receiver: &mut UnboundedReceiver<UiCommand>) -> Option<Vec<UiCommand>> {
    if let Some(ui_command) = receiver.recv().await {
        let mut results = vec![ui_command];
        while let Ok(ui_command) = receiver.try_recv() {
            results.push(ui_command);
        }
        Some(results)
    } else {
        None
    }
}

async fn start_process(mut receiver: UnboundedReceiver<UiCommand>) {
    let (width, height) = window_geometry_or_default();
    let (mut nvim, io_handler, _) =
        create::new_child_cmd(&mut create_nvim_command(), NeovimHandler())
            .await
            .unwrap_or_explained_panic("Could not locate or start the neovim process");

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
        BRIDGE.running.store(false, Ordering::Relaxed);
    });

    if let Ok(Value::Integer(correct_version)) = nvim.eval("has(\"nvim-0.4\")").await {
        if correct_version.as_i64() != Some(1) {
            error!("Neovide requires version 0.4 or higher");
            std::process::exit(0);
        }
    } else {
        error!("Neovide requires version 0.4 or higher");
        std::process::exit(0);
    };

    nvim.set_var("neovide", Value::Boolean(true))
        .await
        .unwrap_or_explained_panic("Could not communicate with neovim process");
    let mut options = UiAttachOptions::new();
    options.set_linegrid_external(true);
    options.set_rgb(true);
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

    nvim.ui_attach(width as i64, height as i64, &options)
        .await
        .unwrap_or_explained_panic("Could not attach ui to neovim process");
    info!("Neovim process attached");

    let nvim = Arc::new(nvim);
    let input_nvim = nvim.clone();
    tokio::spawn(async move {
        info!("UiCommand processor started");
        while let Some(commands) = drain(&mut receiver).await {
            if !BRIDGE.running.load(Ordering::Relaxed) {
                return;
            }
            let (resize_list, other_commands): (Vec<UiCommand>, Vec<UiCommand>) = commands
                .into_iter()
                .partition(|command| command.is_resize());

            for command in resize_list
                .into_iter()
                .last()
                .into_iter()
                .chain(other_commands.into_iter())
            {
                let input_nvim = input_nvim.clone();
                tokio::spawn(async move {
                    if !BRIDGE.running.load(Ordering::Relaxed) {
                        return;
                    }
                    trace!("Executing UiCommand: {:?}", &command);
                    command.execute(&input_nvim).await;
                });
            }
        }
    });

    SETTINGS.read_initial_values(&nvim).await;
    SETTINGS.setup_changed_listeners(&nvim).await;

    nvim.set_option("lazyredraw", Value::Boolean(false))
        .await
        .ok();
}

pub struct Bridge {
    _runtime: Runtime, // Necessary to keep runtime running
    sender: UnboundedSender<UiCommand>,
    pub running: AtomicBool,
}

impl Bridge {
    pub fn new() -> Bridge {
        let runtime = Runtime::new().unwrap();
        let (sender, receiver) = unbounded_channel::<UiCommand>();

        runtime.spawn(async move {
            start_process(receiver).await;
        });
        Bridge {
            _runtime: runtime,
            sender,
            running: AtomicBool::new(true),
        }
    }

    pub fn queue_command(&self, command: UiCommand) {
        if !BRIDGE.running.load(Ordering::Relaxed) {
            return;
        }
        trace!("UiCommand queued: {:?}", &command);
        self.sender.send(command).unwrap_or_explained_panic(
            "Could not send UI command from the window system to the neovim process.",
        );
    }
}
