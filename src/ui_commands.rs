use nvim_rs::Neovim;
use nvim_rs::compat::tokio::Compat;
use tokio::process::ChildStdin;

#[derive(Debug)]
pub enum UiCommand {
    Resize { width: i64, height: i64 },
    Keyboard(String),
    MouseButton { action: String, position: (i64, i64) },
    Scroll { direction: String, position: (i64, i64) },
    Drag(i64, i64)
}

impl UiCommand {
    pub async fn execute(&self, nvim: &Neovim<Compat<ChildStdin>>) {
        match self {
            UiCommand::Resize { width, height } => 
                nvim.ui_try_resize(*width.max(&10), *height.max(&3)).await
                    .expect("Resize failed"),
            UiCommand::Keyboard(input_command) => { 
                nvim.input(&input_command).await
                    .expect("Input failed"); 
            },
            UiCommand::MouseButton { action, position: (grid_x, grid_y) } => 
                nvim.input_mouse("left", action, "", 0, *grid_x, *grid_y).await
                    .expect("Mouse Input Failed"),
            UiCommand::Scroll { direction, position: (grid_x, grid_y) } => 
                nvim.input_mouse("wheel", direction, "", 0, *grid_x, *grid_y).await
                    .expect("Mouse Scroll Failed"),
            UiCommand::Drag(grid_x, grid_y) =>
                nvim.input_mouse("left", "drag", "", 0, *grid_x, *grid_y).await
                    .expect("Mouse Drag Failed")
        }
    }
}
