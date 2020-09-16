#[macro_use]
pub mod layouts;

mod events;
mod ui_commands;

use std::process::{Stdio, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::mpsc::{Sender, Receiver};
use std::env;
use std::path::Path;
use std::thread;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use log::{error, info, warn};
use neovim_lib::{Neovim, NeovimApi, Session, UiAttachOptions};
use rmpv::Value;

use crate::error_handling::ResultPanicExplanation;
use crate::settings::*;
use crate::window::window_geometry_or_default;
pub use events::*;
pub use layouts::*;
pub use ui_commands::UiCommand;

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

pub fn start_bridge(ui_command_sender: Sender<UiCommand>, ui_command_receiver: Receiver<UiCommand>, redraw_event_sender: Sender<RedrawEvent>, running: Arc<AtomicBool>) {
    thread::spawn(move || {
        let (width, height) = window_geometry_or_default();
        let mut session = Session::new_child_cmd(&mut create_nvim_command()).unwrap_or_explained_panic("Could not locate or start neovim process");
        let notification_receiver = session.start_event_loop_channel();

        let mut nvim = Neovim::new(session);

        if let Ok(Value::Integer(correct_version)) = nvim.eval("has(\"nvim-0.4\")") {
            if correct_version.as_i64() != Some(1) {
                error!("Neovide requires version 0.4 or higher");
                std::process::exit(0);
            }
        } else {
            error!("Neovide requires version 0.4 or higher");
            std::process::exit(0);
        };

        nvim.set_var("neovide", Value::Boolean(true)).unwrap_or_explained_panic("Could not communicate with neovim process");
        if let Err(command_error) = nvim.command("runtime! ginit.vim") {
            nvim.command(&format!(
                "echomsg \"error encountered in ginit.vim {:?}\"",
                command_error
            ))
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
        .ok();

        let neovide_channel: u64 = nvim
            .list_chans()
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
        .ok();

        #[cfg(windows)]
        nvim.command(&build_neovide_command(
            neovide_channel,
            0,
            "NeovideUnregisterRightClick",
            "unregister_right_click",
        ))
        .ok();

        NeovimApi::set_option(&mut nvim, "lazyredraw", Value::Boolean(false)).ok();

        let mut options = UiAttachOptions::new();
        options.set_linegrid_external(true);
        options.set_multigrid_external(true);
        options.set_rgb(true);
        nvim.ui_attach(width as i64, height as i64, &options)
            .unwrap_or_explained_panic("Could not attach ui to neovim process");
        info!("Neovim process attached");

        SETTINGS.read_initial_values(&mut nvim);
        SETTINGS.setup_changed_listeners(&mut nvim);

        let notification_running = running.clone();
        thread::spawn(move || {
            loop {
                if !notification_running.load(Ordering::Relaxed) {
                    break;
                }

                match notification_receiver.recv() {
                    Ok((event_name, arguments)) =>
                        match event_name.as_ref() {
                            "redraw" => {
                                for events in arguments {
                                    let parsed_events = parse_redraw_event(events)
                                        .unwrap_or_explained_panic("Could not parse event from neovim");

                                    for parsed_event in parsed_events {
                                        redraw_event_sender.send(parsed_event).ok();
                                    }
                                }
                            }
                            "setting_changed" => {
                                SETTINGS.handle_changed_notification(arguments);
                            }
                            #[cfg(windows)]
                            "neovide.register_right_click" => {
                                ui_command_sender.send(UiCommand::RegisterRightClick).ok();
                            }
                            #[cfg(windows)]
                            "neovide.unregister_right_click" => {
                                ui_command_sender.send(UiCommand::UnregisterRightClick).ok();
                            }
                            _ => {}
                        },
                    Err(error) => {
                        notification_running.store(false, Ordering::Relaxed);
                        break;
                    }
                }
            } 
        });

        let ui_command_running = running.clone();
        thread::spawn(move || {
            loop {
                if !ui_command_running.load(Ordering::Relaxed) {
                    break;
                }

                match ui_command_receiver.recv() {
                    Ok(ui_command) => {
                        ui_command.execute(&mut nvim);
                    },
                    Err(error) => {
                        ui_command_running.store(false, Ordering::Relaxed);
                        break;
                    }
                }
            }
        });
    });
}
