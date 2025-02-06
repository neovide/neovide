use std::{
    process::ExitCode,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
};

use log::info;

#[derive(Clone)]
pub struct RunningTracker {
    exit_code: Arc<AtomicU8>,
}

impl RunningTracker {
    pub fn new() -> Self {
        Self {
            exit_code: Arc::new(AtomicU8::new(0)),
        }
    }

    pub fn quit_with_code(&self, code: u8, reason: &str) {
        self.exit_code.store(code, Ordering::Release);
        info!("Quit with code {}: {}", code, reason);
    }

    pub fn exit_code(&self) -> ExitCode {
        ExitCode::from(self.exit_code.load(Ordering::Acquire))
    }
}
