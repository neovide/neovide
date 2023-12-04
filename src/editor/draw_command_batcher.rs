use std::sync::mpsc::{channel, Receiver, SendError, Sender};

use crate::{editor::DrawCommand, window::UserEvent};

use winit::event_loop::EventLoopProxy;

pub struct DrawCommandBatcher {
    window_draw_command_sender: Sender<DrawCommand>,
    window_draw_command_receiver: Receiver<DrawCommand>,
    proxy: EventLoopProxy<UserEvent>,
}

impl DrawCommandBatcher {
    pub fn new(proxy: EventLoopProxy<UserEvent>) -> DrawCommandBatcher {
        let (sender, receiver) = channel();

        DrawCommandBatcher {
            window_draw_command_sender: sender,
            window_draw_command_receiver: receiver,
            proxy,
        }
    }

    pub fn queue(&self, draw_command: DrawCommand) -> Result<(), Box<SendError<DrawCommand>>> {
        self.window_draw_command_sender
            .send(draw_command)
            .map_err(Box::new)
    }

    pub fn send_batch(&self) {
        let batch: Vec<DrawCommand> = self.window_draw_command_receiver.try_iter().collect();
        let _ = self.proxy.send_event(UserEvent::DrawCommandBatch(batch));
    }
}
