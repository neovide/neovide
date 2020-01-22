use std::sync::{Arc, Mutex};
use std::time::Instant;

use skulpin::winit::window::Window;
use tokio::runtime::Runtime;
use tokio::time::{Instant as TokioInstant, delay_until};

lazy_static! {
    pub static ref REDRAW_SCHEDULER: RedrawScheduler = RedrawScheduler::new();
}

pub struct RedrawScheduler {
    runtime: Runtime,
    window: Mutex<Option<Arc<Window>>> // Swap to some atomic type
}

impl RedrawScheduler {
    pub fn new() -> RedrawScheduler {
        RedrawScheduler { 
            runtime: Runtime::new().unwrap(),
            window: Mutex::new(None)
        }
    }

    pub fn schedule(&self, time: Instant) {
        let window = {
            self.window.lock().unwrap().clone()
        };

        if let Some(window) = window {
            self.runtime.spawn(async move {
                delay_until(TokioInstant::from_std(time)).await;
                window.request_redraw();
            });
        }
    }

    pub fn request_redraw(&self) {
        if let Some(window) = self.window.lock().unwrap().as_ref() {
            window.request_redraw();
        }
    }

    pub fn set_window(&self, window: &Arc<Window>){
        let mut window_ref = self.window.lock().unwrap();
        window_ref.replace(window.clone());
    }
}
