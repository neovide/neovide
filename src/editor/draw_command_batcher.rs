use crate::{editor::DrawCommand, window::UserEvent};

use winit::event_loop::EventLoopProxy;

pub struct DrawCommandBatcher {
    batch: Vec<DrawCommand>,
    enabled: bool,
    queued: Vec<Vec<DrawCommand>>,
}

impl DrawCommandBatcher {
    pub fn new() -> DrawCommandBatcher {
        Self {
            batch: Vec::new(),
            enabled: true,
            queued: Vec::new(),
        }
    }

    pub fn queue(&mut self, draw_command: DrawCommand) {
        self.batch.push(draw_command);
    }

    pub fn set_enabled(&mut self, enabled: bool, proxy: &EventLoopProxy<UserEvent>) {
        log::info!("Set redraw {enabled}");
        if enabled && !self.enabled {
            for queued in self.queued.drain(..) {
                proxy.send_event(queued.into()).ok();
            }
        }
        self.enabled = enabled;
    }

    pub fn send_batch(&mut self, proxy: &EventLoopProxy<UserEvent>) {
        let batch = self.batch.split_off(0);
        if self.enabled {
            proxy.send_event(batch.into()).ok();
        } else {
            self.queued.push(batch);
        }
    }
}
