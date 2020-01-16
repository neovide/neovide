#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod editor;
mod events;
mod window;
mod keybindings;
mod renderer;
mod error_handling;
mod ui_commands;

#[macro_use] extern crate derive_new;
#[macro_use] extern crate rust_embed;

use std::sync::{Arc, Mutex};
//use std::thread;
use std::process::Stdio;
use std::sync::mpsc::{channel, Receiver};

use async_trait::async_trait;
use rmpv::Value;
use nvim_rs::runtime::ChildStdin;
use nvim_rs::{create, Neovim, UiAttachOptions, Handler};
use tokio::process::Command;
use tokio::runtime::Runtime;

use window::ui_loop;
use editor::Editor;
use events::parse_neovim_event;
use error_handling::ResultPanicExplanation;
use ui_commands::UiCommand;

const INITIAL_WIDTH: u64 = 100;
const INITIAL_HEIGHT: u64 = 50;

#[cfg(target_os = "windows")]
fn set_windows_creation_flags(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
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

struct NeovimHandler(Arc<Mutex<Editor>>);

#[async_trait]
impl Handler for NeovimHandler {
    type Writer = ChildStdin;

    async fn handle_notify(&self, event_name: String, arguments: Vec<Value>, _neovim: Neovim<ChildStdin>) {
        dbg!(&event_name);
        let parsed_events = parse_neovim_event(event_name, arguments)
            .unwrap_or_explained_panic("Could not parse event", "Could not parse event from neovim");
        for event in parsed_events {
            let mut editor = self.0.lock().unwrap();
            editor.handle_redraw_event(event);
        }
    }
}

async fn start_nvim(editor: Arc<Mutex<Editor>>, receiver: Receiver<UiCommand>) {
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

    let mut options = UiAttachOptions::new();
    options.set_linegrid_external(true);
    options.set_rgb(true);
    nvim.set_var("neovide", Value::Boolean(true)).await
        .unwrap_or_explained_panic("Could not communicate.", "Could not communicate with neovim process");
    nvim.ui_attach(INITIAL_WIDTH as i64, INITIAL_HEIGHT as i64, &options).await
        .unwrap_or_explained_panic("Could not attach.", "Could not attach ui to neovim process");

    loop {
      let r = receiver.recv();

      if let Ok(ui_command) = r {
        dbg!(&ui_command);
        ui_command.execute(&nvim).await;
      } else {
        return
      }
    }
}

fn main() {
    let rt = Runtime::new().unwrap();

    let editor = Arc::new(Mutex::new(Editor::new(INITIAL_WIDTH, INITIAL_HEIGHT)));
    let (sender, receiver) = channel::<UiCommand>();
    let editor_clone = editor.clone();
    rt.spawn(async move {
      start_nvim(editor_clone, receiver).await;
    });
    ui_loop(editor, sender, (INITIAL_WIDTH, INITIAL_HEIGHT));
}
