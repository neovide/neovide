#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

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
mod dimensions;
mod editor;
mod error_handling;
mod event_aggregator;
mod frame;
mod redraw_scheduler;
mod renderer;
mod running_tracker;
mod settings;
mod window;
mod windows_utils;

#[macro_use]
extern crate derive_new;
#[macro_use]
extern crate lazy_static;

use std::env::args;

#[cfg(not(test))]
use flexi_logger::{Cleanup, Criterion, Duplicate, Logger, Naming};
use log::trace;

use bridge::start_bridge;
use cmd_line::CmdLineSettings;
use editor::start_editor;
use renderer::{cursor_renderer::CursorSettings, RendererSettings};
use settings::SETTINGS;
use window::{create_window, KeyboardSettings, WindowSettings};

pub use channel_utils::*;
pub use event_aggregator::*;
pub use running_tracker::*;
pub use windows_utils::*;

fn main() {
    //  --------------
    // | Architecture |
    //  --------------
    //
    // BRIDGE:
    //   The bridge is responsible for the connection to the neovim process itself. It is in charge
    //   of starting and communicating to and from the process. The bridge is async and has a
    //   couple of sub components:
    //
    //     NEOVIM HANDLER:
    //       This component handles events from neovim sent specifically to the gui. This includes
    //       redraw events responsible for updating the gui state, and custom neovide specific
    //       events which are registered on startup and handle syncing of settings or features from
    //       the neovim process.
    //
    //     UI COMMAND HANDLER:
    //       This component handles communication from other components to the neovim process. The
    //       commands are split into Serial and Parallel commands. Serial commands must be
    //       processed in order while parallel commands can be processed in any order and in
    //       parallel.
    //
    // EDITOR:
    //   The editor is responsible for processing and transforming redraw events into something
    //   more readily renderable. Ligature support and multi window management requires some
    //   significant preprocessing of the redraw events in order to capture what exactly should get
    //   drawn where. Futher this step takes a bit of processing power to accomplish, so it is done
    //   on it's own thread. Ideally heavily computationally expensive tasks should be done in the
    //   editor.
    //
    // RENDERER:
    //   The renderer is responsible for drawing the editor's output to the screen. It uses skia
    //   for drawing and is responsible for maintaining the various draw surfaces which are stored
    //   to prevent unnecessary redraws.
    //
    // WINDOW:
    //   The window is responsible for rendering and gathering input events from the user. This
    //   inncludes taking the draw commands from the editor and turning them into pixels on the
    //   screen. The ui commands are then forwarded back to the BRIDGE to convert them into
    //   commands for neovim to handle properly.
    //
    //  ------------------
    // | Other Components |
    //  ------------------
    //
    // Neovide also includes some other systems which are globally available via lazy static
    // instantiations.
    //
    // EVENT AGGREGATOR:
    //   Central system which distributes events to each of the other components. This is done
    //   using TypeIds and channels. Any component can publish any Clone + Debug + Send + Sync type
    //   to the aggregator, but only one component can subscribe to any type. The system takes
    //   pains to ensure that channels are shared by thread in order to keep things performant.
    //   Also tokio channels are used so that the async components can properly await for events.
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

    #[cfg(target_os = "windows")]
    windows_attach_to_console();

    //Will exit if -h or -v
    if let Err(err) = cmd_line::handle_command_line_arguments(args().collect()) {
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

    // We need to keep the bridge reference around to prevent the tokio runtime from getting freed
    let _bridge = start_bridge();
    start_editor();
    create_window();
}

#[cfg(not(test))]
pub fn init_logger() {
    let settings = SETTINGS.get::<CmdLineSettings>();

    let logger = if settings.log_to_file {
        Logger::with_env_or_str("neovide")
            .log_to_file()
            .rotate(
                Criterion::Size(10_000_000),
                Naming::Timestamps,
                Cleanup::KeepLogFiles(1),
            )
            .duplicate_to_stderr(Duplicate::Error)
    } else {
        Logger::with_env_or_str("neovide = error")
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
        // printenv is the proper way to print env variables. using echo $PATH would break Fish.
        let cmd = "printenv PATH";
        if let Ok(path) = std::process::Command::new(shell)
            .arg("-lic") // interactive login shell, this simulates opening a real terminal emulator
            .arg(cmd)
            .output()
        {
            env::set_var("PATH", std::str::from_utf8(&path.stdout).unwrap());
        }
    }
}

#[cfg(target_os = "windows")]
fn windows_attach_to_console() {
    // Attach to parent console tip found here: https://github.com/rust-lang/rust/issues/67159#issuecomment-987882771
    use winapi::um::wincon::{AttachConsole, ATTACH_PARENT_PROCESS};
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}
