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
use log::info;
use nvim_rs::{error::CallError, Neovim, UiAttachOptions, Value};
use rmpv::Utf8String;
use std::{io::Error, ops::Add, time::Duration};
use tokio::{
    runtime::{Builder, Runtime},
    select,
    time::timeout,
};
use winit::event_loop::EventLoopProxy;

use crate::{
    cmd_line::CmdLineSettings, editor::start_editor, settings::*, units::GridSize,
    window::UserEvent,
};
pub use handler::NeovimHandler;
use session::{NeovimInstance, NeovimSession};
use setup::{get_api_information, setup_neovide_specific_state};

pub use command::create_nvim_command;
pub use events::*;
pub use session::NeovimWriter;
pub use ui_commands::{send_ui, start_ui_command_handler, ParallelCommand, SerialCommand};

const NEOVIM_REQUIRED_VERSION: &str = "0.10.0";

pub struct NeovimRuntime {
    runtime: Runtime,
}

fn neovim_instance() -> Result<NeovimInstance> {
    if let Some(address) = SETTINGS.get::<CmdLineSettings>().server {
        Ok(NeovimInstance::Server { address })
    } else {
        let cmd = create_nvim_command()?;
        Ok(NeovimInstance::Embedded(cmd))
    }
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

async fn launch(handler: NeovimHandler, grid_size: Option<GridSize<u32>>) -> Result<NeovimSession> {
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

    start_ui_command_handler(session.neovim.clone());
    SETTINGS.read_initial_values(&session.neovim).await?;

    let mut options = UiAttachOptions::new();
    options.set_linegrid_external(true);
    options.set_multigrid_external(!settings.no_multi_grid);
    options.set_rgb(true);

    // Triggers loading the user config

    let grid_size = grid_size.map_or(DEFAULT_GRID_SIZE, |v| clamped_grid_size(&v));
    let res = session
        .neovim
        .ui_attach(grid_size.width as i64, grid_size.height as i64, &options)
        .await
        .context("Could not attach ui to neovim process");

    info!("Neovim process attached");
    res.map(|()| session)
}

async fn run(session: NeovimSession, proxy: EventLoopProxy<UserEvent>) {
    let mut session = session;

    if let Some(process) = session.neovim_process.as_mut() {
        // We primarily wait for the stdio to finish, but due to bugs,
        // for example, this one in in Neovim 0.9.5
        // https://github.com/neovim/neovim/issues/26743
        // it does not always finish.
        // So wait for some additional time, both to make the bug obvious and to prevent incomplete
        // data.
        select! {
            _ = &mut session.io_handle => {}
            _ = process.wait() => {
                log::info!("The Neovim process quit before the IO stream, waiting two seconds");
                if timeout(Duration::from_millis(2000), session.io_handle)
                        .await
                        .is_err()
                {
                    log::info!("The IO stream was never closed, forcing Neovide to exit");
                }
            }
        };
    } else {
        session.io_handle.await.ok();
    }
    log::info!("Neovim has quit");
    proxy.send_event(UserEvent::NeovimExited).ok();
}

impl NeovimRuntime {
    pub fn new() -> Result<Self, Error> {
        let runtime = Builder::new_multi_thread().enable_all().build()?;

        Ok(Self { runtime })
    }

    pub fn launch(
        &mut self,
        event_loop_proxy: EventLoopProxy<UserEvent>,
        grid_size: Option<GridSize<u32>>,
    ) -> Result<()> {
        let handler = start_editor(event_loop_proxy.clone());
        let session = self.runtime.block_on(launch(handler, grid_size))?;
        self.runtime.spawn(run(session, event_loop_proxy));
        Ok(())
    }
}
