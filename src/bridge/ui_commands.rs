use std::sync::Arc;

#[cfg(windows)]
use log::error;
use log::trace;

use nvim_rs::{call_args, rpc::model::IntoVal, Neovim};
use tokio::sync::mpsc::unbounded_channel;

#[cfg(windows)]
use crate::windows_utils::{
    register_rightclick_directory, register_rightclick_file, unregister_rightclick,
};

use super::show_intro_message;
use crate::{
    bridge::NeovimWriter, event_aggregator::EVENT_AGGREGATOR, running_tracker::RUNNING_TRACKER,
};

// Serial commands are any commands which must complete before the next value is sent. This
// includes keyboard and mouse input which would cause problems if sent out of order.
//
// When in doubt, use Parallel Commands.
#[derive(Clone, Debug)]
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
        match self {
            SerialCommand::Keyboard(input_command) => {
                trace!("Keyboard Input Sent: {}", input_command);
                nvim.input(&input_command).await.expect("Input failed");
            }
            SerialCommand::MouseButton {
                button,
                action,
                grid_id,
                position: (grid_x, grid_y),
                modifier_string,
            } => {
                nvim.input_mouse(
                    &button,
                    &action,
                    &modifier_string,
                    grid_id as i64,
                    grid_y as i64,
                    grid_x as i64,
                )
                .await
                .expect("Mouse Input Failed");
            }
            SerialCommand::Scroll {
                direction,
                grid_id,
                position: (grid_x, grid_y),
                modifier_string,
            } => {
                nvim.input_mouse(
                    "wheel",
                    &direction,
                    &modifier_string,
                    grid_id as i64,
                    grid_y as i64,
                    grid_x as i64,
                )
                .await
                .expect("Mouse Scroll Failed");
            }
            SerialCommand::Drag {
                button,
                grid_id,
                position: (grid_x, grid_y),
                modifier_string,
            } => {
                nvim.input_mouse(
                    &button,
                    "drag",
                    &modifier_string,
                    grid_id as i64,
                    grid_y as i64,
                    grid_x as i64,
                )
                .await
                .expect("Mouse Drag Failed");
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum ParallelCommand {
    Quit,
    Resize {
        width: u64,
        height: u64,
    },
    FileDrop(String),
    FocusLost,
    FocusGained,
    DisplayAvailableFonts(Vec<String>),
    SetBackground(String),
    #[cfg(windows)]
    RegisterRightClick,
    #[cfg(windows)]
    UnregisterRightClick,
    ShowIntro {
        message: Vec<String>,
    },
}

impl ParallelCommand {
    async fn execute(self, nvim: &Neovim<NeovimWriter>) {
        match self {
            ParallelCommand::Quit => {
                nvim.command(
                    "if get(g:, 'neovide_confirm_quit', 0) == 1 | confirm qa | else | qa! | endif",
                )
                .await
                .ok();
            }
            ParallelCommand::Resize { width, height } => nvim
                .ui_try_resize(width.max(10) as i64, height.max(3) as i64)
                .await
                .expect("Resize failed"),
            ParallelCommand::FocusLost => {
                nvim.ui_set_focus(false).await.expect("Focus Lost Failed")
            }
            ParallelCommand::FocusGained => {
                nvim.ui_set_focus(true).await.expect("Focus Gained Failed")
            }
            ParallelCommand::FileDrop(path) => {
                nvim.command(format!("e {path}").as_str()).await.ok();
            }
            ParallelCommand::SetBackground(background) => {
                nvim.command(format!("set background={}", background).as_str())
                    .await
                    .ok();
            }
            ParallelCommand::DisplayAvailableFonts(fonts) => {
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

                nvim.command("split").await.ok();
                nvim.command("noswapfile hide enew").await.ok();
                nvim.command("setlocal buftype=nofile").await.ok();
                nvim.command("setlocal bufhidden=hide").await.ok();
                nvim.command("\"setlocal nobuflisted").await.ok();
                nvim.command("\"lcd ~").await.ok();
                nvim.command("file scratch").await.ok();
                nvim.call(
                    "nvim_buf_set_lines",
                    call_args![0i64, 0i64, -1i64, false, content],
                )
                .await
                .ok();
                nvim.command(
                    "nnoremap <buffer> <CR> <cmd>lua vim.opt.guifont=vim.fn.getline('.')<CR>",
                )
                .await
                .ok();
            }
            #[cfg(windows)]
            ParallelCommand::RegisterRightClick => {
                if unregister_rightclick() {
                    let msg =
                        "Could not unregister previous menu item. Possibly already registered.";
                    nvim.err_writeln(msg).await.ok();
                    error!("{}", msg);
                }
                if !register_rightclick_directory() {
                    let msg = "Could not register directory context menu item. Possibly already registered.";
                    nvim.err_writeln(msg).await.ok();
                    error!("{}", msg);
                }
                if !register_rightclick_file() {
                    let msg =
                        "Could not register file context menu item. Possibly already registered.";
                    nvim.err_writeln(msg).await.ok();
                    error!("{}", msg);
                }
            }
            #[cfg(windows)]
            ParallelCommand::UnregisterRightClick => {
                if !unregister_rightclick() {
                    let msg = "Could not remove context menu items. Possibly already removed.";
                    nvim.err_writeln(msg).await.ok();
                    error!("{}", msg);
                }
            }
            ParallelCommand::ShowIntro { message } => {
                show_intro_message(nvim, &message).await.ok();
            }
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

pub fn start_ui_command_handler(nvim: Arc<Neovim<NeovimWriter>>) {
    let (serial_tx, mut serial_rx) = unbounded_channel::<SerialCommand>();
    let ui_command_nvim = nvim.clone();
    tokio::spawn(async move {
        let mut ui_command_receiver = EVENT_AGGREGATOR.register_event::<UiCommand>();
        while RUNNING_TRACKER.is_running() {
            match ui_command_receiver.recv().await {
                Some(UiCommand::Serial(serial_command)) => serial_tx
                    .send(serial_command)
                    .expect("Could not send serial ui command"),
                Some(UiCommand::Parallel(parallel_command)) => {
                    let ui_command_nvim = ui_command_nvim.clone();
                    tokio::spawn(async move {
                        parallel_command.execute(&ui_command_nvim).await;
                    });
                }
                None => {
                    RUNNING_TRACKER.quit("ui command channel failed");
                }
            }
        }
    });

    tokio::spawn(async move {
        while RUNNING_TRACKER.is_running() {
            match serial_rx.recv().await {
                Some(serial_command) => {
                    serial_command.execute(&nvim).await;
                }
                None => {
                    RUNNING_TRACKER.quit("serial ui command channel failed");
                }
            }
        }
    });
}
