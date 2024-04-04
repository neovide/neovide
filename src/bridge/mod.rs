mod api_info;
mod clipboard;
mod command;
mod events;
mod handler;
pub mod session;
mod setup;
mod ui_commands;

use anyhow::{bail, Context, Result};
use itertools::Itertools;
use log::{error, info};
use nvim_rs::{error::CallError, Neovim, UiAttachOptions, Value};
use rmpv::Utf8String;
use std::{io::Error, ops::Add};
use tokio::runtime::{Builder, Runtime};
use winit::event_loop::EventLoopProxy;

use crate::{
    cmd_line::CmdLineSettings, dimensions::Dimensions, editor::start_editor, running_tracker::*,
    settings::*, window::UserEvent,
};
pub use handler::NeovimHandler;
use session::{NeovimInstance, NeovimSession};
use setup::{get_api_information, setup_neovide_specific_state};

pub use api_info::*;
pub use command::create_nvim_command;
pub use events::*;
pub use session::NeovimWriter;
pub use ui_commands::{send_ui, start_ui_command_handler, ParallelCommand, SerialCommand};

const INTRO_MESSAGE_LUA: &str = include_str!("../../lua/intro.lua");
const NEOVIM_REQUIRED_VERSION: &str = "0.9.2";

pub struct NeovimRuntime {
    runtime: Option<Runtime>,
}

fn neovim_instance() -> Result<NeovimInstance> {
    if let Some(address) = SETTINGS.get::<CmdLineSettings>().server {
        Ok(NeovimInstance::Server { address })
    } else {
        let cmd = create_nvim_command()?;
        Ok(NeovimInstance::Embedded(cmd))
    }
}

/// Takes the --cmd or -c argument and returns the command to be executed.
fn handle_command_arg(position: usize, args: Vec<String>) -> String {
    args.get(position + 1).cloned().unwrap_or_default()
}

/// Takes the valid path argument and returns the startup directory.
fn handle_command_arg_as_path_or_default(args: &mut Vec<String>) -> Option<String> {
    args.retain(|arg| is_valid_path(arg));

    let path = args.first().cloned().unwrap_or_default();
    get_startup_directory(&path).map(|startup_directory| {
        format!(
            "if g:neovide_ntty == v:true | chdir {} | endif",
            startup_directory
        )
    })
}

/// The function `setup_tty_startup_directory` sets up the startup directory for
/// a non TTY Neovim session
///
/// Any nvim --cmd or -c argument is handled as a command to be executed.
///
/// Conditions:
///
/// - Is not a TTY session.
/// - Argument is a directory, it becomes the startup directory.
/// - Argument is a file, its parent directory becomes the startup directory.
/// - Neither directory nor file, $HOME is used. (macOS specific)
pub async fn setup_tty_startup_directory(
    nvim: &Neovim<NeovimWriter>,
) -> Result<(), Box<CallError>> {
    use self::command::is_tty;

    if is_tty() {
        return Ok(());
    }

    let neovim_args = SETTINGS.get::<CmdLineSettings>().neovim_args;

    let cmd_arg = neovim_args
        .iter()
        .rposition(|arg| arg == "--cmd" || arg == "-c");

    let cmd = handle_command_arg_as_path_or_default(&mut neovim_args.clone());
    if cmd.is_none() {
        return Ok(());
    }
    let mut cmd = cmd.unwrap();

    if let Some(pos) = cmd_arg {
        cmd = format!("{} | {}", cmd, handle_command_arg(pos, neovim_args));
    }

    match nvim.command(cmd.as_str()).await {
        Ok(_) => {}
        Err(e) => log::error!("Error setting startup directory: {}", e),
    }

    Ok(())
}

fn get_startup_directory(path: &str) -> Option<String> {
    use std::path::{Path, PathBuf};

    // Only set the home directory on macOS
    #[cfg(target_os = "macos")]
    let home = Some(
        dirs::home_dir()
            .expect("Could not find home directory")
            .to_string_lossy()
            .to_string(),
    );

    #[cfg(not(target_os = "macos"))]
    let home = None;

    match path {
        arg if PathBuf::from(&arg).is_dir() => Some(arg.to_string()),
        arg if PathBuf::from(&arg).is_file() => Path::new(&arg)
            .parent()
            .map_or(home, |p| Some(p.to_string_lossy().to_string())),
        _ => home,
    }
}

fn is_valid_path(path: &str) -> bool {
    use std::path::PathBuf;

    PathBuf::from(path).is_dir() || PathBuf::from(path).is_file()
}

pub async fn setup_intro_message_autocommand(
    nvim: &Neovim<NeovimWriter>,
) -> Result<Value, Box<CallError>> {
    let args = vec![Value::from("setup_autocommand")];
    nvim.exec_lua(INTRO_MESSAGE_LUA, args).await
}

pub async fn show_error_message(
    nvim: &Neovim<NeovimWriter>,
    lines: &[String],
) -> Result<(), Box<CallError>> {
    let error_msg_highlight: Utf8String = "ErrorMsg".into();
    let mut prepared_lines = lines
        .iter()
        .map(|l| {
            Value::Array(vec![
                Value::String(l.clone().add("\n").into()),
                Value::String(error_msg_highlight.clone()),
            ])
        })
        .collect_vec();
    prepared_lines.insert(
        0,
        Value::Array(vec![
            Value::String("Error: ".into()),
            Value::String(error_msg_highlight.clone()),
        ]),
    );
    nvim.echo(prepared_lines, true, vec![]).await
}

async fn launch(handler: NeovimHandler, grid_size: Option<Dimensions>) -> Result<NeovimSession> {
    let neovim_instance = neovim_instance()?;

    let session = NeovimSession::new(neovim_instance, handler)
        .await
        .context("Could not locate or start neovim process")?;

    // Check the neovim version to ensure its high enough
    match session
        .neovim
        .command_output(&format!("echo has('nvim-{NEOVIM_REQUIRED_VERSION}')"))
        .await
        .as_deref()
    {
        Ok("1") => {} // This is just a guard
        _ => {
            bail!("Neovide requires nvim version {NEOVIM_REQUIRED_VERSION} or higher. Download the latest version here https://github.com/neovim/neovim/wiki/Installing-Neovim");
        }
    }

    let settings = SETTINGS.get::<CmdLineSettings>();

    let should_handle_clipboard = settings.wsl || settings.server.is_some();
    let api_information = get_api_information(&session.neovim).await?;
    info!(
        "Neovide registered to nvim with channel id {}",
        api_information.channel
    );
    // This is too verbose to keep enabled all the time
    // log::info!("Api information {:#?}", api_information);
    setup_neovide_specific_state(&session.neovim, should_handle_clipboard, &api_information)
        .await?;

    start_ui_command_handler(session.neovim.clone(), &api_information);
    SETTINGS.read_initial_values(&session.neovim).await?;

    let mut options = UiAttachOptions::new();
    if !api_information.has_event("win_viewport_margins") {
        options.set_hlstate_external(true);
    }
    options.set_linegrid_external(true);
    options.set_multigrid_external(!settings.no_multi_grid);
    options.set_rgb(true);

    // Triggers loading the user config

    let grid_size = grid_size.map_or(DEFAULT_GRID_SIZE, |v| v.clamped_grid_size());
    let res = session
        .neovim
        .ui_attach(grid_size.width as i64, grid_size.height as i64, &options)
        .await
        .context("Could not attach ui to neovim process");

    info!("Neovim process attached");
    res.map(|()| session)
}

async fn run(session: NeovimSession) {
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

impl NeovimRuntime {
    pub fn new() -> Result<Self, Error> {
        let runtime = Builder::new_multi_thread().enable_all().build()?;

        Ok(Self {
            runtime: Some(runtime),
        })
    }

    pub fn launch(
        &mut self,
        event_loop_proxy: EventLoopProxy<UserEvent>,
        grid_size: Option<Dimensions>,
    ) -> Result<()> {
        let handler = start_editor(event_loop_proxy);
        let runtime = self.runtime.as_ref().unwrap();
        let session = runtime.block_on(launch(handler, grid_size))?;
        runtime.spawn(run(session));
        Ok(())
    }
}

impl Drop for NeovimRuntime {
    fn drop(&mut self) {
        self.runtime.take().unwrap().shutdown_background();
    }
}
