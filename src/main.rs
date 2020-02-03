#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bridge;
mod editor;
mod window;
mod renderer;
mod error_handling;
mod redraw_scheduler;
mod settings;

#[macro_use] extern crate derive_new;
#[macro_use] extern crate rust_embed;
#[macro_use] extern crate lazy_static;

use std::sync::atomic::Ordering;

use lazy_static::initialize;
use flexi_logger::{Logger, Criterion, Naming, Cleanup};

use bridge::BRIDGE;
use window::ui_loop;
use settings::SETTINGS;

pub const INITIAL_DIMENSIONS: (u64, u64) = (100, 50);

fn main() {
    SETTINGS.neovim_arguments.store(Some(std::env::args().filter_map(|arg| {
        if arg == "--log" {
            Logger::with_str("neovide")
                .log_to_file()
                .rotate(Criterion::Size(10_000_000), Naming::Timestamps, Cleanup::KeepLogFiles(1))
                .start()
                .expect("Could not start logger");
            return None;
        } else if arg == "--noIdle" {
            SETTINGS.no_idle.store(true, Ordering::Relaxed);
            return None;
        }
        return Some(arg.into());
    }).collect::<Vec<String>>()));

    initialize(&BRIDGE);
    ui_loop();
}
