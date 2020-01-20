#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bridge;
mod editor;
mod window;
mod renderer;
mod error_handling;

#[macro_use] extern crate derive_new;
#[macro_use] extern crate rust_embed;

use std::sync::{Arc, Mutex};

use tokio::runtime::Runtime;
use tokio::sync::mpsc::unbounded_channel;

use window::ui_loop;
use editor::Editor;
use bridge::{start_nvim, UiCommand};

const INITIAL_WIDTH: u64 = 100;
const INITIAL_HEIGHT: u64 = 50;

fn main() {
    let editor = Arc::new(Mutex::new(Editor::new(INITIAL_WIDTH, INITIAL_HEIGHT)));
    let (sender, receiver) = unbounded_channel::<UiCommand>();
    let editor_clone = editor.clone();
    start_nvim(editor_clone, receiver, (INITIAL_WIDTH, INITIAL_HEIGHT));
    ui_loop(editor, sender, (INITIAL_WIDTH, INITIAL_HEIGHT));
}
