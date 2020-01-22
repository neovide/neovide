use std::sync::Arc;
use std::time::Instant;

use skulpin::winit::window::Window;
use tokio::runtime::Runtime;
use tokio::time::{Instant as TokioInstant, delay_until};

pub struct RedrawScheduler {
    runtime: Runtime,
    window: Arc<Window>
}

impl RedrawScheduler {
    pub fn new(window: &Arc<Window>) -> RedrawScheduler {
        RedrawScheduler { 
            runtime: Runtime::new().unwrap(),
            window: window.clone()
        }
    }

    pub fn schedule(&self, time: Instant) {
        let window = self.window.clone();
        self.runtime.spawn(async move {
            delay_until(TokioInstant::from_std(time)).await;
            window.request_redraw();
        });
    }
}
