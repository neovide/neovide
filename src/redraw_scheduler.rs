use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::Instant,
};

use log::trace;

lazy_static! {
    pub static ref REDRAW_SCHEDULER: RedrawScheduler = RedrawScheduler::new();
}

pub struct RedrawScheduler {
    scheduled_frame: Mutex<Option<Instant>>,
    frame_queued: AtomicBool,
}

impl RedrawScheduler {
    pub fn new() -> RedrawScheduler {
        RedrawScheduler {
            scheduled_frame: Mutex::new(None),
            frame_queued: AtomicBool::new(true),
        }
    }

    pub fn queue_next_frame(&self) {
        trace!("Next frame queued");
        self.frame_queued.store(true, Ordering::Relaxed);
    }

    pub fn should_draw(&self) -> bool {
        if self.frame_queued.load(Ordering::Relaxed) {
            self.frame_queued.store(false, Ordering::Relaxed);
            true
        } else {
            let mut next_scheduled_frame = self.scheduled_frame.lock().unwrap();

            if let Some(scheduled_frame) = *next_scheduled_frame {
                if scheduled_frame < Instant::now() {
                    *next_scheduled_frame = None;
                    true
                } else {
                    false
                }
            } else {
                false
            }
        }
    }
}
