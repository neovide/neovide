use std::sync::Arc;

#[cfg(windows)]
use log::error;
use log::trace;

use nvim_rs::Neovim;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

use crate::bridge::TxWrapper;
use crate::running_tracker::RUNNING_TRACKER;
#[cfg(windows)]
use crate::windows_utils::{
    register_rightclick_directory, register_rightclick_file, unregister_rightclick,
};

// Serial commands are any commands which must complete before the next value is sent. This
// includes keyboard and mouse input which would cuase problems if sent out of order.
#[derive(Debug, Clone)]
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
    async fn execute(self, nvim: &Neovim<TxWrapper>) {
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
    #[cfg(windows)]
    RegisterRightClick,
    #[cfg(windows)]
    UnregisterRightClick,
}

impl ParallelCommand {
    async fn execute(self, nvim: &Neovim<TxWrapper>) {
        match self {
            ParallelCommand::Quit => {
                nvim.command("qa!").await.ok();
            }
            ParallelCommand::Resize { width, height } => nvim
                .ui_try_resize(width.max(10) as i64, height.max(3) as i64)
                .await
                .expect("Resize failed"),
            ParallelCommand::FocusLost => nvim
                .command("if exists('#FocusLost') | doautocmd <nomodeline> FocusLost | endif")
                .await
                .expect("Focus Lost Failed"),
            ParallelCommand::FocusGained => nvim
                .command("if exists('#FocusGained') | doautocmd <nomodeline> FocusGained | endif")
                .await
                .expect("Focus Gained Failed"),
            ParallelCommand::FileDrop(path) => {
                nvim.command(format!("e {}", path).as_str()).await.ok();
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

pub fn start_ui_command_handler(
    mut ui_command_receiver: UnboundedReceiver<UiCommand>,
    nvim: Arc<Neovim<TxWrapper>>,
) {
    let (serial_tx, mut serial_rx) = unbounded_channel::<SerialCommand>();
    let ui_command_nvim = nvim.clone();
    tokio::spawn(async move {
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
