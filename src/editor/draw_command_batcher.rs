use crate::{editor::DrawCommand, window::UserEvent};

use winit::event_loop::EventLoopProxy;

pub struct DrawCommandBatcher {
    batch: Vec<DrawCommand>,
}

impl DrawCommandBatcher {
    pub fn new() -> DrawCommandBatcher {
        Self { batch: Vec::new() }
    }

    pub fn queue(&mut self, draw_command: DrawCommand) {
        self.batch.push(draw_command);
    }

    pub fn send_batch(&mut self, proxy: &EventLoopProxy<UserEvent>) {
        proxy.send_event(self.batch.split_off(0).into()).ok();
    }
}
