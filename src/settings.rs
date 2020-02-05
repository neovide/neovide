use std::sync::atomic::{AtomicBool, AtomicU16};

use crossbeam::atomic::AtomicCell;

lazy_static! {
    pub static ref SETTINGS: Settings = Settings::new();
}

pub struct Settings {
    pub neovim_arguments: AtomicCell<Option<Vec<String>>>,
    pub no_idle: AtomicBool,
    pub buffer_frames: AtomicU16
}

impl Settings {
    pub fn new() -> Settings {
        Settings {
            neovim_arguments: AtomicCell::new(None),
            no_idle: AtomicBool::new(false),
            buffer_frames: AtomicU16::new(1),
        }
    }
}
