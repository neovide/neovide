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

    pub fn send_batch(&self, proxy: &EventLoopProxy<EventPayload>) {
        let mut batch: Vec<DrawCommand> = self.batch.borrow_mut().split_off(0);
        // Order the draw command batches such that window draw commands are handled first
        // by grid id, and then by the draw command such that they are positioned first.
        batch.sort_by_key(|draw_command| match draw_command {
            DrawCommand::CloseWindow(_) => 0,
            DrawCommand::Window { grid_id, command } => {
                (grid_id + 1) * 100
                    + match command {
                        WindowDrawCommand::Position { .. } => 0,
                        _ => 1,
                    }
            }
            _ => 200,
        });
        proxy
            .send_event(EventPayload::new(
                batch.into(),
                winit::window::WindowId::from(0),
            ))
            .ok();
    }
}
