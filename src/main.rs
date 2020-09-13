#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![feature(entry_insert)]

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

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::channel;
use std::process;

use tokio::sync::mpsc::unbounded_channel;

use window::window_geometry;

use bridge::start_bridge; 
use editor::start_editor;
use window::start_window;

pub const INITIAL_DIMENSIONS: (u64, u64) = (100, 50);

fn main() {
    //  -----------
    // | DATA FLOW |
    //  -----------
    //
    // Data flows in a circular motion via channels. This allows each component to handle and
    // process data on their own thread while not blocking the other components from processing.
    //
    // This way Neovim continues to produce events, the window doesn't freeze and queues up ui
    // commands, and the editor can do the processing necessary to handle the UI events
    // effectively.
    //
    // BRIDGE
    //   V REDRAW EVENT
    // EDITOR
    //   V DRAW COMMAND
    // WINDOW
    //   V UI COMMAND
    // BRIDGE
    //
    // BRIDGE:
    //   The bridge is responsible for the connection to the neovim process itself. It is in charge
    //   of starting and communicating to and from the process.
    //
    // REDRAW EVENT:
    //   Redraw events are direct events from the neovim process meant to specify how the editor
    //   should be drawn to the screen. They also include other things such as whether the mouse is
    //   enabled. The bridge takes these events, filters out some of them meant only for
    //   filtering, and forwards them to the editor.
    //
    // EDITOR:
    //   The editor is responsible for processing and transforming redraw events into something
    //   more readily renderable. Ligature support and multi window management requires some
    //   significant preprocessing of the redraw events in order to capture what exactly should get
    //   drawn where. Futher this step takes a bit of processing power to accomplish, so it is done
    //   on it's own thread. Ideally heavily computationally expensive tasks should be done in the
    //   editor.
    //
    // DRAW COMMAND:
    //   The draw commands are distilled render information describing actions to be done at the
    //   window by window level.
    //
    // WINDOW:
    //   The window is responsible for rendering and gathering input events from the user. This
    //   inncludes taking the draw commands from the editor and turning them into pixels on the
    //   screen. The ui commands are then forwarded back to the BRIDGE to convert them into
    //   commands for neovim to handle properly.
    //
    // UI COMMAND:
    //   The ui commands are things like text input/key bindings, outer window resizes, and mouse
    //   inputs.
    //
    //  ------------------
    // | Other Components |
    //  ------------------
    //
    // Neovide also includes some other systems which are globally available via lazy static
    // instantiations.
    //
    // SETTINGS:
    //   The settings system is live updated from global variables in neovim with the prefix
    //   "neovide". They allow us to configure and manage the functionality of neovide from neovim
    //   init scripts and variables.
    //
    // REDRAW SCHEDULER:
    //   The redraw scheduler is a simple system in charge of deciding if the renderer should draw
    //   another frame next frame, or if it can safely skip drawing to save battery and cpu power.
    //   Multiple other parts of the app "queue_next_frame" function to ensure animations continue
    //   properly or updates to the graphics are pushed to the screen.

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
            let shell = env::var("SHELL").unwrap();
            let cmd = format!(
                "(source /etc/profile && source {} && echo $PATH)",
                profile_path.to_str().unwrap()
            );
            if let Ok(path) = process::Command::new(shell).arg("-c").arg(cmd).output() {
                env::set_var("PATH", std::str::from_utf8(&path.stdout).unwrap());
            }
        }
    }

    bridge::layouts::initialize_settings();
    window::initialize_settings();
    redraw_scheduler::initialize_settings();
    renderer::initialize_settings();
    renderer::cursor_renderer::initialize_settings();

    let running = Arc::new(AtomicBool::new(true));

    let (redraw_event_sender, redraw_event_receiver) = channel();
    let (draw_command_sender, draw_command_receiver) = channel();
    let (ui_command_sender, ui_command_receiver) = unbounded_channel();

    start_bridge(ui_command_sender.clone(), ui_command_receiver, redraw_event_sender, running.clone());
    start_editor(redraw_event_receiver, draw_command_sender);
    start_window(draw_command_receiver, ui_command_sender, running.clone());
}
