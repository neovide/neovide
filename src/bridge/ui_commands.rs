use log::trace;

#[cfg(windows)]
use log::error;

use nvim_rs::Neovim;

use crate::bridge::TxWrapper;

#[cfg(windows)]
use crate::windows_utils::{
    register_rightclick_directory, register_rightclick_file, unregister_rightclick,
};

#[derive(Debug, Clone)]
pub enum UiCommand {
    Resize {
        width: u32,
        height: u32,
    },
    Keyboard(String),
    MouseButton {
        action: String,
        grid_id: u64,
        position: (u32, u32),
    },
    Scroll {
        direction: String,
        grid_id: u64,
        position: (u32, u32),
    },
    Drag {
        grid_id: u64,
        position: (u32, u32),
    },
    FileDrop(String),
    FocusLost,
    FocusGained,
    #[cfg(windows)]
    RegisterRightClick,
    #[cfg(windows)]
    UnregisterRightClick,
}

impl UiCommand {
    pub async fn execute(self, nvim: &Neovim<TxWrapper>) {
        match self {
            UiCommand::Resize { width, height } => nvim
                .ui_try_resize(width.max(10) as i64, height.max(3) as i64)
                .await
                .expect("Resize failed"),
            UiCommand::Keyboard(input_command) => {
                trace!("Keyboard Input Sent: {}", input_command);
                nvim.input(&input_command).await.expect("Input failed");
            }
            UiCommand::MouseButton {
                action,
                grid_id,
                position: (grid_x, grid_y),
            } => {
                nvim.input_mouse(
                    "left",
                    &action,
                    "",
                    grid_id as i64,
                    grid_y as i64,
                    grid_x as i64,
                )
                .await
                .expect("Mouse Input Failed");
            }
            UiCommand::Scroll {
                direction,
                grid_id,
                position: (grid_x, grid_y),
            } => {
                nvim.input_mouse(
                    "wheel",
                    &direction,
                    "",
                    grid_id as i64,
                    grid_y as i64,
                    grid_x as i64,
                )
                .await
                .expect("Mouse Scroll Failed");
            }
            UiCommand::Drag {
                grid_id,
                position: (grid_x, grid_y),
            } => {
                nvim.input_mouse(
                    "left",
                    "drag",
                    "",
                    grid_id as i64,
                    grid_y as i64,
                    grid_x as i64,
                )
                .await
                .expect("Mouse Drag Failed");
            }
            UiCommand::FocusLost => nvim
                .command("if exists('#FocusLost') | doautocmd <nomodeline> FocusLost | endif")
                .await
                .expect("Focus Lost Failed"),
            UiCommand::FocusGained => nvim
                .command("if exists('#FocusGained') | doautocmd <nomodeline> FocusGained | endif")
                .await
                .expect("Focus Gained Failed"),
            UiCommand::FileDrop(path) => {
                nvim.command(format!("e {}", path).as_str()).await.ok();
            }
            #[cfg(windows)]
            UiCommand::RegisterRightClick => {
                if unregister_rightclick() {
                    let msg = "Could not unregister previous menu item. Possibly already registered or not running as Admin?";
                    nvim.err_writeln(msg).await.ok();
                    error!("{}", msg);
                }
                if !register_rightclick_directory() {
                    let msg = "Could not register directory context menu item. Possibly already registered or not running as Admin?";
                    nvim.err_writeln(msg).await.ok();
                    error!("{}", msg);
                }
                if !register_rightclick_file() {
                    let msg = "Could not register file context menu item. Possibly already registered or not running as Admin?";
                    nvim.err_writeln(msg).await.ok();
                    error!("{}", msg);
                }
            }
            #[cfg(windows)]
            UiCommand::UnregisterRightClick => {
                if !unregister_rightclick() {
                    let msg = "Could not remove context menu items. Possibly already removed or not running as Admin?";
                    nvim.err_writeln(msg).await.ok();
                    error!("{}", msg);
                }
            }
        }
    }
}
