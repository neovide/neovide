use std::collections::HashMap;

use crate::bridge::{RedrawEvent, StyledContent};
use crate::editor::{DrawCommand, Style};

const COMMAND_SCALE: u16 = 2;

pub struct CommandLine {
    visible: bool,
    prefix: String,
    content: StyledContent,
    cursor_position: u64,
    special_char: Option<(String, bool)>,
    block: Vec<StyledContent>
}

impl CommandLine {
    pub fn new() -> CommandLine {
        CommandLine {
            visible: false,
            prefix: String::new(),
            content: Vec::new(),
            cursor_position: 0,
            special_char: None,
            block: Vec::new()
        }
    }

    pub fn draw(&self, window_size: (u64, u64), defined_styles: &HashMap<u64, Style>) -> Vec<DrawCommand> {
        let mut draw_commands = Vec::new();
        if self.visible {
            if self.content.len() > 0 {
                let (width, height) = window_size;
                let text_length: usize = self.content.iter().map(|(_, text)| text.len()).sum();

                let text_width = text_length * COMMAND_SCALE as usize;
                let text_height = COMMAND_SCALE;

                let x = (width / 2) - (text_width as u64 / 2);
                let y = (height / 2) - (text_height as u64 / 2);

                let mut start_x = x;
                let mut commands = self.content.iter().map(|(style_id, text)| {
                    let command_width = text.len() * 2;
                    let style = defined_styles.get(style_id).map(|style| style.clone());
                    let mut command = DrawCommand::new(text.clone(), (start_x, y), style);
                    command.scale = COMMAND_SCALE;
                    start_x = start_x + command_width as u64;
                    command
                }).collect::<Vec<DrawCommand>>();
                draw_commands.append(&mut commands);
            }
        }
        draw_commands
    }

    pub fn handle_command_events(&mut self, event: RedrawEvent) {
        match event {
            RedrawEvent::CommandLineShow { content, position, first_character, prompt, indent, level } => self.show(content, position, first_character, prompt, indent, level),
            RedrawEvent::CommandLinePosition { position, level } => self.set_position(position, level),
            RedrawEvent::CommandLineSpecialCharacter { character, shift, level } => self.set_special_character(character, shift, level),
            RedrawEvent::CommandLineHide => self.hide(),
            RedrawEvent::CommandLineBlockShow { lines } => self.show_block(lines),
            RedrawEvent::CommandLineBlockAppend { line } => self.append_line_to_block(line),
            RedrawEvent::CommandLineBlockHide => self.hide_block(),
            _ => {}
        }
    }

    fn show(&mut self, content: StyledContent, position: u64, first_character: String, prompt: String, _indent: u64, _level: u64) {
        let prefix;
        if first_character.len() > 0 {
            prefix = first_character;
        } else {
            prefix = prompt;
        }

        self.visible = true;
        self.prefix = prefix;
        self.content = content;
        self.cursor_position = position;
        self.block = Vec::new();
    }

    fn set_position(&mut self, position: u64, _level: u64) {
        self.cursor_position = position;
    }

    fn set_special_character(&mut self, character: String, shift: bool, _level: u64) {
        self.special_char = Some((character, shift));
    }

    fn hide(&mut self) {
        self.visible = false;
        self.special_char = None;
    }

    fn show_block(&mut self, lines: Vec<StyledContent>) {
        self.block = lines;
    }

    fn append_line_to_block(&mut self, line: StyledContent) {
        self.block.push(line);
    }

    fn hide_block(&mut self) {
        self.block.clear();
    }
}
