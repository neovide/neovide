use std::sync::atomic::AtomicBool;

use crossbeam::atomic::AtomicCell;

lazy_static! {
    pub static ref SETTINGS: Settings = Settings::default();
}

#[derive(Default)]
pub struct Settings {
    pub neovim_arguments: AtomicCell<Option<Vec<String>>>,
    pub no_idle: AtomicBool
}
