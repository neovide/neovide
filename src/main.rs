#![windows_subsystem = "windows"]

mod editor;
mod events;
mod window;
mod keybindings;

#[macro_use]
extern crate derive_new;

use std::process::{Command, Stdio};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::thread;

use env_logger::Env as LoggerEnv;
use neovim_lib::{Neovim, UiAttachOptions, Session};
use rmpv::Value;

use window::ui_loop;
use editor::Editor;
use events::parse_neovim_event;

const INITIAL_WIDTH: u64 = 100;
const INITIAL_HEIGHT: u64 = 50;


fn nvim_event_loop(receiver: Receiver<(String, Vec<Value>)>, editor: &Arc<Mutex<Editor>>) {
    println!("UI thread spawned");
    loop {
        let (event_name, events) = receiver.recv().expect("Could not receive event.");
        let parsed_events = parse_neovim_event(event_name, events).expect("Event parse failed...");
        for event in parsed_events {
            let mut editor = editor.lock().unwrap();
            editor.handle_redraw_event(event);
        }
    }
}

#[cfg(target_os = "windows")]
fn set_windows_creation_flags(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
}

fn main() {
    env_logger::from_env(LoggerEnv::default().default_filter_or("warn")).init();

    let mut cmd = Command::new("nvim");
    cmd.arg("--embed")
        .stderr(Stdio::inherit());

    #[cfg(target_os = "windows")]
    set_windows_creation_flags(&mut cmd);

    let mut session = Session::new_child_cmd(&mut cmd).unwrap();
    let receiver = session.start_event_loop_channel();
    let mut nvim = Neovim::new(session);
    let mut options = UiAttachOptions::new();
    options.set_cmdline_external(false);
    options.set_messages_external(false);
    options.set_linegrid_external(true);
    options.set_rgb(true);
    nvim.ui_attach(INITIAL_WIDTH as i64, INITIAL_HEIGHT as i64, &options).unwrap();

    let editor = Arc::new(Mutex::new(Editor::new(nvim, INITIAL_WIDTH, INITIAL_HEIGHT)));

    let nvim_editor = editor.clone();
    thread::spawn(move || {
        nvim_event_loop(receiver, &nvim_editor);
    });

    ui_loop(editor);
}
