#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod editor;
mod events;
mod window;
mod keybindings;
mod renderer;
mod error_handling;

#[macro_use] extern crate derive_new;
#[macro_use] extern crate rust_embed;

use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

use neovim_lib::{Neovim, UiAttachOptions, Session};

use window::ui_loop;
use editor::Editor;
use events::parse_neovim_event;
use error_handling::ResultPanicExplanation;

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

fn start_nvim(editor: Arc<Mutex<Editor>>) -> Neovim {
    let mut cmd = create_nvim_command();

    let mut session = Session::new_child_cmd(&mut cmd)
        .unwrap_or_explained_panic("Could not create command", "Could not create neovim process command");

    let receiver = session.start_event_loop_channel();
    let join_handle = session.take_dispatch_guard();
    let mut nvim = Neovim::new(session);
    let mut options = UiAttachOptions::new();
    options.set_cmdline_external(false);
    options.set_messages_external(false);
    options.set_linegrid_external(true);
    options.set_rgb(true);

    nvim.ui_attach(INITIAL_WIDTH as i64, INITIAL_HEIGHT as i64, &options)
        .unwrap_or_explained_panic("Could not attach.", "Could not attach ui to neovim process");

    // Listen to neovim events
    thread::spawn(move || {
        println!("UI thread spawned");
        loop {
            let (event_name, events) = receiver.recv()
                .unwrap_or_explained_panic("Could not receive event", "Could not recieve event from neovim");
            let parsed_events = parse_neovim_event(event_name, events)
                .unwrap_or_explained_panic("Could not parse event", "Could not parse event from neovim");
            for event in parsed_events {
                let mut editor = editor.lock().unwrap();
                editor.handle_redraw_event(event);
            }
        }
    });

    // Quit process when nvim exits
    thread::spawn(move || {
        join_handle.join().expect("Could not join neovim process...");
        std::process::exit(0);
    });

    nvim
}

fn main() {
    // env_logger::from_env(LoggerEnv::default().default_filter_or("warn")).init();
    let editor = Arc::new(Mutex::new(Editor::new(INITIAL_WIDTH, INITIAL_HEIGHT)));
    let nvim = start_nvim(editor.clone());
    ui_loop(editor, nvim, (INITIAL_WIDTH, INITIAL_HEIGHT));
}
