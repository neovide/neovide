use log::{error, trace};
use neovim_lib::{Neovim, NeovimApi};

#[cfg(windows)]
use crate::settings::windows_registry::{
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
    Quit,
    #[cfg(windows)]
    RegisterRightClick,
    #[cfg(windows)]
    UnregisterRightClick,
}

impl UiCommand {
    pub fn execute(self, nvim: &mut Neovim) {
        match self {
            UiCommand::Resize { width, height } => nvim
                .ui_try_resize(width.max(10) as i64, height.max(3) as i64)
                .expect("Resize failed"),
            UiCommand::Keyboard(input_command) => {
                trace!("Keyboard Input Sent: {}", input_command);
                nvim.input(&input_command).expect("Input failed");
            }
            UiCommand::MouseButton {
                action,
                grid_id,
                position: (grid_x, grid_y),
            } => {
                nvim.input_mouse("left", &action, "", grid_id as i64, grid_y as i64, grid_x as i64)
                    .expect("Mouse Input Failed");
            }
            UiCommand::Scroll {
                direction,
                grid_id,
                position: (grid_x, grid_y),
            } => {
                nvim.input_mouse("wheel", &direction, "", grid_id as i64, grid_y as i64, grid_x as i64)
                    .expect("Mouse Scroll Failed");
            }
            UiCommand::Drag {
                grid_id,
                position: (grid_x, grid_y)
            } => {
                nvim.input_mouse("left", "drag", "", grid_id as i64, grid_y as i64, grid_x as i64)
                    .expect("Mouse Drag Failed");
            }
            UiCommand::FocusLost => nvim
                .command("if exists('#FocusLost') | doautocmd <nomodeline> FocusLost | endif")
                .expect("Focus Lost Failed"),
            UiCommand::FocusGained => nvim
                .command("if exists('#FocusGained') | doautocmd <nomodeline> FocusGained | endif")
                .expect("Focus Gained Failed"),
            UiCommand::Quit => {
                nvim.command("qa!").ok(); // Ignoring result as it won't succeed since the app closed.
            }
            UiCommand::FileDrop(path) => {
                nvim.command(format!("e {}", path).as_str()).ok();
            }
            #[cfg(windows)]
            UiCommand::RegisterRightClick => {
                if unregister_rightclick() {
                    let msg = "Could not unregister previous menu item. Possibly already registered or not running as Admin?";
                    nvim.err_writeln(msg).ok();
                    error!("{}", msg);
                }
                if !register_rightclick_directory() {
                    let msg = "Could not register directory context menu item. Possibly already registered or not running as Admin?";
                    nvim.err_writeln(msg).ok();
                    error!("{}", msg);
                }
                if !register_rightclick_file() {
                    let msg = "Could not register file context menu item. Possibly already registered or not running as Admin?";
                    nvim.err_writeln(msg).ok();
                    error!("{}", msg);
                }
            }
            #[cfg(windows)]
            UiCommand::UnregisterRightClick => {
                if !unregister_rightclick() {
                    let msg = "Could not remove context menu items. Possibly already removed or not running as Admin?";
                    nvim.err_writeln(msg).ok();
                    error!("{}", msg);
                }
            }
        }
    }

    pub fn is_resize(&self) -> bool {
        matches!(self, UiCommand::Resize { .. })
    }
}
