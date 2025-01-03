use std::cell::RefCell;

use crate::{editor::DrawCommand, renderer::WindowDrawCommand, window::EventPayload};

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

    pub fn send_batch(
        &self,
        winit_window_id: winit::window::WindowId,
        proxy: &EventLoopProxy<EventPayload>,
    ) {
        proxy
            .send_event(EventPayload::new(
                self.batch.borrow_mut().split_off(0).into(),
                winit::window::WindowId::from(winit_window_id),
            ))
            .ok();
    }
}
