use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex},
};

use anyhow::{Context, Result};
use indoc::indoc;
use log::trace;
use nvim_rs::{call_args, error::CallError, rpc::model::IntoVal, Neovim};
use rmpv::Value;
use strum::AsRefStr;
use tokio::sync::mpsc::unbounded_channel;

use super::{set_background_if_allowed, show_error_message, NeovimHandler, Settings};
use crate::{
    bridge::{nvim_dict, NeovimWriter},
    cmd_line::CmdLineSettings,
    profiling::{tracy_dynamic_zone, tracy_fiber_enter, tracy_fiber_leave},
    utils::handle_wslpaths,
    window::RouteId,
};

/// Active handler pointer for places that do not carry a RouteId
/// like global menu callbacks and a few legacy paths
/// this can be None during startup or shutdown
pub static HANDLER_REGISTRY: LazyLock<Mutex<Option<NeovimHandler>>> =
    LazyLock::new(|| Mutex::new(None));

/// Route aware handler map keyed by RouteId
/// this is the source of truth for the multi window routing.
/// we keep HANDLER_REGISTRY in sync with this so older or route agnostic
/// call sites can still send ui commands without carrying route context
pub static ROUTE_HANDLER_REGISTRY: LazyLock<Mutex<HashMap<RouteId, NeovimHandler>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn get_active_handler() -> Option<NeovimHandler> {
    HANDLER_REGISTRY.lock().unwrap().clone()
}

pub fn require_active_handler() -> NeovimHandler {
    get_active_handler().expect("NeovimHandler has not been initialized")
}

async fn ime_call(
    nvim: &Neovim<NeovimWriter>,
    func: &str,
    args: Vec<Value>,
    context: &'static str,
    trace_msg: &'static str,
) -> Result<()> {
    nvim.call("nvim__exec_lua_fast", call_args![func, args])
        .await
        .map(|_| trace!("{trace_msg}"))
        .context(context)
}

// Serial commands are any commands which must complete before the next value is sent. This
// includes keyboard and mouse input which would cause problems if sent out of order.
//
// When in doubt, use Parallel Commands.
#[derive(Clone, Debug, AsRefStr)]
pub enum SerialCommand {
    Keyboard(String),
    KeyboardImeCommit {
        formatted: String,
        raw: String,
    },
    KeyboardImePreedit {
        raw: String,
        cursor_offset: Option<(usize, usize)>,
    },
    MouseButton {
        button: String,
        action: String,
        grid_id: u64,
        position: (u32, u32),
        modifier_string: String,
    },
    Scroll {
        direction: String,
        grid_id: u64,
        position: (u32, u32),
        modifier_string: String,
    },
    Drag {
        button: String,
        grid_id: u64,
        position: (u32, u32),
        modifier_string: String,
    },
    #[cfg(target_os = "macos")]
    ForceClickCommand,
}

impl SerialCommand {
    async fn execute(self, nvim: &Neovim<NeovimWriter>, can_support_ime_api: bool) {
        // Don't panic here unless there's absolutely no chance of continuing the program, Instead
        // just log the error and hope that it's something temporary or recoverable A normal reason
        // for failure is when neovim has already quit, and a command, for example mouse move is
        // being sent
        log::trace!("In Serial Command");
        let result = match self {
            SerialCommand::Keyboard(input_command) => {
                trace!("Keyboard Input Sent: {input_command}");
                nvim.input(&input_command)
                    .await
                    .map(|_| ())
                    .context("Input failed")
            }
            SerialCommand::KeyboardImeCommit { formatted, raw } => {
                // Notified ime commit event, the text is guaranteed not to be None.
                trace!("IME Input Sent: {formatted}");
                if can_support_ime_api {
                    ime_call(
                        nvim,
                        "neovide.commit_handler(...)",
                        vec![Value::from(raw), Value::from(formatted)],
                        "IME Commit failed",
                        "IME Commit Called",
                    )
                    .await
                } else {
                    trace!("Keyboard Input Sent: {formatted}");
                    nvim.input(&formatted)
                        .await
                        .map(|_| ())
                        .context("Input failed")
                }
            }
            SerialCommand::KeyboardImePreedit { raw, cursor_offset } => {
                trace!("IME Input Preedit");
                if can_support_ime_api {
                    let (start_col, end_col) = cursor_offset
                        .map_or((Value::Nil, Value::Nil), |(start, end)| {
                            (Value::from(start), Value::from(end))
                        });

                    ime_call(
                        nvim,
                        "neovide.preedit_handler(...)",
                        vec![Value::from(raw), start_col, end_col],
                        "IME Preedit failed",
                        "IME Preedit Called",
                    )
                    .await
                } else {
                    Ok(())
                }
            }
            SerialCommand::MouseButton {
                button,
                action,
                grid_id,
                position: (grid_x, grid_y),
                modifier_string,
            } => nvim
                .input_mouse(
                    &button,
                    &action,
                    &modifier_string,
                    grid_id as i64,
                    grid_y as i64,
                    grid_x as i64,
                )
                .await
                .context("Mouse input failed"),
            SerialCommand::Scroll {
                direction,
                grid_id,
                position: (grid_x, grid_y),
                modifier_string,
            } => nvim
                .input_mouse(
                    "wheel",
                    &direction,
                    &modifier_string,
                    grid_id as i64,
                    grid_y as i64,
                    grid_x as i64,
                )
                .await
                .context("Mouse Scroll Failed"),
            SerialCommand::Drag {
                button,
                grid_id,
                position: (grid_x, grid_y),
                modifier_string,
            } => nvim
                .input_mouse(
                    &button,
                    "drag",
                    &modifier_string,
                    grid_id as i64,
                    grid_y as i64,
                    grid_x as i64,
                )
                .await
                .context("Mouse Drag Failed"),
            #[cfg(target_os = "macos")]
            SerialCommand::ForceClickCommand => nvim
                .command("NeovideForceClick")
                .await
                .context("Force click command failed"),
        };

        if let Err(error) = result {
            log::error!("{error:?}");
        }
    }
}

#[derive(Debug, Clone, AsRefStr)]
pub enum ParallelCommand {
    Quit,
    Resize { width: u64, height: u64 },
    FileDrop(String),
    FocusLost,
    FocusGained,
    DisplayAvailableFonts(Vec<String>),
    ShowError { lines: Vec<String> },
    SetBackground { background: String },
}

async fn display_available_fonts(
    nvim: &Neovim<NeovimWriter>,
    fonts: Vec<String>,
) -> Result<(), Box<CallError>> {
    let mut content: Vec<String> = vec![
        "What follows are the font names available for guifont. You can try any of them with <CR> in normal mode.",
        "",
        "To switch to one of them, use one of them, type:",
        "",
        "    :set guifont=<font name>:h<font size>",
        "",
        "where <font name> is one of the following with spaces escaped",
        "and <font size> is the desired font size. As an example:",
        "",
        "    :set guifont=Cascadia\\ Code\\ PL:h12",
        "",
        "You may specify multiple fonts for fallback purposes separated by commas like so:",
        "",
        "    :set guifont=Cascadia\\ Code\\ PL,Delugia\\ Nerd\\ Font:h12",
        "",
        "Make sure to add the above command when you're happy with it to your .vimrc file or similar config to make it permanent.",
        "------------------------------",
        "Available Fonts on this System",
        "------------------------------",
    ].into_iter().map(|text| text.to_owned()).collect();
    content.extend(fonts);

    nvim.exec2(
        indoc! {"
            split
            noswapfile hide enew
            setlocal buftype=nofile
            setlocal bufhidden=hide
            file scratch
            nnoremap <buffer> <CR> <cmd>lua vim.opt.guifont=vim.fn.getline('.')<CR>,
        "},
        nvim_dict! {},
    )
    .await?;
    let _ = nvim
        .call(
            "nvim_buf_set_lines",
            call_args![0i64, 0i64, -1i64, false, content],
        )
        .await?;
    Ok(())
}

impl ParallelCommand {
    async fn execute(self, nvim: &Neovim<NeovimWriter>, settings: &Settings) {
        // Don't panic here unless there's absolutely no chance of continuing the program, Instead
        // just log the error and hope that it's something temporary or recoverable A normal reason
        // for failure is when neovim has already quit, and a command, for example mouse move is
        // being sent
        let result = match self {
            ParallelCommand::Quit => {
                // Ignore all errors, since neovim exits immediately before the response is sent.
                // We could an RPC notify instead of request, but nvim-rs does currently not support it.
                let _ = nvim
                    .exec_lua(
                        include_str!("../../lua/exit_handler.lua"),
                        call_args![settings.get::<CmdLineSettings>().server.is_some()],
                    )
                    .await;
                Ok(())
            }
            ParallelCommand::Resize { width, height } => nvim
                .ui_try_resize(width.max(10) as i64, height.max(3) as i64)
                .await
                .context("Resize failed"),
            ParallelCommand::FocusLost => {
                nvim.ui_set_focus(false).await.context("FocusLost failed")
            }
            ParallelCommand::FocusGained => {
                nvim.ui_set_focus(true).await.context("FocusGained failed")
            }
            ParallelCommand::FileDrop(path) => nvim
                .exec_lua(
                    "neovide.private.dropfile(...)",
                    call_args![
                        handle_wslpaths(vec![path], settings.get::<CmdLineSettings>().wsl)
                            .first()
                            .unwrap()
                            .to_string(),
                        settings.get::<CmdLineSettings>().tabs
                    ],
                )
                .await
                .map(|_| ()) // We don't care about the result
                .context("FileDrop failed"),
            ParallelCommand::DisplayAvailableFonts(fonts) => display_available_fonts(nvim, fonts)
                .await
                .context("DisplayAvailableFonts failed"),
            ParallelCommand::ShowError { lines } => {
                // nvim.err_write(&message).await.ok();
                // NOTE: https://github.com/neovim/neovim/issues/5067
                // nvim_err_write[ln] is broken for multiline messages
                // We should go back to it whenever that bug gets fixed.
                show_error_message(nvim, &lines)
                    .await
                    .context("ShowError failed")
            }
            ParallelCommand::SetBackground { background } => {
                set_background_if_allowed(&background, nvim).await;
                Ok(())
            }
        };

        if let Err(error) = result {
            log::error!("{error:?}");
        }
    }
}

#[derive(Debug, Clone)]
pub enum UiCommand {
    Serial(SerialCommand),
    Parallel(ParallelCommand),
}

impl From<SerialCommand> for UiCommand {
    fn from(serial: SerialCommand) -> Self {
        UiCommand::Serial(serial)
    }
}

impl From<ParallelCommand> for UiCommand {
    fn from(parallel: ParallelCommand) -> Self {
        UiCommand::Parallel(parallel)
    }
}

impl AsRef<str> for UiCommand {
    fn as_ref(&self) -> &str {
        match self {
            UiCommand::Serial(cmd) => cmd.as_ref(),
            UiCommand::Parallel(cmd) => cmd.as_ref(),
        }
    }
}

pub fn start_ui_command_handler(
    route_id: RouteId,
    handler: NeovimHandler,
    nvim: Neovim<NeovimWriter>,
    settings: Arc<Settings>,
    can_support_ime_api: bool,
) {
    handler.update_current_neovim(nvim, can_support_ime_api);
    register_route_handler(route_id, handler.clone());
    if handler.mark_ui_command_started() {
        return;
    }

    let (serial_tx, mut serial_rx) = unbounded_channel::<SerialCommand>();
    let (_ui_command_sender, mut ui_command_receiver) = handler.get_ui_command_channel();

    let handler_for_parallel = handler.clone();
    let settings_for_parallel = settings.clone();
    tokio::spawn(async move {
        loop {
            match ui_command_receiver.recv().await {
                Some(UiCommand::Serial(serial_command)) => {
                    tracy_dynamic_zone!(serial_command.as_ref());
                    // This can fail if the serial_rx loop exits before this one, so ignore the errors
                    let _ = serial_tx.send(serial_command);
                }
                Some(UiCommand::Parallel(parallel_command)) => {
                    tracy_dynamic_zone!(parallel_command.as_ref());
                    let handler_for_command = handler_for_parallel.clone();
                    let settings = settings_for_parallel.clone();
                    tokio::spawn(async move {
                        if let Some(ui_command_nvim) = handler_for_command.clone_current_neovim() {
                            parallel_command
                                .execute(&ui_command_nvim, settings.as_ref())
                                .await;
                        } else {
                            log::warn!("Parallel command received without an active Neovim handle");
                        }
                    });
                }
                None => break,
            }
        }
        log::info!("ui command receiver finished");
    });

    let handler_for_serial = handler.clone();
    tokio::spawn(async move {
        tracy_fiber_enter!("Serial command");
        while let Some(serial_command) = serial_rx.recv().await {
            tracy_dynamic_zone!(serial_command.as_ref());
            tracy_fiber_leave();
            match handler_for_serial.clone_current_neovim_with_ime() {
                Some((serial_nvim, ime_api)) => {
                    serial_command.execute(&serial_nvim, ime_api).await;
                }
                None => {
                    log::warn!("Serial command received without an active Neovim handle");
                    break;
                }
            }
            tracy_fiber_enter!("Serial command");
        }
        log::info!("serial command receiver finished");
    });
}

pub fn send_ui<T>(command: T, handler: &NeovimHandler)
where
    T: Into<UiCommand>,
{
    let command: UiCommand = command.into();
    let sender = handler.get_ui_command_channel().0;
    sender
        .send(command)
        .expect("The UI command channel has not been initialized");
}

pub fn register_route_handler(route_id: RouteId, handler: NeovimHandler) {
    ROUTE_HANDLER_REGISTRY
        .lock()
        .unwrap()
        .insert(route_id, handler.clone());
    HANDLER_REGISTRY.lock().unwrap().replace(handler);
}

pub fn set_active_route_handler(route_id: RouteId) {
    if let Some(handler) = ROUTE_HANDLER_REGISTRY
        .lock()
        .unwrap()
        .get(&route_id)
        .cloned()
    {
        HANDLER_REGISTRY.lock().unwrap().replace(handler);
    }
}

pub fn unregister_route_handler(route_id: RouteId) {
    let mut by_route = ROUTE_HANDLER_REGISTRY.lock().unwrap();
    by_route.remove(&route_id);
    let replacement = by_route.values().next().cloned();
    drop(by_route);

    let mut active = HANDLER_REGISTRY.lock().unwrap();
    if let Some(handler) = replacement {
        active.replace(handler);
    } else {
        active.take();
    }
}
