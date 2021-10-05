use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use log::info;

lazy_static! {
    pub static ref RUNNING_TRACKER: RunningTracker = RunningTracker::new();
}

pub struct RunningTracker {
    running: Arc<AtomicBool>,
}

impl RunningTracker {
    fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn quit(&self, reason: &str) {
        self.running.store(false, Ordering::Relaxed);
        info!("Quit {}", reason);
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}
