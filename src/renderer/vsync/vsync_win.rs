use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    },
    thread::{spawn, JoinHandle},
};
use winapi::um::dwmapi::DwmFlush;

use crate::profiling::tracy_zone;

pub struct VSyncWin {
    should_exit: Arc<AtomicBool>,
    vsync_thread: Option<JoinHandle<()>>,
    vsync_count: Arc<(Mutex<usize>, Condvar)>,
    last_vsync: usize,
}

impl VSyncWin {
    // On Windows the fake vsync is always enabled
    // Everything else is very jerky
    pub fn new() -> Self {
        let should_exit = Arc::new(AtomicBool::new(false));
        let should_exit2 = should_exit.clone();
        let vsync_count = Arc::new((Mutex::new(0), Condvar::new()));
        let vsync_count2 = vsync_count.clone();

        // When using OpenGL on Windows in windowed mode, swap_buffers does not seem to be
        // synchronized with the Desktop Window Manager. So work around that by waiting until the
        // DWM is flushed before swapping the buffers. Using a separate thread simplifies things,
        // since it avoids race conditions when the starting the wait just before the next flush is
        // starting to happen.
        let vsync_thread = Some(spawn(move || {
            let (lock, cvar) = &*vsync_count2;
            while !should_exit2.load(Ordering::SeqCst) {
                unsafe {
                    tracy_zone!("VSyncThread");
                    DwmFlush();
                    {
                        let mut count = lock.lock().unwrap();
                        *count += 1;
                        cvar.notify_one();
                    }
                }
            }
        }));

        Self {
            should_exit,
            vsync_thread,
            vsync_count,
            last_vsync: 0,
        }
    }

    pub fn wait_for_vsync(&mut self) {
        let (lock, cvar) = &*self.vsync_count;
        let count = cvar
            .wait_while(lock.lock().unwrap(), |count| *count < self.last_vsync + 1)
            .unwrap();
        self.last_vsync = *count;
    }
}

impl Drop for VSyncWin {
    fn drop(&mut self) {
        self.should_exit.store(true, Ordering::SeqCst);
        self.vsync_thread.take().unwrap().join().unwrap();
    }
}
