mod api_info;
mod clipboard;
mod command;
mod events;
mod handler;
pub mod session;
mod setup;
mod ui_commands;

use std::{io::Error, ops::Add, sync::Arc, time::Duration};

use crate::{
    cmd_line::CmdLineSettings,
    editor::start_editor_handler,
    running_tracker::RunningTracker,
    settings::*,
    units::GridSize,
    window::{EventPayload, UserEvent},
};
use anyhow::{bail, Context, Result};
pub use handler::NeovimHandler;
use itertools::Itertools;
use log::info;
use nvim_rs::{error::CallError, Neovim, UiAttachOptions, Value};
use rmpv::Utf8String;
use session::{NeovimInstance, NeovimSession};
use setup::{get_api_information, setup_neovide_specific_state};
use tokio::{
    runtime::{Builder, Runtime},
    select,
    time::timeout,
};
use winit::event_loop::EventLoopProxy;

pub use command::create_nvim_command;
pub use events::*;
pub use session::NeovimWriter;
pub use ui_commands::{
    send_ui, start_ui_command_handler, ParallelCommand, SerialCommand, HANDLER_REGISTRY,
};

const NEOVIM_REQUIRED_VERSION: &str = "0.10.0";

pub struct NeovimRuntime {
    pub runtime: Runtime,
}

fn neovim_instance(settings: &Settings) -> Result<NeovimInstance> {
    if let Some(address) = settings.get::<CmdLineSettings>().server {
        Ok(NeovimInstance::Server { address })
    } else {
        let cmd = create_nvim_command(settings)?;
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

// TODO: this function name is bringing confusion and is duplicated
// conflicting with the runtime.launch fn, it should be renamed
// to something else
async fn create_neovim_session(
    handler: NeovimHandler,
    grid_size: Option<GridSize<u32>>,
    settings: Arc<Settings>,
) -> Result<NeovimSession> {
    let neovim_instance = neovim_instance(settings.as_ref())?;

    let session = NeovimSession::new(neovim_instance, handler.clone())
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

    let cmdline_settings = settings.get::<CmdLineSettings>();

    let should_handle_clipboard = cmdline_settings.wsl || cmdline_settings.server.is_some();
    let api_information = get_api_information(&session.neovim).await?;
    info!(
        "Neovide registered to nvim with channel id {}",
        api_information.channel
    );
    // This is too verbose to keep enabled all the time
    // log::info!("Api information {:#?}", api_information);
    setup_neovide_specific_state(
        &session.neovim,
        should_handle_clipboard,
        &api_information,
        &settings,
    )
    .await?;

    start_ui_command_handler(handler.clone(), session.neovim.clone(), settings.clone());
    settings.read_initial_values(&session.neovim).await?;

    let mut options = UiAttachOptions::new();
    options.set_linegrid_external(true);
    options.set_multigrid_external(!cmdline_settings.no_multi_grid);
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

async fn run(
    winit_window_id: winit::window::WindowId,
    session: NeovimSession,
    proxy: EventLoopProxy<EventPayload>,
) {
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
                // Wait a little bit more if we detect that Neovim exits before the stream, to
                // allow us to finish reading from it.
                log::info!("The Neovim process quit before the IO stream, waiting for a half second");
                if timeout(Duration::from_millis(500), &mut session.io_handle)
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
    proxy
        .send_event(EventPayload::new(UserEvent::NeovimExited, winit_window_id))
        .ok();
}

impl NeovimRuntime {
    pub fn new() -> Result<Self, Error> {
        let runtime = Builder::new_multi_thread().enable_all().build()?;

        Ok(Self { runtime })
    }

    pub fn launch(
        &mut self,
        winit_window_id: winit::window::WindowId,
        event_loop_proxy: EventLoopProxy<EventPayload>,
        grid_size: Option<GridSize<u32>>,
        running_tracker: RunningTracker,
        settings: Arc<Settings>,
    ) -> Result<NeovimHandler> {
        let editor_handler = start_editor_handler(
            winit_window_id,
            event_loop_proxy.clone(),
            running_tracker,
            settings.clone(),
        );

        let session = self.runtime.block_on(create_neovim_session(
            editor_handler.clone(),
            grid_size,
            settings,
        ))?;

        self.runtime
            .spawn(run(winit_window_id, session, event_loop_proxy));

        Ok(editor_handler)
    }
}
