#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[macro_use]
pub mod settings;

mod bridge;
mod editor;
mod error_handling;
pub mod redraw_scheduler;
mod renderer;
pub mod window;
pub use skulpin::sdl2;

#[macro_use]
extern crate derive_new;
#[macro_use]
extern crate rust_embed;
#[macro_use]
extern crate lazy_static;

use lazy_static::initialize;

use bridge::BRIDGE;

static mut INITIAL_DIMENSIONS: (u64, u64) = (100, 50);

pub fn get_initial_dimensions() -> (u64, u64) {
    unsafe { INITIAL_DIMENSIONS }
}

pub fn init_neovide(initial_dimensions: (u64, u64)) {
    unsafe {
        INITIAL_DIMENSIONS = initial_dimensions;
    }
    window::initialize_settings();
    redraw_scheduler::initialize_settings();
    renderer::cursor_renderer::initialize_settings();

    initialize(&BRIDGE);
}
