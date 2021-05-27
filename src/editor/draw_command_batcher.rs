use std::sync::mpsc::{channel, Receiver, SendError, Sender};

use super::DrawCommand;
use crate::channel_utils::*;

pub struct DrawCommandBatcher {
    window_draw_command_sender: Sender<DrawCommand>,
    window_draw_command_receiver: Receiver<DrawCommand>,

    batched_draw_command_sender: LoggingSender<Vec<DrawCommand>>,
}

impl DrawCommandBatcher {
    pub fn new(batched_draw_command_sender: LoggingSender<Vec<DrawCommand>>) -> DrawCommandBatcher {
        let (sender, receiver) = channel();

        DrawCommandBatcher {
            window_draw_command_sender: sender,
            window_draw_command_receiver: receiver,
            batched_draw_command_sender,
        }
    }

    pub fn queue(&self, draw_command: DrawCommand) -> Result<(), SendError<DrawCommand>> {
        self.window_draw_command_sender.send(draw_command)
    }

    pub fn send_batch(&self) -> Result<(), SendError<Vec<DrawCommand>>> {
        let batch = self.window_draw_command_receiver.try_iter().collect();
        self.batched_draw_command_sender.send(batch)
    }
}
