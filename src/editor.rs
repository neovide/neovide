use std::collections::HashMap;

use neovim_lib::{Neovim, NeovimApi};
use skulpin::skia_safe::{colors, Color4f};

use crate::events::{GridLineCell, RedrawEvent};

#[derive(new, PartialEq, Debug, Clone)]
pub struct Colors {
    pub foreground: Option<Color4f>,
    pub background: Option<Color4f>,
    pub special: Option<Color4f>
}

#[derive(new, Debug, Clone, PartialEq)]
pub struct Style {
    pub colors: Colors,
    #[new(default)]
    pub reverse: bool,
    #[new(default)]
    pub italic: bool,
    #[new(default)]
    pub bold: bool,
    #[new(default)]
    pub strikethrough: bool,
    #[new(default)]
    pub underline: bool,
    #[new(default)]
    pub undercurl: bool,
    #[new(default)]
    pub blend: u8
}

impl Style {
    pub fn foreground(&self, default_colors: &Colors) -> Color4f {
        if self.reverse {
            self.colors.background.clone().unwrap_or(default_colors.background.clone().unwrap())
        } else {
            self.colors.foreground.clone().unwrap_or(default_colors.foreground.clone().unwrap())
        }
    }

    pub fn background(&self, default_colors: &Colors) -> Color4f {
        if self.reverse {
            self.colors.foreground.clone().unwrap_or(default_colors.foreground.clone().unwrap())
        } else {
            self.colors.background.clone().unwrap_or(default_colors.background.clone().unwrap())
        }
    }

    pub fn special(&self, default_colors: &Colors) -> Color4f {
        self.colors.special.clone().unwrap_or(default_colors.special.clone().unwrap())
    }
}

#[derive(new, Debug, Clone, PartialEq)]
pub struct ModeInfo {
    #[new(default)]
    pub cursor_type: Option<CursorType>,
    #[new(default)]
    pub cursor_style_id: Option<u64>
}

pub type GridCell = Option<(char, Style)>;

#[derive(new, Debug, Clone)]
pub struct DrawCommand {
    pub text: String,
    pub grid_position: (u64, u64),
    pub style: Style
}

#[derive(Debug, Clone, PartialEq)]
pub enum CursorType {
    Block,
    Horizontal,
    Vertical
}

impl CursorType {
    pub fn from_type_name(name: &str) -> Option<CursorType> {
        match name {
            "block" => Some(CursorType::Block),
            "horizontal" => Some(CursorType::Horizontal),
            "vertical" => Some(CursorType::Vertical),
            _ => None
        }
    }
}

pub struct Editor {
    pub grid: Vec<Vec<GridCell>>,
    pub cursor_pos: (u64, u64),
    pub cursor_type: CursorType,
    pub cursor_style: Option<Style>,
    pub cursor_enabled: bool,
    pub size: (u64, u64),
    pub default_colors: Colors,
    pub defined_styles: HashMap<u64, Style>,
    pub mode_list: Vec<ModeInfo>,
    pub previous_style: Option<Style>
}

impl Editor {
    pub fn new(width: u64, height: u64) -> Editor {
        let mut editor = Editor {
            grid: Vec::new(),
            cursor_pos: (0, 0),
            cursor_type: CursorType::Block,
            cursor_style: None,
            cursor_enabled: true,
            size: (width, height),
            default_colors: Colors::new(Some(colors::WHITE), Some(colors::BLACK), Some(colors::GREY)),
            defined_styles: HashMap::new(),
            mode_list: Vec::new(),
            previous_style: None
        };
        editor.clear();
        editor
    }

    pub fn cursor_foreground(&self) -> Color4f {
        if let Some(cursor_style) = &self.cursor_style {
            cursor_style.colors.foreground.clone().unwrap_or(self.default_colors.background.clone().unwrap())
        } else {
            self.default_colors.background.clone().unwrap()
        }
    }

    pub fn cursor_background(&self) -> Color4f {
        if let Some(cursor_style) = &self.cursor_style {
            cursor_style.colors.background.clone().unwrap_or(self.default_colors.foreground.clone().unwrap())
        } else {
            self.default_colors.foreground.clone().unwrap()
        }
    }

    pub fn build_draw_commands(&self) -> Vec<DrawCommand> {
        self.grid.iter().enumerate().map(|(row_index, row)| {
            let mut draw_commands = Vec::new();
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

            draw_commands
        }).flatten().collect()
    }

    pub fn handle_redraw_event(&mut self, event: RedrawEvent) {
        match event {
            RedrawEvent::ModeInfoSet { mode_list } => self.set_mode_list(mode_list),
            RedrawEvent::ModeChange { mode_index } => self.change_mode(mode_index),
            RedrawEvent::BusyStart => self.set_cursor_enabled(false),
            RedrawEvent::BusyStop => self.set_cursor_enabled(true),
            RedrawEvent::Resize { width, height, .. } => self.resize(width, height),
            RedrawEvent::DefaultColorsSet { foreground, background, special } => self.set_default_colors(foreground, background, special),
            RedrawEvent::HighlightAttributesDefine { id, style } => self.define_style(id, style),
            RedrawEvent::GridLine { row, column_start, cells, .. } => self.draw_grid_line(row, column_start, cells),
            RedrawEvent::Clear { .. } => self.clear(),
            RedrawEvent::CursorGoto { row, column, .. } => self.jump_cursor_to(row, column),
            RedrawEvent::Scroll { top, bottom, left, right, rows, columns, .. } => self.scroll_region(top, bottom, left, right, rows, columns)
        }
    }

    pub fn set_mode_list(&mut self, mode_list: Vec<ModeInfo>) {
        self.mode_list = mode_list;
    }

    pub fn change_mode(&mut self, mode_index: u64) {
        if let Some(ModeInfo { cursor_type, cursor_style_id })  = self.mode_list.get(mode_index as usize) {
            if let Some(cursor_type) = cursor_type {
                self.cursor_type = cursor_type.clone();
            }

            if let Some(cursor_style_id) = cursor_style_id {
                self.cursor_style = self.defined_styles
                    .get(cursor_style_id)
                    .map(|style_reference| style_reference.clone());
            }
        }
    }

    pub fn set_cursor_enabled(&mut self, cursor_enabled: bool) {
        self.cursor_enabled = cursor_enabled;
    }

    pub fn resize(&mut self, new_width: u64, new_height: u64) {
        self.size = (new_width, new_height);
    }

    fn set_default_colors(&mut self, foreground: Color4f, background: Color4f, special: Color4f) {
        self.default_colors = Colors::new(Some(foreground), Some(background), Some(special));
    }

    fn define_style(&mut self, id: u64, style: Style) {
        self.defined_styles.insert(id, style);
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
        for (i, character) in text.chars().enumerate() {
            let pointer_index = i + *column_pos as usize;
            if pointer_index < row.len() {
                row[pointer_index] = Some((character, style.clone()));
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

    fn clear(&mut self) {
        let (width, height) = self.size;
        self.grid = vec![vec![None; width as usize]; height as usize];
    }

    fn jump_cursor_to(&mut self, row: u64, col: u64) {
        self.cursor_pos = (row, col);
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

        let width = right - left;
        let height = bot - top;

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
                    let mut row = &mut self.grid[y as usize];
                    let x = new_left + x as i64;
                    if x >= 0 && x < row.len() as i64 {
                        row[x as usize] = cell;
                    }
                }
            }
        }
    }
}
