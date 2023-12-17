use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{spawn, JoinHandle},
};
use winapi::um::dwmapi::DwmFlush;
use winit::event_loop::EventLoopProxy;

use crate::{profiling::tracy_zone, window::UserEvent};

pub struct VSyncWin {
    should_exit: Arc<AtomicBool>,
    vsync_thread: Option<JoinHandle<()>>,
}

impl VSyncWin {
    // On Windows the fake vsync is always enabled
    // Everything else is very jerky
    pub fn new(proxy: EventLoopProxy<UserEvent>) -> Self {
        let should_exit = Arc::new(AtomicBool::new(false));

        // When using OpenGL on Windows in windowed mode, swap_buffers does not seem to be
        // synchronized with the Desktop Window Manager. So work around that by waiting until the
        // DWM is flushed before swapping the buffers. Using a separate thread simplifies things,
        // since it avoids race conditions when the starting the wait just before the next flush is
        // starting to happen.
        let vsync_thread = {
            let should_exit = should_exit.clone();
            Some(spawn(move || {
                while !should_exit.load(Ordering::SeqCst) {
                    unsafe {
                        tracy_zone!("VSyncThread");
                        DwmFlush();
                        let _ = proxy.send_event(UserEvent::RedrawRequested);
                    }
                }
            }))
        };

        Self {
            should_exit,
            vsync_thread,
        }
    }

    pub fn wait_for_vsync(&mut self) {}
}

impl Drop for VSyncWin {
    fn drop(&mut self) {
        self.should_exit.store(true, Ordering::SeqCst);
        self.vsync_thread.take().unwrap().join().unwrap();
    }
}
