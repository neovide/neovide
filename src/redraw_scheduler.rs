use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use log::trace;

use crate::{cmd_line::CmdLineSettings, settings::*};

lazy_static! {
    pub static ref REDRAW_SCHEDULER: RedrawScheduler = RedrawScheduler::new();
}

#[derive(Clone, SettingGroup)]
pub struct RedrawSettings {
    extra_buffer_frames: u64,
}

impl Default for RedrawSettings {
    fn default() -> Self {
        Self {
            extra_buffer_frames: if SETTINGS
                .get::<CmdLineSettings>()
                .neovim_args
                .contains(&"--extraBufferFrames".to_string())
            {
                60
            } else {
                1
            },
        }
    }
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

    pub fn schedule(&self, new_scheduled: Instant) {
        trace!("Redraw scheduled for {:?}", new_scheduled);
        let mut scheduled_frame = self.scheduled_frame.lock().unwrap();

        if let Some(previous_scheduled) = *scheduled_frame {
            if new_scheduled < previous_scheduled {
                *scheduled_frame = Some(new_scheduled);
            }
        } else {
            *scheduled_frame = Some(new_scheduled);
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
