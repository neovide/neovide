#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[macro_use]
mod settings;

mod bridge;
mod editor;
mod error_handling;
mod redraw_scheduler;
mod renderer;
mod window;

#[macro_use]
extern crate derive_new;
#[macro_use]
extern crate rust_embed;
#[macro_use]
extern crate lazy_static;

use lazy_static::initialize;

use bridge::BRIDGE;
use std::process;
use window::ui_loop;
use window::window_geometry;

pub const INITIAL_DIMENSIONS: (u64, u64) = (100, 50);

fn main() {
    if let Err(err) = window_geometry() {
        eprintln!("{}", err);
        process::exit(1);
    };

    #[cfg(target_os = "macos")]
    {
        use std::env;
        if env::var_os("TERM").is_none() {
            let mut profile_path = dirs::home_dir().unwrap();
            profile_path.push(".profile");
            let mut zshrc_path = dirs::home_dir().unwrap();
            zshrc_path.push(".zshrc");
            let shell = env::var("SHELL").unwrap();
            let cmd = format!(
                "(source /etc/profile && source {} && printenv)",
                profile_path.to_str().unwrap(),
            );
            if let Ok(environment) = process::Command::new(shell).arg("-c").arg(cmd).output() {
                let vars = std::str::from_utf8(&environment.stdout)
                    .unwrap()
                    .split('\n');
                for v in vars {
                    let var: Vec<&str> = v.splitn(2, '=').collect();
                    if let Some(value) = var.get(1) {
                        env::set_var(var[0], value);
                    }
                }
            }
        }
    }

    window::initialize_settings();
    redraw_scheduler::initialize_settings();
    renderer::cursor_renderer::initialize_settings();
    bridge::layouts::initialize_settings();

    initialize(&BRIDGE);
    ui_loop();
}
