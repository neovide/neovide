use std::{
    cmp::Reverse,
    collections::BinaryHeap,
    sync::{
        atomic::{AtomicU32, Ordering},
        mpsc::{channel, Sender},
        Arc,
    },
    time::Instant,
    thread,
};

use glutin::window::Window;
use log::trace;
use parking_lot::{Mutex, RwLock, RwLockUpgradableReadGuard};

use crate::running_tracker::RUNNING_TRACKER;

lazy_static! {
    pub static ref REDRAW_SCHEDULER: RedrawScheduler = RedrawScheduler::new();
}

thread_local! {
    static THREAD_SENDER: RwLock<Option<Sender<Instant>>> = RwLock::new(None);
}
const EXTRA_FRAMES: u32 = 100;

pub struct RedrawScheduler {
    frame_buffer: AtomicU32,
    schedule_sender: Mutex<Sender<Instant>>,
    window_reference: RwLock<Option<Arc<Window>>>,
}

impl RedrawScheduler {
    pub fn new() -> RedrawScheduler {
        let (schedule_sender, schedule_receiver) = channel();
        let scheduler = RedrawScheduler {
            frame_buffer: AtomicU32::new(0),
            schedule_sender: Mutex::new(schedule_sender),
            window_reference: RwLock::new(None),
        };

        thread::spawn(move || {
            let mut scheduled_instants: BinaryHeap<Reverse<Instant>> = BinaryHeap::new();

            while RUNNING_TRACKER.is_running() {
                while let Some(Reverse(next_scheduled_instant)) = scheduled_instants.peek() {
                    if Instant::now() >= *next_scheduled_instant {
                        let _ = scheduled_instants.pop();
                        REDRAW_SCHEDULER.redraw();
                    } else {
                        break;
                    }
                }

                if let Some(Reverse(next_scheduled_instant)) = scheduled_instants.peek() {
                    if let Ok(new_schedule) = schedule_receiver.recv_timeout(*next_scheduled_instant - Instant::now()) {
                        scheduled_instants.push(Reverse(new_schedule));
                    }
                } else if let Ok(new_schedule) = schedule_receiver.recv() {
                    scheduled_instants.push(Reverse(new_schedule));
                }
            }
        });

        scheduler
    }

    pub fn schedule_redraw(&self, redraw_at: Instant) {
        trace!("Redraw scheduled for {:?}", redraw_at);
        THREAD_SENDER.with(|sender_option| {
            let sender_option = sender_option.upgradable_read();
            if let Some(sender) = sender_option.as_ref() {
                sender.send(redraw_at).unwrap();
            } else {
                let sender = {
                    self.schedule_sender.lock().clone()
                };

                let mut empty_sender_option = RwLockUpgradableReadGuard::upgrade(sender_option);
                sender.send(redraw_at).unwrap();
                empty_sender_option.replace(sender);
            }
        });
    }

    pub fn register_window(&self, window: Arc<Window>) {
        self.window_reference.write().replace(window);
    }

    pub fn redraw(&self) {
        if let Some(window) = self.window_reference.read().as_ref() {
            window.request_redraw();
            self.frame_buffer.store(EXTRA_FRAMES, Ordering::Relaxed);
        }
    }

    pub fn should_draw_again(&self) -> bool {
        let remaining_frames = self.frame_buffer.load(Ordering::Relaxed);
        if remaining_frames > 0 {
            self.frame_buffer.store(remaining_frames - 1, Ordering::Relaxed);
            true
        } else {
            false
        }
    }
}
