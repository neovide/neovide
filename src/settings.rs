use std::sync::atomic::{AtomicBool, AtomicU16};

use flexi_logger::{Logger, Criterion, Naming, Cleanup};

lazy_static! {
    pub static ref SETTINGS: Settings = Settings::new();
}

pub struct Settings {
    pub neovim_arguments: Vec<String>,
    
    pub no_idle: AtomicBool,
    pub buffer_frames: AtomicU16
}

impl Settings {
    pub fn new() -> Settings {
        let mut no_idle = false;
        let mut buffer_frames = 1;

        let neovim_arguments = std::env::args().filter(|arg| {
            if arg == "--log" {
                Logger::with_str("neovide")
                    .log_to_file()
                    .rotate(Criterion::Size(10_000_000), Naming::Timestamps, Cleanup::KeepLogFiles(1))
                    .start()
                    .expect("Could not start logger");
                false
            } else if arg == "--noIdle" {
                no_idle = true;
                false
            } else if arg == "--extraBufferFrames" {
                buffer_frames = 60;
                false
            } else {
                true
            }
        }).collect::<Vec<String>>();

        Settings {
            neovim_arguments,
            no_idle: AtomicBool::new(no_idle),
            buffer_frames: AtomicU16::new(buffer_frames),
        }
    }
}
