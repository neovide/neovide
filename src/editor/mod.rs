use std::collections::HashMap;
use std::sync::Arc;
use skulpin::skia_safe::colors;
use skulpin::winit::window::Window;

mod cursor;
mod style;
mod command_line;

pub use cursor::{Cursor, CursorShape, CursorMode};
pub use style::{Colors, Style};
use crate::events::{GridLineCell, RedrawEvent};

pub type GridCell = Option<(char, Style)>;

#[derive(new, Debug, Clone)]
pub struct DrawCommand {
    pub text: String,
    pub grid_position: (u64, u64),
    pub style: Style
}

pub struct Editor {
    pub grid: Vec<Vec<GridCell>>,
    pub dirty: Vec<Vec<bool>>,
    pub should_clear: bool,

    pub window: Option<Arc<Window>>,

    pub title: String,
    pub size: (u64, u64),
    pub cursor: Cursor,
    pub default_colors: Colors,
    pub defined_styles: HashMap<u64, Style>,
    pub previous_style: Option<Style>
}

impl Editor {
    pub fn new(width: u64, height: u64) -> Editor {
        let mut editor = Editor {
            grid: Vec::new(),
            dirty: Vec::new(),
            should_clear: true,

            window: None,

            title: "Neovide".to_string(),
            cursor: Cursor::new(),
            size: (width, height),
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
            RedrawEvent::ModeChange { mode_index } => self.cursor.change_mode(mode_index, &self.defined_styles),
            RedrawEvent::BusyStart => self.cursor.enabled = false,
            RedrawEvent::BusyStop => self.cursor.enabled = true,
            RedrawEvent::Flush => { self.window.as_ref().map(|window| window.request_redraw()); },
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

            fn command_matches(command: &Option<DrawCommand>, style: &Style) -> bool {
                match command {
                    Some(command) => &command.style == style,
                    None => true
                }
            }

            fn add_character(command: &mut Option<DrawCommand>, character: &char, row_index: u64, col_index: u64, style: Style) {
                match command {
                    Some(command) => command.text.push(character.clone()),
                    None => {
                        command.replace(DrawCommand::new(character.to_string(), (col_index, row_index), style));
                    }
                }
            }

            for (col_index, cell) in row.iter().enumerate() {
                if let Some((character, new_style)) = cell {
                    if !command_matches(&command, &new_style) {
                        add_command(&mut draw_commands, command);
                        command = None;
                    }
                    add_character(&mut command, &character, row_index as u64, col_index as u64, new_style.clone());
                } else {
                    add_command(&mut draw_commands, command);
                    command = None;
                }
            }
            add_command(&mut draw_commands, command);
        }
        let should_clear = self.should_clear;

        let draw_commands = draw_commands.into_iter().filter(|command| {
            let (x, y) = command.grid_position;
            let dirty_row = &self.dirty[y as usize];

            for char_index in 0..command.text.chars().count() {
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
        let style = match (cell.highlight_id, self.previous_style.clone()) {
            (Some(0), _) => Style::new(self.default_colors.clone()),
            (Some(style_id), _) => self.defined_styles.get(&style_id).expect("GridLineCell must use defined color").clone(),
            (None, Some(previous_style)) => previous_style,
            (None, None) => Style::new(self.default_colors.clone())
        };

        let mut text = cell.text;
        if let Some(times) = cell.repeat {
            text = text.repeat(times as usize);
        }

        let row = self.grid.get_mut(row_index as usize).expect("Grid must have size greater than row_index");
        let dirty_row = &mut self.dirty[row_index as usize];
        for (i, character) in text.chars().enumerate() {
            let pointer_index = i + *column_pos as usize;
            if pointer_index < row.len() {
                row[pointer_index] = Some((character, style.clone()));
                dirty_row[pointer_index] = true;
            }
        }

        *column_pos = *column_pos + text.chars().count() as u64;
        self.previous_style = Some(style);
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
}
