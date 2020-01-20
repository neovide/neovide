mod events;
mod handler;
mod keybindings;
mod ui_commands;

use std::sync::{Arc, Mutex};
use std::process::Stdio;

use rmpv::Value;
use nvim_rs::{create::tokio as create, UiAttachOptions};
use tokio::runtime::Runtime;
use tokio::process::Command;
use tokio::sync::mpsc::UnboundedReceiver;

pub use events::*;
pub use keybindings::*;
pub use ui_commands::UiCommand;
use crate::editor::Editor;
use crate::error_handling::ResultPanicExplanation;
use handler::NeovimHandler;

#[cfg(target_os = "windows")]
fn set_windows_creation_flags(cmd: &mut Command) {
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
}

fn create_nvim_command() -> Command {
    let mut cmd = Command::new("nvim");

    cmd.arg("--embed")
        .args(std::env::args().skip(1))
        .stderr(Stdio::inherit());

    #[cfg(target_os = "windows")]
    set_windows_creation_flags(&mut cmd);

    cmd
}

async fn start_process(editor: Arc<Mutex<Editor>>, mut receiver: UnboundedReceiver<UiCommand>, grid_dimensions: (u64, u64)) {
    let (width, height) = grid_dimensions;
    let (mut nvim, io_handler, _) = create::new_child_cmd(&mut create_nvim_command(), NeovimHandler(editor.clone())).await
        .unwrap_or_explained_panic("Could not create nvim process", "Could not locate or start the neovim process");

    tokio::spawn(async move {
        match io_handler.await {
            Err(join_error) => eprintln!("Error joining IO loop: '{}'", join_error),
            Ok(Err(error)) => eprintln!("Error: '{}'", error),
            Ok(Ok(())) => {}
        };
        std::process::exit(0);
    });

    nvim.set_var("neovide", Value::Boolean(true)).await
        .unwrap_or_explained_panic("Could not communicate.", "Could not communicate with neovim process");
    let mut options = UiAttachOptions::new();
    options.set_linegrid_external(true);
    options.set_rgb(true);
    nvim.ui_attach(width as i64, height as i64, &options).await
        .unwrap_or_explained_panic("Could not attach.", "Could not attach ui to neovim process");

    let nvim = Arc::new(nvim);
    loop {
        let mut commands = Vec::new();
        let mut resize_command = None;
        while let Ok(ui_command) = receiver.try_recv() {
            if let UiCommand::Resize { .. } = ui_command {
                resize_command = Some(ui_command);
            } else {
                commands.push(ui_command);
            }
        }
        if let Some(resize_command) = resize_command {
            commands.push(resize_command);
        }

        for ui_command in commands.into_iter() {
            ui_command.execute(&nvim).await;
        }
    }
}

pub fn start_nvim(editor: Arc<Mutex<Editor>>, receiver: UnboundedReceiver<UiCommand>, grid_dimensions: (u64, u64)) {
    let rt = Runtime::new().unwrap();

    rt.spawn(async move {
        start_process(editor, receiver, grid_dimensions).await;
    });
}
