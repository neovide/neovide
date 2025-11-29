#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// Test naming occasionally uses camelCase with underscores to separate sections of
// the test name.
#![cfg_attr(test, allow(non_snake_case))]
#![allow(unknown_lints)]
#[macro_use]
extern crate neovide_derive;

#[cfg(target_os = "windows")]
#[cfg(test)]
#[macro_use]
extern crate approx;

#[macro_use]
extern crate clap;

mod bridge;
mod channel_utils;
mod clipboard;
mod cmd_line;
mod dimensions;
mod editor;
mod error_handling;
mod frame;
mod platform;
mod profiling;
mod renderer;
mod running_tracker;
mod settings;
mod units;
mod utils;
mod window;

#[cfg(target_os = "windows")]
mod windows_utils;

#[macro_use]
extern crate derive_new;

use std::{
    env::{self, args},
    fs::{create_dir_all, File, OpenOptions},
    io::Write,
    path::PathBuf,
    process::ExitCode,
    sync::Arc,
    time::SystemTime,
};

use anyhow::Result;
use log::trace;
use settings::font::FontSettings;
use std::panic::{set_hook, PanicHookInfo};
use time::{macros::format_description, OffsetDateTime};
use winit::{error::EventLoopError, event_loop::EventLoopProxy};

#[cfg(not(test))]
use flexi_logger::{Cleanup, Criterion, Duplicate, FileSpec, Logger, Naming};

use backtrace::Backtrace;
use cmd_line::CmdLineSettings;
use error_handling::handle_startup_errors;
use renderer::{
    cursor_renderer::CursorSettings, progress_bar::ProgressBarSettings, RendererSettings,
};
use window::{
    create_event_loop, determine_grid_size, determine_window_size, Application, EventPayload,
    WindowSettings, WindowSize,
};

pub use channel_utils::*;
#[cfg(target_os = "windows")]
pub use windows_utils::*;

use crate::settings::{load_last_window_settings, Config, Settings};

pub use profiling::startup_profiler;

#[cfg(target_os = "macos")]
use crate::frame::Frame;

const DEFAULT_BACKTRACES_FILE: &str = "neovide_backtraces.log";
const BACKTRACES_FILE_ENV_VAR: &str = "NEOVIDE_BACKTRACES";
const REQUEST_MESSAGE: &str = "This is a bug and we would love for it to be reported to https://github.com/neovide/neovide/issues";

fn main() -> ExitCode {
    set_hook(Box::new(|panic_info| {
        let backtrace = Backtrace::new();

        let stderr_msg = generate_stderr_log_message(panic_info, &backtrace);
        eprintln!("{stderr_msg}");

        log_panic_to_file(panic_info, &backtrace, &None);
    }));

    #[cfg(target_os = "windows")]
    {
        windows_fix_dpi();
    }

    // This variable is set by the AppImage runtime and causes problems for child processes
    #[cfg(target_os = "linux")]
    env::remove_var("ARGV0");

    let event_loop = create_event_loop();
    clipboard::init(&event_loop);

    let colorscheme_stream = mundy::Preferences::stream(mundy::Interest::ColorScheme);

    let settings = Arc::new(Settings::new());
    let proxy = event_loop.create_proxy();
    match setup(proxy.clone(), settings.clone()) {
        Err(err) => handle_startup_errors(err, event_loop, settings.clone()),
        Ok(font_settings) => {
            // TODO: move it to application as propers
            // Set BgColor by default when using a transparent frame, so the titlebar text gets correct
            // color.
            #[cfg(target_os = "macos")]
            if settings.get::<CmdLineSettings>().frame == Frame::Transparent {
                let mut window_settings = settings.get::<WindowSettings>();
                window_settings.theme = window::ThemeSettings::BgColor;
                settings.set(&window_settings);
            }
            let window_settings = load_last_window_settings().ok();
            let window_size = determine_window_size(window_settings.as_ref(), &settings);
            let grid_size = determine_grid_size(&window_size, window_settings);

            let mut application = Application::new(
                window_size,
                grid_size,
                font_settings,
                event_loop.create_proxy(),
                settings.clone(),
                colorscheme_stream,
            );

            let result = application.run(event_loop);

            // Wait a little bit more and force Nevovim to exit after that.
            // This should not be required, but Neovim through libuv spawns childprocesses that inherits all the handles
            // This means that the stdio and stderr handles are not properly closed, so the nvim-rs
            // read will hang forever, waiting for more data to read.
            // See https://github.com/neovide/neovide/issues/2182 (which includes links to libuv issues)
            application
                .window_wrapper
                .runtime
                .runtime
                .shutdown_timeout(std::time::Duration::from_millis(500));

            match result {
                Ok(_) => application.runtime_tracker.exit_code(),
                Err(EventLoopError::ExitFailure(code)) => ExitCode::from(code as u8),
                _ => ExitCode::FAILURE,
            }
        }
    }
}

fn setup(
    proxy: EventLoopProxy<EventPayload>,
    settings: Arc<Settings>,
) -> Result<Option<FontSettings>> {
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
    //       parallel. `send_ui` is used to send those commands from the window code.
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
    // instantiations or passed between components.
    //
    // Settings:
    //   The settings system is live updated from global variables in neovim with the prefix
    //   "neovide". They allow us to configure and manage the functionality of neovide from neovim
    //   init scripts and variables.
    //
    // RunningTracker:
    //   The running tracker responds to quit requests, allowing other systems to trigger a process
    //   exit.
    //
    //  ------------------
    // | Communication flow |
    //  ------------------
    //
    // The bridge reads from Neovim, and sends `RedrawEvent` to the editor. Some events are also
    // sent directly to the window event loop using `WindowCommand`. Finally changed settings are
    // parsed, which are sent as a window event through `SettingChanged`.
    //
    // The editor reads `RedrawEvent` and sends `DrawCommand` to the Window.
    //
    // The Window event loop sends UICommand to the bridge, which forwards them to Neovim. It also
    // reads `DrawCommand`, `SettingChanged`, and `WindowCommand` from the other components.

    settings.register::<WindowSettings>();
    settings.register::<RendererSettings>();
    settings.register::<CursorSettings>();
    settings.register::<ProgressBarSettings>();

    let config = Config::init();
    Config::watch_config_file(config.clone(), proxy.clone());

    set_hook(Box::new({
        let path = config.backtraces_path.clone();
        move |panic_info: &PanicHookInfo<'_>| {
            let backtrace = Backtrace::new();

            let stderr_msg = generate_stderr_log_message(panic_info, &backtrace);
            eprintln!("{stderr_msg}");

            log_panic_to_file(panic_info, &backtrace, &path);
        }
    }));

    //Will exit if -h or -v
    cmd_line::handle_command_line_arguments(args().collect(), settings.as_ref())?;
    {
        let cmdline_settings = settings.get::<CmdLineSettings>();
        if let Some(status) = cmd_line::maybe_passthrough_to_neovim(&cmdline_settings)? {
            std::process::exit(cmd_line::exit_status_code(status));
        }
    }
    #[cfg(not(target_os = "windows"))]
    maybe_disown(&settings);

    startup_profiler();

    #[cfg(not(test))]
    init_logger(&settings);

    trace!("Neovide version: {}", crate_version!());

    Ok(config.font)
}

#[cfg(not(test))]
pub fn init_logger(settings: &Settings) {
    let cmdline_settings = settings.get::<CmdLineSettings>();

    let logger = if cmdline_settings.log_to_file {
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

#[cfg(not(target_os = "windows"))]
fn maybe_disown(settings: &Settings) {
    use std::process;

    let cmdline_settings = settings.get::<CmdLineSettings>();

    // Never fork unless a tty is attached
    if !cmdline_settings.fork || !utils::is_tty() {
        return;
    }

    if let Ok(current_exe) = env::current_exe() {
        assert!(process::Command::new(current_exe)
            .stdin(process::Stdio::null())
            .stdout(process::Stdio::null())
            .stderr(process::Stdio::null())
            .args(env::args().skip(1))
            .spawn()
            .is_ok());
        process::exit(0);
    } else {
        eprintln!("error in disowning process, cannot obtain the path for the current executable, continuing without disowning...");
    }
}

fn generate_stderr_log_message(panic_info: &PanicHookInfo, backtrace: &Backtrace) -> String {
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

fn log_panic_to_file(panic_info: &PanicHookInfo, backtrace: &Backtrace, path: &Option<PathBuf>) {
    let log_msg = generate_panic_log_message(panic_info, backtrace);

    let file_path = match path {
        Some(v) => v,
        None => &match env::var(BACKTRACES_FILE_ENV_VAR) {
            Ok(v) => PathBuf::from(v),
            Err(_) => settings::neovide_std_datapath().join(DEFAULT_BACKTRACES_FILE),
        },
    };

    if let Some(parent) = file_path.parent() {
        create_dir_all(parent).ok();
    }

    let mut file = match OpenOptions::new()
        .append(true)
        .open(file_path)
        .or_else(|_| File::create(file_path))
    {
        Ok(x) => x,
        Err(e) => {
            eprintln!("Could not create backtraces file. ({e})");
            return;
        }
    };

    match file.write_all(log_msg.as_bytes()) {
        Ok(()) => eprintln!("\nBacktrace saved to {file_path:?}!"),
        Err(e) => eprintln!("Failed writing panic to {file_path:?}: {e}"),
    }
}

fn generate_panic_log_message(panic_info: &PanicHookInfo, backtrace: &Backtrace) -> String {
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

fn generate_panic_message(panic_info: &PanicHookInfo) -> String {
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
