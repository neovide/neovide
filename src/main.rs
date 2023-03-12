#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// Test naming occasionally uses camelCase with underscores to separate sections of
// the test name.
#![cfg_attr(test, allow(non_snake_case))]
#[macro_use]
extern crate neovide_derive;

#[macro_use]
extern crate clap;

mod bridge;
mod channel_utils;
mod clipboard;
mod cmd_line;
mod dimensions;
mod editor;
mod error_handling;
mod event_aggregator;
mod frame;
mod profiling;
mod renderer;
mod running_tracker;
mod settings;
mod window;

#[cfg(target_os = "windows")]
mod windows_utils;

#[macro_use]
extern crate derive_new;
#[macro_use]
extern crate lazy_static;

use std::env::{self, args};

#[cfg(not(test))]
use flexi_logger::{Cleanup, Criterion, Duplicate, FileSpec, Logger, Naming};

use log::trace;

use backtrace::Backtrace;
use bridge::start_bridge;
use cmd_line::CmdLineSettings;
use editor::start_editor;
use renderer::{cursor_renderer::CursorSettings, RendererSettings};
use settings::SETTINGS;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::panic::{set_hook, PanicInfo};
use std::time::SystemTime;
use time::macros::format_description;
use time::OffsetDateTime;
use window::{create_window, KeyboardSettings, WindowSettings};

pub use channel_utils::*;
pub use event_aggregator::*;
pub use running_tracker::*;
#[cfg(target_os = "windows")]
pub use windows_utils::*;

use crate::settings::Config;

pub use profiling::startup_profiler;

const BACKTRACES_FILE: &str = "neovide_backtraces.log";
const REQUEST_MESSAGE: &str = "This is a bug and we would love for it to be reported to https://github.com/neovide/neovide/issues";

fn main() {
    set_hook(Box::new(|panic_info| {
        let backtrace = Backtrace::new();

        let stderr_msg = generate_stderr_log_message(panic_info, &backtrace);
        eprintln!("{stderr_msg}");

        log_panic_to_file(panic_info, &backtrace);
    }));

    protected_main()
}

fn protected_main() {
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
    //   drawn where. Further this step takes a bit of processing power to accomplish, so it is done
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

    startup_profiler();

    #[cfg(target_os = "windows")]
    windows_attach_to_console();

    Config::init();

    //Will exit if -h or -v
    if let Err(err) = cmd_line::handle_command_line_arguments(args().collect()) {
        eprintln!("{err}");
        return;
    }

    #[cfg(not(test))]
    init_logger();

    trace!("Neovide version: {}", crate_version!());

    maybe_disown();

    #[cfg(target_os = "windows")]
    windows_fix_dpi();

    WindowSettings::register();
    RendererSettings::register();
    CursorSettings::register();
    KeyboardSettings::register();

    start_bridge();
    start_editor();
    create_window();
}

#[cfg(not(test))]
pub fn init_logger() {
    let settings = SETTINGS.get::<CmdLineSettings>();

    let logger = if settings.log_to_file {
        Logger::try_with_env_or_str("neovide")
            .expect("Could not init logger")
            .log_to_file(FileSpec::default())
            .rotate(
                Criterion::Size(10_000_000),
                Naming::Timestamps,
                Cleanup::KeepLogFiles(1),
            )
            .duplicate_to_stderr(Duplicate::Error)
    } else {
        Logger::try_with_env_or_str("neovide = error").expect("Could not init logger")
    };

    logger.start().expect("Could not start logger");
}

fn maybe_disown() {
    use std::process;

    let settings = SETTINGS.get::<CmdLineSettings>();

    if cfg!(debug_assertions) || settings.no_fork {
        return;
    }

    #[cfg(target_os = "windows")]
    windows_detach_from_console();

    if let Ok(current_exe) = env::current_exe() {
        assert!(process::Command::new(current_exe)
            .stdin(process::Stdio::null())
            .stdout(process::Stdio::null())
            .stderr(process::Stdio::null())
            .arg("--nofork")
            .args(env::args().skip(1))
            .spawn()
            .is_ok());
        process::exit(0);
    } else {
        eprintln!("error in disowning process, cannot obtain the path for the current executable, continuing without disowning...");
    }
}

fn generate_stderr_log_message(panic_info: &PanicInfo, backtrace: &Backtrace) -> String {
    if cfg!(debug_assertions) {
        let print_backtrace = match env::var("RUST_BACKTRACE") {
            Ok(x) => x == "full" || x == "1",
            Err(_) => false,
        };

        let backtrace_msg = match print_backtrace {
            true => format!("{backtrace:?}"),
            false => {
                "note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace"
                    .to_owned()
            }
        };

        let panic_msg = generate_panic_message(panic_info);

        format!("{panic_msg}\n{REQUEST_MESSAGE}\n{backtrace_msg}")
    } else {
        let panic_msg = generate_panic_message(panic_info);
        format!("{panic_msg}\n{REQUEST_MESSAGE}")
    }
}

fn log_panic_to_file(panic_info: &PanicInfo, backtrace: &Backtrace) {
    let log_msg = generate_panic_log_message(panic_info, backtrace);

    let mut file = match OpenOptions::new()
        .append(true)
        .open(BACKTRACES_FILE)
        .or_else(|_| File::create(BACKTRACES_FILE))
    {
        Ok(x) => x,
        Err(e) => {
            eprintln!("Could not create backtraces file. ({e})");
            return;
        }
    };

    match file.write_all(log_msg.as_bytes()) {
        Ok(()) => eprintln!("\nBacktrace saved to {BACKTRACES_FILE}!"),
        Err(e) => eprintln!("Failed writing panic to {BACKTRACES_FILE}: {e}"),
    }
}

fn generate_panic_log_message(panic_info: &PanicInfo, backtrace: &Backtrace) -> String {
    let system_time: OffsetDateTime = SystemTime::now().into();

    let timestamp = system_time
        .format(format_description!(
            "[year]-[month]-[day] [hour]:[minute]:[second]"
        ))
        .expect("Failed to parse current time");

    let partial_panic_msg = generate_panic_message(panic_info);
    let full_panic_msg = format!("{timestamp} - {partial_panic_msg}");

    format!("{full_panic_msg}\n{backtrace:?}\n")
}

fn generate_panic_message(panic_info: &PanicInfo) -> String {
    // As per the documentation for `.location()`(https://doc.rust-lang.org/std/panic/struct.PanicInfo.html#method.location)
    // the call to location cannot currently return `None`, so we unwrap.
    let location_info = panic_info.location().unwrap();
    let file = location_info.file();
    let line = location_info.line();
    let column = location_info.column();

    let raw_payload = panic_info.payload();

    let payload = match raw_payload
        .downcast_ref::<&str>()
        .map(ToOwned::to_owned)
        // Some panic messages are &str, some are String, try both to see which it is
        .or_else(|| raw_payload.downcast_ref().map(String::as_str))
    {
        Some(msg) => msg.to_owned(),
        None => return "Could not parse panic payload to a string. This is a bug.".to_owned(),
    };

    format!("Neovide panicked with the message '{payload}'. (File: {file}; Line: {line}, Column: {column})")
}
