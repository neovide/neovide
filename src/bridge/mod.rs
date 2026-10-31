//! Starts neovim sessions and connects them to window routes. The window
//! wrapper asks `NeovimRuntime` to launch or replace a session, while
//! `NeovimHandler` turns RPC notifications into editor and window events.
//! A replacement reuses the handler for the route after the old session
//! reports exit.

mod api_info;
mod clipboard;
mod command;
mod events;
mod handler;
pub mod session;
mod setup;
mod transition;
mod ui_commands;

use std::io::Error;
use std::ops::Add;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
pub use command::OpenArgs;
pub use command::OpenMode;
pub use command::create_blocking_nvim_command;
#[cfg(test)]
pub use command::create_tokio_nvim_command;
pub use events::*;
use futures::StreamExt;
pub use handler::NeovimHandler;
use itertools::Itertools;
use log::info;
use mundy::Interest;
use mundy::Preferences;
use nvim_rs::Neovim;
use nvim_rs::UiAttachOptions;
use nvim_rs::Value;
use nvim_rs::call_args;
use nvim_rs::error::CallError;
use nvim_rs::rpc::model::IntoVal;
use rmpv::Utf8String;
use session::NeovimInstance;
use session::NeovimSession;
pub use session::NeovimWriter;
use setup::get_api_information;
use setup::setup_neovide_specific_state;
use tokio::runtime::Builder;
use tokio::runtime::Runtime;
use tokio::select;
use tokio::time::timeout;
pub use transition::RestartDetails;
pub use transition::SessionTransition;
pub use transition::TransitionFailure;
pub use ui_commands::ParallelCommand;
pub use ui_commands::SerialCommand;
pub use ui_commands::get_active_handler;
#[cfg(target_os = "macos")]
pub use ui_commands::require_active_handler;
#[cfg(target_os = "macos")]
pub use ui_commands::send_or_queue_file_drop;
pub use ui_commands::send_ui;
pub use ui_commands::set_active_route_handler;
pub use ui_commands::start_ui_command_handler;
pub use ui_commands::unregister_route_handler;
use winit::event_loop::EventLoopProxy;

use crate::clipboard::ClipboardHandle;
use crate::cmd_line::CmdLineSettings;
use crate::editor::start_editor_handler;
use crate::running_tracker::RunningTracker;
use crate::settings::*;
use crate::units::GridSize;
use crate::window::EventPayload;
use crate::window::RouteId;
use crate::window::UserEvent;
use crate::window::WindowSettings;

const NEOVIM_REQUIRED_VERSION: (u64, u64, u64) = (0, 12, 0);

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

/// Tokio runtime for neovim RPC loops and sessions for each route. The window
/// wrapper owns this runtime. Each launch or transition starts `run`, which
/// reports the session exit by `RouteId` to the window event loop.
pub struct NeovimRuntime {
    pub runtime: Option<Runtime>,
    clipboard: ClipboardHandle,
    background_preference: Arc<Mutex<String>>,
}

/// Choose the source of the next neovim session for a route. `Startup` uses
/// `--server` when it is set. A transition uses the address sent by neovim
/// and can restore restart channel properties. `FreshEmbedded` starts a new
/// process even when `--server` is set, so a failed connect can keep the route
/// usable after the previous server stops, if the connect request reaches the UI.
#[derive(Clone, Copy)]
pub enum SessionTarget<'a> {
    Startup,
    Transition(&'a SessionTransition),
    FreshEmbedded,
}

async fn neovim_instance(
    settings: &Settings,
    target: SessionTarget<'_>,
    cwd: Option<&Path>,
    mode: OpenMode,
) -> Result<NeovimInstance> {
    let address = match target {
        SessionTarget::Startup => settings.get::<CmdLineSettings>().server,
        SessionTarget::Transition(transition) => Some(transition.address().to_owned()),
        SessionTarget::FreshEmbedded => None,
    };

    if let Some(address) = address {
        return Ok(NeovimInstance::Server { address });
    }

    let cmdline_settings = settings.get::<CmdLineSettings>();
    Ok(NeovimInstance::Embedded(command::create_tokio_nvim_command(
        &cmdline_settings,
        true,
        cwd,
        mode,
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

    let opts: Vec<(Value, Value)> = nvim_dict! {};
    nvim.call("nvim_echo", call_args![prepared_lines, true, opts]).await??;

    Ok(())
}

/// Show one highlighted error line in a recovered session. Unlike the
/// multiline error helper, this does not add a trailing newline.
pub async fn show_error_status(
    nvim: &Neovim<NeovimWriter>,
    message: &str,
) -> Result<(), Box<CallError>> {
    let content = vec![Value::Array(vec![Value::from(message), Value::from("ErrorMsg")])];
    let options: Vec<(Value, Value)> = nvim_dict! {};
    nvim.call("nvim_echo", call_args![content, true, options]).await??;
    Ok(())
}

pub async fn show_startup_message(
    nvim: &Neovim<NeovimWriter>,
    message: &StartupMessage,
) -> Result<(), Box<CallError>> {
    let mut chunk = vec![Value::String(message.content.clone().add("\n").into())];
    if let Some(highlight_group) = message.kind.highlight_group() {
        chunk.push(Value::String(highlight_group.into()));
    }

    let mut opts: Vec<(Value, Value)> = nvim_dict! {};
    if message.kind.is_error() {
        opts.push((Value::from("err"), Value::from(true)));
    }

    // show the captured message without duplicating nvim message history.
    nvim.call("nvim_echo", call_args![vec![Value::Array(chunk)], false, opts]).await??;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn create_neovim_session(
    route_id: RouteId,
    handler: NeovimHandler,
    grid_size: Option<GridSize<u32>>,
    settings: Arc<Settings>,
    background: &str,
    target: SessionTarget<'_>,
    cwd: Option<&Path>,
    mode: OpenMode,
) -> Result<NeovimSession> {
    let neovim_instance = neovim_instance(settings.as_ref(), target, cwd, mode).await?;
    let explicit_server_address = match &neovim_instance {
        NeovimInstance::Server { address } => Some(address.clone()),
        NeovimInstance::Embedded(_) => None,
    };
    let is_wsl = settings.get::<CmdLineSettings>().wsl;
    // `:connect` can change the transport without changing startup settings.
    // Setup uses the transport selected for this session.
    let remote = matches!(neovim_instance, NeovimInstance::Server { .. }) || is_wsl;
    let channel_config = match target {
        SessionTarget::Transition(transition) => transition.channel_config(),
        SessionTarget::Startup | SessionTarget::FreshEmbedded => None,
    };

    #[allow(unused_mut)]
    let mut session = NeovimSession::new(neovim_instance, handler.clone(), channel_config)
        .await
        .context("Could not establish Neovim session")?;

    let api_information = get_api_information(&session.neovim).await?;
    info!("Neovide registered to nvim with channel id {}", api_information.channel);

    let server_address = match explicit_server_address {
        Some(address) => Some(address),
        None if is_wsl => None,
        None => session.neovim.get_vvar("servername").await.ok().and_then(|value| {
            value.as_str().filter(|address| !address.is_empty()).map(str::to_owned)
        }),
    };

    handler.update_server_address(server_address);

    let (major, minor, patch) = NEOVIM_REQUIRED_VERSION;
    if !api_information.version.has_version(major, minor, patch, Some(0)) {
        let found = api_information.version.string;
        bail!(
            "Neovide requires nvim version {major}.{minor}.{patch} or higher, but {found} was detected. Download the latest version here https://github.com/neovim/neovim/wiki/Installing-Neovim"
        );
    }

    let cmdline_settings = settings.get::<CmdLineSettings>();

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
        route_id,
        handler.clone(),
        session.neovim.clone(),
        settings.clone(),
        can_support_ime_api,
    );
    settings.read_initial_values(&session.neovim).await?;
    set_background_if_allowed(background, &session.neovim).await;

    let capture_startup_messages = cmdline_settings.startup_message_capture;
    if capture_startup_messages {
        let pre_attach_cmdheight = session
            .neovim
            .get_option("cmdheight")
            .await
            .context("Read pre-attach cmdheight failed")?;
        handler.set_pre_attach_cmdheight(pre_attach_cmdheight);
        handler.send_redraw_event(RedrawEvent::NeovimSessionStarted);
    }

    let mut options = UiAttachOptions::new();
    options.set_linegrid_external(true);
    options.set_multigrid_external(!cmdline_settings.no_multi_grid);
    options.set_rgb(true);
    if capture_startup_messages {
        // Temporarily externalize messages so startup errors before the first grid update are not
        // lost behind a hit-enter prompt. After the first rendered batch, we restore nvim's
        // message/cmdline screen space and replay the captured messages.
        // See https://github.com/neovide/neovide/issues/3499
        options.set_messages_external(true);
    }

    #[cfg(target_os = "macos")]
    options.set_hlstate_external(true);

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
    session
        .neovim
        .ui_attach(grid_size.width as i64, grid_size.height as i64, &options)
        .await
        .context("Could not attach ui to neovim process")?;

    #[cfg(target_os = "macos")]
    ui_commands::mark_file_drop_handler_ready();

    info!("Neovim process attached");

    Ok(session)
}

/// Report a route exit when its RPC loop ends or its embedded child exits. A
/// server connection has no child to watch. If RPC ends first, as with an
/// ordinary `:connect`, dropping the join handle for the supervisor leaves its
/// task waiting for the old child while the runtime lives. If the child exits
/// first, allow RPC IO 500 ms to finish before reporting exit even if that
/// stream remains open.
async fn run(route_id: RouteId, session: NeovimSession, proxy: EventLoopProxy<EventPayload>) {
    let mut session = session;

    if let Some(mut process_task) = session.supervise_process() {
        // An exited child can leave RPC IO pending (for example,
        // https://github.com/neovim/neovim/issues/26743).
        select! {
            _ = &mut session.io_handle => {}
            _ = &mut process_task => {
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

    proxy.send_event(EventPayload::for_route(UserEvent::NeovimExited, route_id)).ok();
}

pub async fn set_background_if_allowed(background: &str, neovim: &Neovim<NeovimWriter>) {
    // Unfortunately neovim does not set the last_set_chan for options when they are set through
    // exec_lua. The last_set_sid is also generic, so we are forced to do two calls.
    if let Ok(can_set) = neovim
        .exec_lua("return neovide.private.can_set_background()", vec![background.into()])
        .await
        && can_set.as_bool().unwrap()
    {
        let _ = neovim.set_option("background", background.into()).await;
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
        Ok(Some(preferences)) => {
            background_from_preferences(&preferences).unwrap_or("dark").to_string()
        }
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
                ParallelCommand::SetBackground { background: background.to_string() },
                &handler,
            );
        }
    }
}

impl NeovimRuntime {
    pub fn new(clipboard: ClipboardHandle) -> Result<Self, Error> {
        let runtime = Builder::new_multi_thread().enable_all().build()?;

        Ok(Self {
            runtime: Some(runtime),
            clipboard,
            background_preference: Arc::new(Mutex::new("dark".to_string())),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn launch(
        &mut self,
        route_id: RouteId,
        event_loop_proxy: EventLoopProxy<EventPayload>,
        grid_size: Option<GridSize<u32>>,
        running_tracker: RunningTracker,
        settings: Arc<Settings>,
        config: &Config,
        cwd: Option<&Path>,
        mode: OpenMode,
    ) -> Result<NeovimHandler> {
        let mut colorscheme_stream = self.colorscheme_stream();
        let editor_handler = start_editor_handler(
            route_id,
            event_loop_proxy.clone(),
            running_tracker,
            settings.clone(),
            self.clipboard.clone(),
        );
        let initial_background =
            self.runtime().block_on(initial_background_from_stream(&mut colorscheme_stream));
        self.set_background_preference(&initial_background);

        let mut font_config_state = settings.get::<FontConfigState>();
        font_config_state.has_font = config.font.is_some();
        settings.set(&font_config_state);

        let session = match self.runtime().block_on(create_neovim_session(
            route_id,
            editor_handler.clone(),
            grid_size,
            settings,
            &initial_background,
            SessionTarget::Startup,
            cwd,
            mode,
        )) {
            Ok(session) => session,
            Err(err) => {
                self.runtime().block_on(async move {
                    drop(colorscheme_stream);
                });
                return Err(err);
            }
        };

        self.runtime().spawn(update_colorscheme(
            colorscheme_stream,
            self.background_preference.clone(),
            editor_handler.clone(),
        ));

        self.runtime().spawn(run(route_id, session, event_loop_proxy));

        Ok(editor_handler)
    }

    pub fn shutdown(mut self, timeout: Duration) { self.shutdown_timeout(timeout); }

    /// Attach a replacement session to an existing route handler. The window
    /// wrapper calls this after the old session reports exit. A transition
    /// uses the address sent by Neovim. `FreshEmbedded` starts a new process
    /// when no server connection remains usable. Only restart restores channel
    /// properties before `nvim_ui_attach`. The route keeps its editor thread.
    /// Connection and setup errors return to the wrapper for route cleanup
    /// and presentation by the application.
    #[allow(clippy::too_many_arguments)]
    pub fn replace_session(
        &mut self,
        route_id: RouteId,
        event_loop_proxy: EventLoopProxy<EventPayload>,
        handler: NeovimHandler,
        grid_size: GridSize<u32>,
        settings: Arc<Settings>,
        target: SessionTarget<'_>,
        cwd: Option<&Path>,
    ) -> Result<()> {
        let background = self.current_background();
        let session = self.runtime().block_on(create_neovim_session(
            route_id,
            handler,
            Some(grid_size),
            settings,
            &background,
            target,
            cwd,
            OpenMode::None,
        ))?;

        self.runtime().spawn(run(route_id, session, event_loop_proxy));

        Ok(())
    }

    fn runtime(&self) -> &Runtime {
        self.runtime.as_ref().expect("runtime must be available while NeovimRuntime is alive")
    }

    pub fn shutdown_timeout(&mut self, timeout: Duration) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_timeout(timeout);
        }
    }

    pub fn colorscheme_stream(&self) -> mundy::PreferencesStream {
        let _guard = self.runtime().enter();
        Preferences::stream(Interest::ColorScheme)
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

impl Drop for NeovimRuntime {
    fn drop(&mut self) {
        // on the happy path it is redundant but intentionally defensive.
        // application Drop is the normal path and this one is the fallback
        // for safety unexpected teardow paths. It's also not double down
        // since the application shutdown consumes the inner tokio runtime
        self.shutdown_timeout(Duration::from_millis(500));
    }
}

#[cfg(test)]
mod tests {
    use super::SessionTarget;
    use super::neovim_instance;
    use crate::bridge::OpenMode;
    use crate::bridge::session::NeovimInstance;
    use crate::cmd_line::CmdLineSettings;
    use crate::settings::Settings;

    #[tokio::test]
    async fn fresh_embedded_ignores_startup_server() {
        let settings = Settings::new();
        let mut cmdline = CmdLineSettings::default();
        cmdline.server = Some("/tmp/old-server.sock".to_owned());
        settings.set(&cmdline);

        let startup =
            neovim_instance(&settings, SessionTarget::Startup, None, OpenMode::None).await.unwrap();
        assert!(matches!(startup, NeovimInstance::Server { .. }));

        let fallback =
            neovim_instance(&settings, SessionTarget::FreshEmbedded, None, OpenMode::None)
                .await
                .unwrap();

        assert!(matches!(fallback, NeovimInstance::Embedded(_)));
    }
}
