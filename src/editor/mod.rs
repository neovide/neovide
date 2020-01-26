mod cursor;
mod style;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use skulpin::skia_safe::colors;
use unicode_segmentation::UnicodeSegmentation;

pub use cursor::{Cursor, CursorShape, CursorMode};
pub use style::{Colors, Style};
use crate::bridge::{GridLineCell, GuiOption, RedrawEvent};
use crate::redraw_scheduler::REDRAW_SCHEDULER;
use crate::INITIAL_DIMENSIONS;

lazy_static! {
    pub static ref EDITOR: Arc<Mutex<Editor>> = Arc::new(Mutex::new(Editor::new()));
}

pub type GridCell = Option<(String, Option<Style>)>;

#[derive(new, Debug, Clone)]
pub struct DrawCommand {
    pub text: String,
    pub grid_position: (u64, u64),
    pub style: Option<Style>,
    #[new(value = "1")]
    pub scale: u16
}

pub struct Editor {
    pub grid: Vec<Vec<GridCell>>,
    pub dirty: Vec<Vec<bool>>,
    pub should_clear: bool,

    pub title: String,
    pub size: (u64, u64),
    pub font_name: Option<String>,
    pub font_size: Option<f32>,
    pub cursor: Cursor,
    pub default_colors: Colors,
    pub defined_styles: HashMap<u64, Style>,
    pub previous_style: Option<Style>
}

impl Editor {
    pub fn new() -> Editor {
        let mut editor = Editor {
            grid: Vec::new(),
            dirty: Vec::new(),
            should_clear: true,

            title: "Neovide".to_string(),
            cursor: Cursor::new(),
            size: INITIAL_DIMENSIONS,
            font_name: None,
            font_size: None,
            default_colors: Colors::new(Some(colors::WHITE), Some(colors::BLACK), Some(colors::GREY)),
            defined_styles: HashMap::new(),
            previous_style: None
        };

        editor.clear();
        editor
    }

    pub fn handle_redraw_event(&mut self, event: RedrawEvent) {
        match event {
            RedrawEvent::SetTitle { title } => self.title = title,
            RedrawEvent::ModeInfoSet { cursor_modes } => self.cursor.mode_list = cursor_modes,
            RedrawEvent::OptionSet { gui_option } => self.set_option(gui_option),
            RedrawEvent::ModeChange { mode_index } => self.cursor.change_mode(mode_index, &self.defined_styles),
            RedrawEvent::BusyStart => self.cursor.enabled = false,
            RedrawEvent::BusyStop => self.cursor.enabled = true,
            RedrawEvent::Flush => REDRAW_SCHEDULER.request_redraw(),
            RedrawEvent::Resize { width, height, .. } => self.resize((width, height)),
            RedrawEvent::DefaultColorsSet { colors } => self.default_colors = colors,
            RedrawEvent::HighlightAttributesDefine { id, style } => { self.defined_styles.insert(id, style); },
            RedrawEvent::GridLine { row, column_start, cells, .. } => self.draw_grid_line(row, column_start, cells),
            RedrawEvent::Clear { .. } => self.clear(),
            RedrawEvent::CursorGoto { row, column, .. } => self.cursor.position = (row, column),
            RedrawEvent::Scroll { top, bottom, left, right, rows, columns, .. } => self.scroll_region(top, bottom, left, right, rows, columns),
            _ => {}
        };
    }

    pub fn build_draw_commands(&mut self) -> (Vec<DrawCommand>, bool) {
        let mut draw_commands = Vec::new();
        for (row_index, row) in self.grid.iter().enumerate() {
            let mut command = None;

            fn add_command(commands_list: &mut Vec<DrawCommand>, command: Option<DrawCommand>) {
                if let Some(command) = command {
                    commands_list.push(command);
                }
            }

            fn command_matches(command: &Option<DrawCommand>, style: &Option<Style>) -> bool {
                match command {
                    Some(command) => &command.style == style,
                    None => true
                }
            }

            fn add_character(command: &mut Option<DrawCommand>, character: &str, row_index: u64, col_index: u64, style: Option<Style>) {
                match command {
                    Some(command) => command.text.push_str(character),
                    None => {
                        command.replace(DrawCommand::new(character.to_string(), (col_index, row_index), style));
                    }
                }
            }

            for (col_index, cell) in row.iter().enumerate() {
                let (character, style) = cell.clone().unwrap_or_else(|| (' '.to_string(), Some(Style::new(self.default_colors.clone()))));
                if character.is_empty() {
                    add_character(&mut command, &" ", row_index as u64, col_index as u64, style.clone());
                    add_command(&mut draw_commands, command);
                    command = None;
                } else {
                    if !command_matches(&command, &style) {
                        add_command(&mut draw_commands, command);
                        command = None;
                    }
                    add_character(&mut command, &character, row_index as u64, col_index as u64, style.clone());
                }
            }
            add_command(&mut draw_commands, command);
        }
        let should_clear = self.should_clear;

        let draw_commands = draw_commands.into_iter().filter(|command| {
            let (x, y) = command.grid_position;
            let dirty_row = &self.dirty[y as usize];

            for char_index in 0..command.text.graphemes(true).count() {
                if dirty_row[x as usize + char_index] {
                    return true;
                }
            }
            return false;
        }).collect::<Vec<DrawCommand>>();

        let (width, height) = self.size;
        self.dirty = vec![vec![false; width as usize]; height as usize];
        self.should_clear = false;
        (draw_commands, should_clear)
    }

    fn draw_grid_line_cell(&mut self, row_index: u64, column_pos: &mut u64, cell: GridLineCell) {
        let style = match cell.highlight_id {
            Some(0) => None,
            Some(style_id) => self.defined_styles.get(&style_id).map(|style| style.clone()),
            None => self.previous_style.clone()
        };

        let mut text = cell.text;
        if let Some(times) = cell.repeat {
            text = text.repeat(times as usize);
        }

        let row = self.grid.get_mut(row_index as usize).expect("Grid must have size greater than row_index");
        let dirty_row = &mut self.dirty[row_index as usize];

        if text.is_empty() {
            row[*column_pos as usize] = Some(("".to_string(), style.clone()));
            dirty_row[*column_pos as usize] = true;
            *column_pos = *column_pos + 1;
        } else {
            for (i, character) in text.graphemes(true).enumerate() {
                let pointer_index = i + *column_pos as usize;
                if pointer_index < row.len() {
                    row[pointer_index] = Some((character.to_string(), style.clone()));
                    dirty_row[pointer_index] = true;
                }
            }
            *column_pos = *column_pos + text.graphemes(true).count() as u64;
        }
        self.previous_style = style;
    }

    fn draw_grid_line(&mut self, row: u64, column_start: u64, cells: Vec<GridLineCell>) {
        if row < self.grid.len() as u64 {
            let mut column_pos = column_start;
            for cell in cells {
                self.draw_grid_line_cell(row, &mut column_pos, cell);
            }
        } else {
            println!("Draw command out of bounds");
        }
    }

    fn scroll_region(&mut self, top: u64, bot: u64, left: u64, right: u64, rows: i64, cols: i64) {
        let (top, bot) =  if rows > 0 {
            (top as i64 + rows, bot as i64)
        } else if rows < 0 {
            (top as i64, bot as i64 + rows)
        } else {
            (top as i64, bot as i64)
        };

        let (left, right) = if cols > 0 {
            (left as i64 + cols, right as i64)
        } else if rows < 0 {
            (left as i64, right as i64 + cols)
        } else {
            (left as i64, right as i64)
        };

        let mut region = Vec::new();
        for y in top..bot {
            let row = &self.grid[y as usize];
            let mut copied_section = Vec::new();
            for x in left..right {
                copied_section.push(row[x as usize].clone());
            }
            region.push(copied_section);
        }

        let new_top = top as i64 - rows;
        let new_left = left as i64 - cols;

        for (y, row_section) in region.into_iter().enumerate() {
            for (x, cell) in row_section.into_iter().enumerate() {
                let y = new_top + y as i64;
                if y >= 0 && y < self.grid.len() as i64 {
                    let row = &mut self.grid[y as usize];
                    let dirty_row = &mut self.dirty[y as usize];
                    let x = new_left + x as i64;
                    if x >= 0 && x < row.len() as i64 {
                        row[x as usize] = cell;
                        dirty_row[x as usize] = true;
                    }
                }
            }
        }
    }

    fn resize(&mut self, new_size: (u64, u64)) {
        self.size = new_size;
        self.clear();
    }

    fn clear(&mut self) {
        let (width, height) = self.size;
        self.grid = vec![vec![None; width as usize]; height as usize];
        self.dirty = vec![vec![true; width as usize]; height as usize];
        self.should_clear = true;
    }

    fn set_option(&mut self, gui_option: GuiOption) {
        match gui_option {
            GuiOption::GuiFont(font_description) => {
                let parts: Vec<&str> = font_description.split(":").collect();
                self.font_name = Some(parts[0].to_string());
                for part in parts.iter().skip(1) {
                    if part.starts_with("h") && part.len() > 1 {
                        self.font_size = part[1..].parse::<f32>().ok();
                    }
                }
            },
            _ => {}
        }
    }
}

