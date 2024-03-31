use std::sync::{
    atomic::{AtomicI32, Ordering},
    Arc,
};

use log::info;

lazy_static! {
    pub static ref RUNNING_TRACKER: RunningTracker = RunningTracker::new();
}

pub struct RunningTracker {
    exit_code: Arc<AtomicI32>,
}

impl RunningTracker {
    fn new() -> Self {
        Self {
            exit_code: Arc::new(AtomicI32::new(0)),
        }
    }

    pub fn quit(&self, reason: &str) {
        info!("Quit {}", reason);
    }

    pub fn quit_with_code(&self, code: i32, reason: &str) {
        self.exit_code.store(code, Ordering::Release);
        info!("Quit with code {}: {}", code, reason);
    }

    pub fn exit_code(&self) -> i32 {
        self.exit_code.load(Ordering::Acquire)
    }
}
