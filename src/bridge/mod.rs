mod api_info;
mod clipboard;
mod command;
mod events;
mod handler;
mod restart;
pub mod session;
mod setup;
mod ui_commands;

use std::{
    io::Error,
    ops::Add,
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::{
    clipboard::ClipboardHandle,
    cmd_line::CmdLineSettings,
    editor::start_editor_handler,
    running_tracker::RunningTracker,
    settings::*,
    units::GridSize,
    window::{EventPayload, UserEvent, WindowSettings},
};
use anyhow::{bail, Context, Result};
use futures::StreamExt;
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

use command::{create_nvim_command_with_args, create_restart_nvim_command};
pub use command::create_blocking_nvim_command;
#[cfg(test)]
#[cfg(test)]
pub use command::create_nvim_command;
pub use events::*;
pub use restart::RestartDetails;
pub use session::NeovimWriter;
pub use ui_commands::{
    send_ui, start_ui_command_handler, ParallelCommand, SerialCommand, HANDLER_REGISTRY,
};

const NEOVIM_REQUIRED_VERSION: (u64, u64, u64) = (0, 10, 0);

macro_rules! nvim_dict {
    ( $( $key:expr => $value:expr ),* $(,)? ) => {
        vec![
            $( (Value::from($key), Value::from($value)) ),*
        ]
    };
}
pub(crate) use nvim_dict;

/// nvim_command_output is deprecated, so use our own version
async fn nvim_exec_output(
    nvim: &Neovim<NeovimWriter>,
    func: &str,
) -> Result<String, Box<CallError>> {
    let result = nvim
        .exec2(
            func,
            nvim_dict! {
                "output" => true,
            },
        )
        .await?;
    Ok(result
        .iter()
        .find(|(k, _)| k.as_str() == Some("output"))
        .and_then(|(_, v)| v.as_str())
        .unwrap_or("")
        .to_string())
}

pub struct NeovimRuntime {
    pub runtime: Runtime,
    clipboard: ClipboardHandle,
    background_preference: Arc<Mutex<String>>,
}

async fn neovim_instance(
    settings: &Settings,
    restart: Option<&RestartDetails>,
    override_nvim_args: Option<&[String]>,
) -> Result<NeovimInstance> {
    if let Some(info) = restart {
        return Ok(NeovimInstance::Embedded(create_restart_nvim_command(info)));
    }

    if let Some(address) = settings.get::<CmdLineSettings>().server {
        if override_nvim_args.is_some() {
            bail!("Cannot override nvim args when connecting to a server instance");
        }
        return Ok(NeovimInstance::Server { address });
    }

    Ok(NeovimInstance::Embedded(create_nvim_command_with_args(
        settings,
        override_nvim_args,
    )))
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
    nvim.echo(prepared_lines, true, nvim_dict! {}).await
}

// TODO: this function name is bringing confusion and is duplicated
// conflicting with the runtime.launch fn, it should be renamed
// to something else
async fn create_neovim_session(
    handler: NeovimHandler,
    grid_size: Option<GridSize<u32>>,
    settings: Arc<Settings>,
    background: &str,
    restart_details: Option<&RestartDetails>,
    override_nvim_args: Option<Vec<String>>,
) -> Result<NeovimSession> {
    let neovim_instance =
        neovim_instance(settings.as_ref(), restart_details, override_nvim_args.as_deref()).await?;
    #[allow(unused_mut)]
    let mut session = NeovimSession::new(neovim_instance, handler.clone())
        .await
        .context("Could not locate or start neovim process")?;

    let api_information = get_api_information(&session.neovim).await?;
    info!(
        "Neovide registered to nvim with channel id {}",
        api_information.channel
    );

    let (major, minor, patch) = NEOVIM_REQUIRED_VERSION;
    if !api_information
        .version
        .has_version(major, minor, patch, None)
    {
        let found = api_information.version.string;
        bail!("Neovide requires nvim version {major}.{minor}.{patch} or higher, but {found} was detected. Download the latest version here https://github.com/neovim/neovim/wiki/Installing-Neovim");
    }

    let cmdline_settings = settings.get::<CmdLineSettings>();

    let remote =
        cmdline_settings.wsl || (cmdline_settings.server.is_some() && restart_details.is_none());
    // This is too verbose to keep enabled all the time
    // log::info!("Api information {:#?}", api_information);
    setup_neovide_specific_state(&session.neovim, remote, &api_information, &settings).await?;
    if api_information.version.has_version(0, 12, 0, Some(1264)) {
        let mut window_settings = settings.get::<WindowSettings>();
        window_settings.has_mouse_grid_detection = true;
        settings.set::<WindowSettings>(&window_settings);
    }

    start_ui_command_handler(handler.clone(), session.neovim.clone(), settings.clone());
    settings.read_initial_values(&session.neovim).await?;
    set_background_if_allowed(background, &session.neovim).await;

    let mut options = UiAttachOptions::new();
    options.set_linegrid_external(true);
    options.set_multigrid_external(!cmdline_settings.no_multi_grid);
    options.set_rgb(true);
    // We can close the handle here, as Neovim already owns it
    #[cfg(not(target_os = "windows"))]
    if let Some(fd) = session.stdin_fd.take() {
        use rustix::fd::AsRawFd;
        if let Ok(fd) = fd.as_raw_fd().try_into() {
            options.set_stdin_fd(fd);
        }
    }

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

    // Try to ensure that the stderr output has finished
    if let Some(stderr_task) = &mut session.stderr_task {
        timeout(Duration::from_millis(500), stderr_task).await.ok();
    };

    log::info!("Neovim has quit");
    proxy
        .send_event(EventPayload::for_window(UserEvent::NeovimExited, winit_window_id))
        .ok();
}

pub async fn set_background_if_allowed(background: &str, neovim: &Neovim<NeovimWriter>) {
    // Unfortunately neovim does not set the last_set_chan for options when they are set through
    // exec_lua. The last_set_sid is also generic, so we are forced to do two calls.
    if let Ok(can_set) = neovim
        .exec_lua(
            "return neovide.private.can_set_background()",
            vec![background.into()],
        )
        .await
    {
        if can_set.as_bool().unwrap() {
            let _ = neovim.set_option("background", background.into()).await;
        }
    }
}

fn background_from_preferences(preferences: &mundy::Preferences) -> Option<&'static str> {
    match preferences.color_scheme {
        mundy::ColorScheme::Dark => Some("dark"),
        mundy::ColorScheme::Light => Some("light"),
        // At least KDE Plasma sends this after sending the actual color scheme
        // So do nothing
        mundy::ColorScheme::NoPreference => None,
    }
}

async fn initial_background_from_stream(stream: &mut mundy::PreferencesStream) -> String {
    match timeout(Duration::from_millis(200), stream.next()).await {
        Ok(Some(preferences)) => background_from_preferences(&preferences)
            .unwrap_or("dark")
            .to_string(),
        Ok(None) => "dark".to_string(),
        Err(_) => "dark".to_string(),
    }
}

async fn update_colorscheme(
    mut stream: mundy::PreferencesStream,
    background_preference: Arc<Mutex<String>>,
    handler: NeovimHandler,
) {
    while let Some(preferences) = stream.next().await {
        if let Some(background) = background_from_preferences(&preferences) {
            {
                if let Ok(mut guard) = background_preference.lock() {
                    guard.clear();
                    guard.push_str(background);
                }
            }
            send_ui(
                ParallelCommand::SetBackground {
                    background: background.to_string(),
                },
                &handler,
            );
        }
    }
}

impl NeovimRuntime {
    pub fn new(clipboard: ClipboardHandle) -> Result<Self, Error> {
        let runtime = Builder::new_multi_thread().enable_all().build()?;

        Ok(Self {
            runtime,
            clipboard,
            background_preference: Arc::new(Mutex::new("dark".to_string())),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn launch(
        &mut self,
        winit_window_id: winit::window::WindowId,
        event_loop_proxy: EventLoopProxy<EventPayload>,
        grid_size: Option<GridSize<u32>>,
        running_tracker: RunningTracker,
        settings: Arc<Settings>,
        mut colorscheme_stream: mundy::PreferencesStream,
        override_nvim_args: Option<Vec<String>>,
    ) -> Result<NeovimHandler> {
        let editor_handler = start_editor_handler(
            winit_window_id,
            event_loop_proxy.clone(),
            running_tracker,
            settings.clone(),
            self.clipboard.clone(),
        );
        let initial_background = self
            .runtime
            .block_on(initial_background_from_stream(&mut colorscheme_stream));
        self.set_background_preference(&initial_background);

        let session = self.runtime.block_on(create_neovim_session(
            editor_handler.clone(),
            grid_size,
            settings,
            &initial_background,
            None,
            override_nvim_args,
        ))?;

        self.runtime.spawn(update_colorscheme(
            colorscheme_stream,
            self.background_preference.clone(),
            editor_handler.clone(),
        ));

        self.runtime
            .spawn(run(winit_window_id, session, event_loop_proxy));

        Ok(editor_handler)
    }

    pub fn shutdown(self, timeout: Duration) {
        self.runtime.shutdown_timeout(timeout);
    }

    pub fn restart(
        &mut self,
        winit_window_id: winit::window::WindowId,
        event_loop_proxy: EventLoopProxy<EventPayload>,
        handler: NeovimHandler,
        grid_size: GridSize<u32>,
        settings: Arc<Settings>,
        restart_details: RestartDetails,
    ) -> Result<()> {
        let background = self.current_background();
        let session = self.runtime.block_on(create_neovim_session(
            handler,
            Some(grid_size),
            settings,
            &background,
            Some(&restart_details),
            None,
        ))?;

        self.runtime
            .spawn(run(winit_window_id, session, event_loop_proxy));

        Ok(())
    }

    fn set_background_preference(&self, background: &str) {
        if let Ok(mut guard) = self.background_preference.lock() {
            guard.clear();
            guard.push_str(background);
        }
    }

    fn current_background(&self) -> String {
        self.background_preference
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_else(|_| "dark".to_string())
    }
}
