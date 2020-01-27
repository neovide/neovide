use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use std::thread;

use skulpin::winit::window::Window;

lazy_static! {
    pub static ref REDRAW_SCHEDULER: RedrawScheduler = RedrawScheduler::new();
}

pub struct RedrawScheduler {
    window: Mutex<Option<Arc<Window>>>, // Would prefer not to have to lock this every time.
    frame_queued: AtomicBool,
    scheduled_frame: Mutex<Option<Instant>>
}

pub fn redraw_loop() {
    thread::spawn(|| {
        loop {
            let frame_start = Instant::now();

            let request_redraw = {
                if REDRAW_SCHEDULER.frame_queued.load(Ordering::Relaxed) {
                    REDRAW_SCHEDULER.frame_queued.store(false, Ordering::Relaxed);
                    true
                } else {
                    let mut next_scheduled_frame = REDRAW_SCHEDULER.scheduled_frame.lock().unwrap();
                    if let Some(scheduled_frame) = next_scheduled_frame.clone() {
                        if scheduled_frame < frame_start {
                            *next_scheduled_frame = None;
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                }
            };

            if request_redraw {
                let window = REDRAW_SCHEDULER.window.lock().unwrap();
                if let Some(window) = &*window {
                    window.request_redraw();
                }
            }

            if let Some(time_to_sleep) = Duration::from_secs_f32(1.0 / 60.0).checked_sub(frame_start.elapsed()) {
                thread::sleep(time_to_sleep)
            }
        }
    });
}

impl RedrawScheduler {
    pub fn new() -> RedrawScheduler {
        redraw_loop();
        RedrawScheduler { 
            window: Mutex::new(None),
            frame_queued: AtomicBool::new(false),
            scheduled_frame: Mutex::new(None)
        }
    }

    pub fn schedule(&self, new_scheduled: Instant) {
        let mut scheduled_frame = self.scheduled_frame.lock().unwrap();
        if let Some(previous_scheduled) = scheduled_frame.clone() {
            if new_scheduled < previous_scheduled {
                *scheduled_frame = Some(new_scheduled);
            }
        } else {
            *scheduled_frame = Some(new_scheduled);
        }
    }

    pub fn queue_next_frame(&self) {
        self.frame_queued.store(true, Ordering::Relaxed);
    }

    pub fn set_window(&self, window: &Arc<Window>){
        let mut window_ref = self.window.lock().unwrap();
        window_ref.replace(window.clone());
    }
}
