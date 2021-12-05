#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(test))]
use flexi_logger::{Cleanup, Criterion, Duplicate, Logger, Naming};

// Test naming occasionally uses camelCase with underscores to separate sections of
// the test name.
#[cfg_attr(test, allow(non_snake_case))]
#[macro_use]
extern crate neovide_derive;

#[macro_use]
extern crate clap;

mod bridge;
mod channel_utils;
mod cmd_line;
mod editor;
mod error_handling;
mod redraw_scheduler;
mod renderer;
mod running_tracker;
mod settings;
mod utils;
mod window;
mod windows_utils;

#[macro_use]
extern crate derive_new;
#[macro_use]
extern crate lazy_static;

use std::sync::mpsc::channel;

use log::trace;
use tokio::sync::mpsc::unbounded_channel;

use bridge::start_bridge;
use cmd_line::CmdLineSettings;
use editor::start_editor;
use renderer::{cursor_renderer::CursorSettings, RendererSettings};
use settings::SETTINGS;
use window::{create_window, KeyboardSettings, WindowSettings};

pub use channel_utils::*;
pub use running_tracker::*;
pub use windows_utils::*;

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

    //Will exit if -h or -v
    if let Err(err) = cmd_line::handle_command_line_arguments() {
        eprintln!("{}", err);
        return;
    }

    #[cfg(not(test))]
    init_logger();

    trace!("Neovide version: {}", crate_version!());

    maybe_disown();

    #[cfg(target_os = "windows")]
    windows_fix_dpi();

    #[cfg(target_os = "macos")]
    handle_macos();

    WindowSettings::register();
    RendererSettings::register();
    CursorSettings::register();
    KeyboardSettings::register();

    let (redraw_event_sender, redraw_event_receiver) = unbounded_channel();
    let logging_redraw_event_sender =
        LoggingTx::attach(redraw_event_sender, "redraw_event".to_owned());

    let (batched_draw_command_sender, batched_draw_command_receiver) = channel();
    let logging_batched_draw_command_sender = LoggingSender::attach(
        batched_draw_command_sender,
        "batched_draw_command".to_owned(),
    );

    let (ui_command_sender, ui_command_receiver) = unbounded_channel();
    let logging_ui_command_sender = LoggingTx::attach(ui_command_sender, "ui_command".to_owned());

    let (window_command_sender, window_command_receiver) = channel();
    let logging_window_command_sender =
        LoggingSender::attach(window_command_sender, "window_command".to_owned());

    // We need to keep the bridge reference around to prevent the tokio runtime from getting freed
    let _bridge = start_bridge(
        logging_ui_command_sender.clone(),
        ui_command_receiver,
        logging_redraw_event_sender,
    );
    start_editor(
        redraw_event_receiver,
        logging_batched_draw_command_sender,
        logging_window_command_sender,
    );
    create_window(
        batched_draw_command_receiver,
        window_command_receiver,
        logging_ui_command_sender,
    );
}

#[cfg(not(test))]
pub fn init_logger() {
    let settings = SETTINGS.get::<CmdLineSettings>();

    let verbosity = match settings.verbosity {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    let logger = match settings.log_to_file {
        true => Logger::with_env_or_str("neovide")
            .duplicate_to_stderr(Duplicate::Error)
            .log_to_file()
            .rotate(
                Criterion::Size(10_000_000),
                Naming::Timestamps,
                Cleanup::KeepLogFiles(1),
            ),
        false => Logger::with_env_or_str(format!("neovide = {}", verbosity)),
    };
    logger.start().expect("Could not start logger");
}

fn maybe_disown() {
    use std::{env, process};

    let settings = SETTINGS.get::<CmdLineSettings>();

    if cfg!(debug_assertions) || settings.no_fork {
        return;
    }

    if let Ok(current_exe) = env::current_exe() {
        assert!(process::Command::new(current_exe)
            .arg("--nofork")
            .args(env::args().skip(1))
            .spawn()
            .is_ok());
        process::exit(0);
    } else {
        eprintln!("error in disowning process, cannot obtain the path for the current executable, continuing without disowning...");
    }
}

#[cfg(target_os = "windows")]
fn windows_fix_dpi() {
    use winapi::shared::windef::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2;
    use winapi::um::winuser::SetProcessDpiAwarenessContext;
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

#[cfg(target_os = "macos")]
fn handle_macos() {
    use std::env;

    if env::var_os("TERM").is_none() {
        let shell = env::var("SHELL").unwrap();
        let cmd = "printenv PATH";
        if let Ok(path) = std::process::Command::new(shell)
            .arg("-lic")
            .arg(cmd)
            .output()
        {
            env::set_var("PATH", std::str::from_utf8(&path.stdout).unwrap());
        }
    }
}
