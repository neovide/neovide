use std::sync::{Arc, LazyLock, Mutex};

use anyhow::{Context, Result};
use indoc::indoc;
use log::trace;
use nvim_rs::{call_args, error::CallError, rpc::model::IntoVal, Neovim, Value};
use strum::AsRefStr;
use tokio::sync::mpsc::unbounded_channel;

use super::{show_error_message, NeovimHandler, Settings};
use crate::{
    bridge::{nvim_dict, NeovimWriter},
    cmd_line::CmdLineSettings,
    profiling::{tracy_dynamic_zone, tracy_fiber_enter, tracy_fiber_leave},
    utils::handle_wslpaths,
};

pub static HANDLER_REGISTRY: LazyLock<Mutex<Option<NeovimHandler>>> =
    LazyLock::new(|| Mutex::new(None));

// Serial commands are any commands which must complete before the next value is sent. This
// includes keyboard and mouse input which would cause problems if sent out of order.
//
// When in doubt, use Parallel Commands.
#[derive(Clone, Debug, AsRefStr)]
pub enum SerialCommand {
    Keyboard(String),
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
}

impl SerialCommand {
    async fn execute(self, nvim: &Neovim<NeovimWriter>) {
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
    SetBackground(String),
    ShowError { lines: Vec<String> },
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
            ParallelCommand::SetBackground(background) => nvim
                .set_option_value("background", Value::from(background), nvim_dict! {})
                .await
                .context("SetBackground failed"),
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
    handler: NeovimHandler,
    nvim: Neovim<NeovimWriter>,
    settings: Arc<Settings>,
) {
    let (serial_tx, mut serial_rx) = unbounded_channel::<SerialCommand>();
    let ui_command_nvim = nvim.clone();
    let (_ui_command_sender, mut ui_command_receiver) = handler.get_ui_command_channel();
    HANDLER_REGISTRY.lock().unwrap().replace(handler);
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
                    let ui_command_nvim = ui_command_nvim.clone();
                    let settings = settings.clone();
                    tokio::spawn(async move {
                        parallel_command
                            .execute(&ui_command_nvim, settings.as_ref())
                            .await;
                    });
                }
                None => break,
            }
        }
        log::info!("ui command receiver finished");
    });

    tokio::spawn(async move {
        tracy_fiber_enter!("Serial command");
        loop {
            tracy_fiber_leave();
            let res = serial_rx.recv().await;
            tracy_fiber_enter!("Serial command");
            match res {
                Some(serial_command) => {
                    tracy_dynamic_zone!(serial_command.as_ref());
                    tracy_fiber_leave();
                    serial_command.execute(&nvim).await;
                    tracy_fiber_enter!("Serial command");
                }
                None => break,
            }
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
        .expect("2.The UI command channel has not been initialized");
}
