use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use log::trace;

use crate::settings::*;

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
                .neovim_arguments
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
    frames_queued: AtomicU16,
    scheduled_frame: Mutex<Option<Instant>>,
}

impl RedrawScheduler {
    pub fn new() -> RedrawScheduler {
        RedrawScheduler {
            frames_queued: AtomicU16::new(1),
            scheduled_frame: Mutex::new(None),
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
        let buffer_frames = SETTINGS.get::<RedrawSettings>().extra_buffer_frames;

        self.frames_queued
            .store(buffer_frames as u16, Ordering::Relaxed);
    }

    pub fn should_draw(&self) -> bool {
        let frames_queued = self.frames_queued.load(Ordering::Relaxed);

        if frames_queued > 0 {
            self.frames_queued
                .store(frames_queued - 1, Ordering::Relaxed);
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
