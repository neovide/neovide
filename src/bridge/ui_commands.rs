use std::sync::{Arc, OnceLock, RwLock};

use anyhow::{Context, Result};
use indoc::indoc;
use log::trace;
use nvim_rs::{call_args, error::CallError, rpc::model::IntoVal, Neovim};
use rmpv::Value;
use strum::AsRefStr;
use tokio::sync::mpsc::unbounded_channel;

use super::{set_background_if_allowed, show_error_message, Settings};
use crate::{
    bridge::{nvim_dict, NeovimWriter},
    cmd_line::CmdLineSettings,
    profiling::{tracy_dynamic_zone, tracy_fiber_enter, tracy_fiber_leave},
    utils::handle_wslpaths,
    LoggingSender,
};

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

#[derive(Default)]
struct NeovimState {
    nvim: Option<Neovim<NeovimWriter>>,
    can_support_ime_api: bool,
}

static CURRENT_NEOVIM: OnceLock<Arc<RwLock<NeovimState>>> = OnceLock::new();
static UI_COMMAND_CHANNEL: OnceLock<LoggingSender<UiCommand>> = OnceLock::new();

fn neovim_holder() -> &'static Arc<RwLock<NeovimState>> {
    CURRENT_NEOVIM.get_or_init(|| Arc::new(RwLock::new(NeovimState::default())))
}

fn update_current_neovim(nvim: Neovim<NeovimWriter>, can_support_ime_api: bool) {
    let holder = neovim_holder();
    if let Ok(mut guard) = holder.write() {
        guard.nvim = Some(nvim);
        guard.can_support_ime_api = can_support_ime_api;
    }
}

fn clone_neovim(holder: &Arc<RwLock<NeovimState>>) -> Option<Neovim<NeovimWriter>> {
    holder
        .read()
        .ok()
        .and_then(|guard| guard.nvim.as_ref().cloned())
}

fn clone_neovim_with_ime(
    holder: &Arc<RwLock<NeovimState>>,
) -> Option<(Neovim<NeovimWriter>, bool)> {
    holder.read().ok().and_then(|guard| {
        guard
            .nvim
            .as_ref()
            .cloned()
            .map(|nvim| (nvim, guard.can_support_ime_api))
    })
}

pub fn start_ui_command_handler(
    nvim: Neovim<NeovimWriter>,
    settings: Arc<Settings>,
    can_support_ime_api: bool,
) {
    update_current_neovim(nvim, can_support_ime_api);
    if UI_COMMAND_CHANNEL.get().is_some() {
        return;
    }

    let neovim_holder = neovim_holder().clone();
    let (serial_tx, mut serial_rx) = unbounded_channel::<SerialCommand>();
    let (sender, mut ui_command_receiver) = unbounded_channel();
    UI_COMMAND_CHANNEL
        .set(LoggingSender::attach(sender, "UIComand"))
        .expect("The UI command channel is already created");
    tokio::spawn({
        let neovim_holder = neovim_holder.clone();
        let settings = settings.clone();
        async move {
            loop {
                match ui_command_receiver.recv().await {
                    Some(UiCommand::Serial(serial_command)) => {
                        tracy_dynamic_zone!(serial_command.as_ref());
                        // This can fail if the serial_rx loop exits before this one, so ignore the errors
                        let _ = serial_tx.send(serial_command);
                    }
                    Some(UiCommand::Parallel(parallel_command)) => {
                        tracy_dynamic_zone!(parallel_command.as_ref());
                        let neovim_holder = neovim_holder.clone();
                        let settings = settings.clone();
                        tokio::spawn(async move {
                            if let Some(ui_command_nvim) = clone_neovim(&neovim_holder) {
                                parallel_command
                                    .execute(&ui_command_nvim, settings.as_ref())
                                    .await;
                            }
                        });
                    }
                    None => break,
                }
            }
            log::info!("ui command receiver finished");
        }
    });

    tokio::spawn({
        let neovim_holder = neovim_holder.clone();
        async move {
            tracy_fiber_enter!("Serial command");
            while let Some(serial_command) = serial_rx.recv().await {
                tracy_dynamic_zone!(serial_command.as_ref());
                tracy_fiber_leave();
                match clone_neovim_with_ime(&neovim_holder) {
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
        }
    });
}

pub fn send_ui<T>(command: T)
where
    T: Into<UiCommand>,
{
    let command: UiCommand = command.into();
    let _ = UI_COMMAND_CHANNEL
        .get()
        .expect("The UI command channel has not been initialized")
        .send(command);
}
