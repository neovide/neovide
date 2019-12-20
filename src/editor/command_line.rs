use crate::events::{GridLineCell, RedrawEvent, StyledContent};

pub struct CommandLine {
    visible: bool,
    prefix: String,
    content: StyledContent,
    cursor_position: u64,
    special_char: (String, bool),
    block: Vec<StyledContent>
}

impl CommandLine {
    pub fn handle_command_events(&mut self, event: RedrawEvent) {
        match event {
            RedrawEvent::CommandLineShow { content, position, first_character, prompt, indent, level } => {},
            RedrawEvent::CommandLinePosition { position, level } => {},
            RedrawEvent::CommandLineSpecialCharacter { character, shift, level } => {},
            RedrawEvent::CommandLineHide => {},
            RedrawEvent::CommandLineBlockShow { lines } => {},
            RedrawEvent::CommandLineBlockAppend { line } => {},
            RedrawEvent::CommandLineBlockHide => {}
            _ => {}
        }
    }

//     fn show()
}
