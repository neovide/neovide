mod api_info;
mod clipboard;
mod command;
mod events;
mod handler;
pub mod session;
mod setup;
mod ui_commands;

use std::{io::Error, ops::Add, sync::Arc, time::Duration};

use anyhow::{bail, Context, Result};
use futures::StreamExt;
use itertools::Itertools;
use log::info;
use nvim_rs::{error::CallError, Neovim, UiAttachOptions, Value};
use rmpv::Utf8String;
use tokio::{
    runtime::{Builder, Runtime},
    select,
    time::timeout,
};
use winit::event_loop::EventLoopProxy;

use crate::{
    clipboard::ClipboardHandle,
    cmd_line::CmdLineSettings,
    editor::start_editor,
    running_tracker::RunningTracker,
    settings::*,
    units::GridSize,
    window::{UserEvent, WindowSettings},
};
pub use handler::NeovimHandler;
use session::{NeovimInstance, NeovimSession};
use setup::{get_api_information, setup_neovide_specific_state};

pub use command::{create_blocking_nvim_command, create_nvim_command};
pub use events::*;
pub use session::NeovimWriter;
pub use ui_commands::{send_ui, start_ui_command_handler, ParallelCommand, SerialCommand};

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
}

async fn neovim_instance(settings: &Settings) -> Result<NeovimInstance> {
    if let Some(address) = settings.get::<CmdLineSettings>().server {
        Ok(NeovimInstance::Server { address })
    } else {
        let cmd = create_nvim_command(settings);
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
    nvim.echo(prepared_lines, true, nvim_dict! {}).await
}

async fn launch(
    handler: NeovimHandler,
    grid_size: Option<GridSize<u32>>,
    settings: Arc<Settings>,
    colorscheme_stream: &mut mundy::PreferencesStream,
) -> Result<NeovimSession> {
    let neovim_instance = neovim_instance(settings.as_ref()).await?;
    #[allow(unused_mut)]
    let mut session = NeovimSession::new(neovim_instance, handler)
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

    let remote = cmdline_settings.wsl || cmdline_settings.server.is_some();
    // This is too verbose to keep enabled all the time
    // log::info!("Api information {:#?}", api_information);
    setup_neovide_specific_state(&session.neovim, remote, &api_information, &settings).await?;
    if api_information.version.has_version(0, 12, 0, Some(1264)) {
        let mut window_settings = settings.get::<WindowSettings>();
        window_settings.has_mouse_grid_detection = true;
        settings.set::<WindowSettings>(&window_settings);
    }

    // NOTE: `api_information.has_function("nvim__exec_lua_fast")` does not return the correct
    // result so we check the version instead.
    let can_support_ime_api = api_information.version.has_version(0, 12, 0, Some(1724));

    start_ui_command_handler(
        session.neovim.clone(),
        settings.clone(),
        can_support_ime_api,
    );
    settings.read_initial_values(&session.neovim).await?;

    let colorscheme = timeout(Duration::from_millis(200), colorscheme_stream.next()).await;
    let background = match colorscheme {
        Ok(Some(preferences)) => match preferences.color_scheme {
            mundy::ColorScheme::Dark => "dark",
            mundy::ColorScheme::Light => "light",
            mundy::ColorScheme::NoPreference => "dark",
        },
        Ok(None) => "dark",
        Err(..) => "dark",
    };
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
    proxy.send_event(UserEvent::NeovimExited).ok();
}

async fn set_background_if_allowed(background: &str, neovim: &Neovim<NeovimWriter>) {
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

async fn update_colorscheme(mut stream: mundy::PreferencesStream, neovim: Neovim<NeovimWriter>) {
    while let Some(preferences) = stream.next().await {
        if let Some(background) = match preferences.color_scheme {
            mundy::ColorScheme::Dark => Some("dark"),
            mundy::ColorScheme::Light => Some("light"),
            // At least KDE Plasma sends this after sending the actual color scheme
            // So do nothing
            mundy::ColorScheme::NoPreference => None,
        } {
            set_background_if_allowed(background, &neovim).await;
        }
    }
}

impl NeovimRuntime {
    pub fn new(clipboard: ClipboardHandle) -> Result<Self, Error> {
        let runtime = Builder::new_multi_thread().enable_all().build()?;

        Ok(Self { runtime, clipboard })
    }

    pub fn launch(
        &mut self,
        event_loop_proxy: EventLoopProxy<UserEvent>,
        grid_size: Option<GridSize<u32>>,
        running_tracker: RunningTracker,
        settings: Arc<Settings>,
        mut colorscheme_stream: mundy::PreferencesStream,
    ) -> Result<()> {
        let handler = start_editor(
            event_loop_proxy.clone(),
            running_tracker,
            settings.clone(),
            self.clipboard.clone(),
        );
        let session = self.runtime.block_on(launch(
            handler,
            grid_size,
            settings,
            &mut colorscheme_stream,
        ))?;
        self.runtime.spawn(update_colorscheme(
            colorscheme_stream,
            session.neovim.clone(),
        ));
        self.runtime.spawn(run(session, event_loop_proxy));
        Ok(())
    }
}
