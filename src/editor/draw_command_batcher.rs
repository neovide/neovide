use std::cell::RefCell;

use crate::{editor::DrawCommand, window::UserEvent};

use winit::event_loop::EventLoopProxy;

pub struct DrawCommandBatcher {
    batch: RefCell<Vec<DrawCommand>>,
}

impl DrawCommandBatcher {
    pub fn new() -> DrawCommandBatcher {
        Self {
            batch: RefCell::default(),
        }
    }

    pub fn queue(&self, draw_command: DrawCommand) {
        self.batch.borrow_mut().push(draw_command);
    }

    pub fn send_batch(&self, proxy: &EventLoopProxy<UserEvent>) {
        let _ = proxy.send_event(self.batch.borrow_mut().split_off(0).into());
    }
}
