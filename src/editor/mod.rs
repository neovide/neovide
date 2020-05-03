mod cursor;
mod grid;
mod style;

use std::collections::HashMap;
use std::sync::Arc;

use log::trace;
use parking_lot::Mutex;
use skulpin::skia_safe::colors;
use unicode_segmentation::UnicodeSegmentation;

use crate::bridge::{EditorMode, GridLineCell, GuiOption, RedrawEvent};
use crate::redraw_scheduler::REDRAW_SCHEDULER;
use crate::window::window_geometry_or_default;
pub use cursor::{Cursor, CursorMode, CursorShape};
pub use grid::CharacterGrid;
pub use style::{Colors, Style};

lazy_static! {
    pub static ref EDITOR: Arc<Mutex<Editor>> = Arc::new(Mutex::new(Editor::new()));
}

#[derive(new, Debug, Clone)]
pub struct DrawCommand {
    pub text: String,
    pub cell_width: u64,
    pub grid_position: (u64, u64),
    pub style: Option<Arc<Style>>,
}

pub struct Editor {
    pub grid: CharacterGrid,
    pub title: String,
    pub mouse_enabled: bool,
    pub guifont: Option<String>,
    pub cursor: Cursor,
    pub default_style: Arc<Style>,
    pub defined_styles: HashMap<u64, Arc<Style>>,
    pub previous_style: Option<Arc<Style>>,
    pub mode_list: Vec<CursorMode>,
    pub current_mode: EditorMode,
}

impl Editor {
    pub fn new() -> Editor {
        Editor {
            grid: CharacterGrid::new(window_geometry_or_default()),
            title: "Neovide".to_string(),
            mouse_enabled: true,
            guifont: None,
            cursor: Cursor::new(),
            default_style: Arc::new(Style::new(Colors::new(
                Some(colors::WHITE),
                Some(colors::BLACK),
                Some(colors::GREY),
            ))),
            defined_styles: HashMap::new(),
            previous_style: None,
            mode_list: Vec::new(),
            current_mode: EditorMode::Unknown(String::from("")),
        }
    }

    pub fn handle_redraw_event(&mut self, event: RedrawEvent) {
        match event {
            RedrawEvent::SetTitle { title } => self.title = title,
            RedrawEvent::ModeInfoSet { cursor_modes } => self.mode_list = cursor_modes,
            RedrawEvent::OptionSet { gui_option } => self.set_option(gui_option),
            RedrawEvent::ModeChange { mode, mode_index } => {
                if let Some(cursor_mode) = self.mode_list.get(mode_index as usize) {
                    self.cursor.change_mode(cursor_mode, &self.defined_styles);
                    self.current_mode = mode
                }
            }
            RedrawEvent::MouseOn => {
                self.mouse_enabled = true;
            }
            RedrawEvent::MouseOff => {
                self.mouse_enabled = false;
            }
            RedrawEvent::BusyStart => {
                trace!("Cursor off");
                self.cursor.enabled = false;
            }
            RedrawEvent::BusyStop => {
                trace!("Cursor on");
                self.cursor.enabled = true;
            }
            RedrawEvent::Flush => {
                trace!("Image flushed");
                REDRAW_SCHEDULER.queue_next_frame();
            }
            RedrawEvent::Resize { width, height, .. } => self.grid.resize(width, height),
            RedrawEvent::DefaultColorsSet { colors } => {
                self.default_style = Arc::new(Style::new(colors))
            }
            RedrawEvent::HighlightAttributesDefine { id, style } => {
                self.defined_styles.insert(id, Arc::new(style));
            }
            RedrawEvent::GridLine {
                row,
                column_start,
                cells,
                ..
            } => self.draw_grid_line(row, column_start, cells),
            RedrawEvent::Clear { .. } => self.grid.clear(),
            RedrawEvent::CursorGoto { row, column, .. } => self.cursor.position = (row, column),
            RedrawEvent::Scroll {
                top,
                bottom,
                left,
                right,
                rows,
                columns,
                ..
            } => self.scroll_region(top, bottom, left, right, rows, columns),
            _ => {}
        };
    }

    pub fn build_draw_commands(&mut self) -> (Vec<DrawCommand>, bool) {
        let mut draw_commands = Vec::new();

        for (row_index, row) in self.grid.rows().enumerate() {
            let mut command = None;

            fn add_command(commands_list: &mut Vec<DrawCommand>, command: Option<DrawCommand>) {
                if let Some(command) = command {
                    commands_list.push(command);
                }
            }

            fn command_matches(command: &Option<DrawCommand>, style: &Option<Arc<Style>>) -> bool {
                match command {
                    Some(command) => &command.style == style,
                    None => true,
                }
            }

            fn add_character(
                command: &mut Option<DrawCommand>,
                character: &str,
                row_index: u64,
                col_index: u64,
                style: Option<Arc<Style>>,
            ) {
                match command {
                    Some(command) => {
                        command.text.push_str(character);
                        command.cell_width += 1;
                    }
                    None => {
                        command.replace(DrawCommand::new(
                            character.to_string(),
                            1,
                            (col_index, row_index),
                            style,
                        ));
                    }
                }
            }

            for (col_index, cell) in row.iter().enumerate() {
                if let Some((character, style)) = cell {
                    if character.is_empty() {
                        add_character(
                            &mut command,
                            &" ",
                            row_index as u64,
                            col_index as u64,
                            style.clone(),
                        );
                        add_command(&mut draw_commands, command);
                        command = None;
                    } else {
                        if !command_matches(&command, &style) {
                            add_command(&mut draw_commands, command);
                            command = None;
                        }
                        add_character(
                            &mut command,
                            &character,
                            row_index as u64,
                            col_index as u64,
                            style.clone(),
                        );
                    }
                } else {
                    if !command_matches(&command, &None) {
                        add_command(&mut draw_commands, command);
                        command = None;
                    }
                    add_character(&mut command, " ", row_index as u64, col_index as u64, None);
                }
            }
            add_command(&mut draw_commands, command);
        }

        let should_clear = self.grid.should_clear;
        let draw_commands = draw_commands
            .into_iter()
            .filter(|command| {
                let (x, y) = command.grid_position;
                let min = (x as i64 - 1).max(0) as u64;
                let max = (x + command.cell_width + 1).min(self.grid.width);

                for char_index in min..max {
                    if self.grid.is_dirty_cell(char_index, y) {
                        return true;
                    }
                }
                false
            })
            .collect::<Vec<DrawCommand>>();

        self.grid.set_dirty_all(false);
        self.grid.should_clear = false;

        trace!("Draw commands sent");
        (draw_commands, should_clear)
    }

    fn draw_grid_line_cell(&mut self, row_index: u64, column_pos: &mut u64, cell: GridLineCell) {
        let style = match cell.highlight_id {
            Some(0) => None,
            Some(style_id) => self.defined_styles.get(&style_id).cloned(),
            None => self.previous_style.clone(),
        };

        let mut text = cell.text;

        if let Some(times) = cell.repeat {
            text = text.repeat(times as usize);
        }

        if text.is_empty() {
            if let Some(cell) = self.grid.get_cell_mut(*column_pos, row_index) {
                *cell = Some(("".to_string(), style.clone()));
            }

            self.grid.set_dirty_cell(*column_pos, row_index);
            *column_pos += 1;
        } else {
            for (i, character) in text.graphemes(true).enumerate() {
                if let Some(cell) = self.grid.get_cell_mut(i as u64 + *column_pos, row_index) {
                    *cell = Some((character.to_string(), style.clone()));
                    self.grid.set_dirty_cell(*column_pos, row_index);
                }
            }
            *column_pos += text.graphemes(true).count() as u64;
        }

        self.previous_style = style;
    }

    fn draw_grid_line(&mut self, row: u64, column_start: u64, cells: Vec<GridLineCell>) {
        if row < self.grid.height {
            let mut column_pos = column_start;
            for cell in cells {
                self.draw_grid_line_cell(row, &mut column_pos, cell);
            }
        } else {
            println!("Draw command out of bounds");
        }
    }

    fn scroll_region(&mut self, top: u64, bot: u64, left: u64, right: u64, rows: i64, cols: i64) {
        let y_iter: Box<dyn Iterator<Item = i64>> = if rows > 0 {
            Box::new((top as i64 + rows)..bot as i64)
        } else {
            Box::new((top as i64..(bot as i64 + rows)).rev())
        };

        for y in y_iter {
            let dest_y = y - rows;
            if dest_y >= 0 && dest_y < self.grid.height as i64 {
                let x_iter: Box<dyn Iterator<Item = i64>> = if cols > 0 {
                    Box::new((left as i64 + cols)..right as i64)
                } else {
                    Box::new((left as i64..(right as i64 + cols)).rev())
                };

                for x in x_iter {
                    let dest_x = x - cols;
                    let cell_data = self.grid.get_cell(x as u64, y as u64).cloned();

                    if let Some(cell_data) = cell_data {
                        if let Some(dest_cell) =
                            self.grid.get_cell_mut(dest_x as u64, dest_y as u64)
                        {
                            *dest_cell = cell_data;
                            self.grid.set_dirty_cell(dest_x as u64, dest_y as u64);
                        }
                    }
                }
            }
        }
        trace!("Region scrolled");
    }

    fn set_option(&mut self, gui_option: GuiOption) {
        trace!("Option set {:?}", &gui_option);
        match gui_option {
            GuiOption::GuiFont(guifont) => {
                self.guifont = Some(guifont);
            }
            _ => {}
        }
    }
}
