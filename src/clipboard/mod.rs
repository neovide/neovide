use copypasta::{ClipboardContext, ClipboardProvider};

use log::error;

use std::{error::Error, thread};

use crate::event_aggregator::EVENT_AGGREGATOR;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync + 'static>>;

#[derive(Clone, Debug)]
pub enum ClipboardCommand {
    SetContents(String),
}

pub struct Clipboard {
    context: ClipboardContext,
}

impl Clipboard {
    pub fn new() -> Result<Self> {
        Ok(Self {
            context: ClipboardContext::new()?,
        })
    }

    pub fn get_contents(&mut self) -> Result<String> {
        self.context.get_contents()
    }

    pub fn set_contents(&mut self, lines: String) -> Result<()> {
        self.context.set_contents(lines)
    }
}

struct ClipboardCommandHandler {
    clipboard: Clipboard,
}

impl ClipboardCommandHandler {
    pub fn new() -> Result<Self> {
        Ok(Self {
            clipboard: Clipboard::new()?,
        })
    }

    fn handle_command(&mut self, command: ClipboardCommand) -> Result<()> {
        match command {
            ClipboardCommand::SetContents(lines) => self.clipboard.set_contents(lines),
        }
    }
}

pub fn start_clipboard_command_handler() {
    thread::spawn(move || {
        let mut command_handler = ClipboardCommandHandler::new().unwrap();

        let mut command_receiver = EVENT_AGGREGATOR.register_event::<ClipboardCommand>();
        while let Some(command) = command_receiver.blocking_recv() {
            if let Err(e) = command_handler.handle_command(command) {
                error!("Error in ClipboardCommandHandler: {}", e)
            }
        }
    });
}
