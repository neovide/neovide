#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bridge;
mod editor;
mod window;
mod renderer;
mod error_handling;
mod redraw_scheduler;

#[macro_use] extern crate derive_new;
#[macro_use] extern crate rust_embed;

use std::sync::{Arc, Mutex};

use window::ui_loop;
use editor::Editor;
use bridge::Bridge;

const INITIAL_DIMENSIONS: (u64, u64) = (100, 50);

fn main() {
    let editor = Arc::new(Mutex::new(Editor::new(INITIAL_DIMENSIONS)));
    let bridge = Bridge::new(editor.clone(), INITIAL_DIMENSIONS);
    ui_loop(editor, bridge, INITIAL_DIMENSIONS);
}
