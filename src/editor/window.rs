use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::fmt;

use log::warn;
use unicode_segmentation::UnicodeSegmentation;

use super::{DrawCommand, AnchorInfo};
use super::grid::CharacterGrid;
use super::style::Style;
use crate::bridge::GridLineCell;

#[derive(new, Clone)]
pub enum WindowDrawCommand {
    Position {
        grid_left: f64,
        grid_top: f64,
        width: u64,
        height: u64,
        floating: bool
    },
    Cell {
        text: String,
        cell_width: u64,
        window_left: u64,
        window_top: u64,
        style: Option<Arc<Style>>,
    },
    Scroll {
        top: u64,
        bot: u64,
        left: u64,
        right: u64,
        rows: i64,
        cols: i64,
    },
    Clear,
    Show,
    Hide,
    Close
}

impl fmt::Debug for WindowDrawCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WindowDrawCommand::Position { .. } => write!(formatter, "Position"),
            WindowDrawCommand::Cell { .. } => write!(formatter, "Cell"),
            WindowDrawCommand::Scroll { .. } => write!(formatter, "Scroll"),
            WindowDrawCommand::Clear => write!(formatter, "Clear"),
            WindowDrawCommand::Show => write!(formatter, "Show"),
            WindowDrawCommand::Hide => write!(formatter, "Hide"),
            WindowDrawCommand::Close => write!(formatter, "Close"),
        }
    }
}

pub struct Window {
    grid_id: u64,
    grid: CharacterGrid,

    pub anchor_info: Option<AnchorInfo>,

    grid_left: f64,
    grid_top: f64,

    draw_command_sender: Sender<DrawCommand>
}

impl Window {
    pub fn new(
        grid_id: u64,
        width: u64,
        height: u64,
        anchor_info: Option<AnchorInfo>,
        grid_left: f64,
        grid_top: f64,
        draw_command_sender: Sender<DrawCommand>
    ) -> Window {
        let window = Window {
            grid_id,
            grid: CharacterGrid::new((width, height)),
            anchor_info,
            grid_left,
            grid_top,
            draw_command_sender
        };
        window.send_updated_position();
        window
    }

    fn send_command(&self, command: WindowDrawCommand) {
        self.draw_command_sender.send(DrawCommand::Window {
            grid_id: self.grid_id,
            command
        }).ok();
    }

    fn send_updated_position(&self) {
        self.send_command(WindowDrawCommand::Position {
            grid_left: self.grid_left,
            grid_top: self.grid_top,
            width: self.grid.width,
            height: self.grid.height,
            floating: self.anchor_info.is_some()
        });
    }

    pub fn get_cursor_character(&self, window_left: u64, window_top: u64) -> (String, bool) {
        let character = match self.grid.get_cell(window_left, window_top) {
            Some(Some((character, _))) => character.clone(),
            _ => ' '.to_string(),
        };

        let double_width = match self.grid.get_cell(window_left + 1, window_top) {
            Some(Some((character, _))) => character.is_empty(),
            _ => false,
        };

        (character, double_width)
    }

    pub fn get_width(&self) -> u64 {
        self.grid.width
    }

    pub fn get_height(&self) -> u64 {
        self.grid.height
    }

    pub fn get_grid_position(&self) -> (f64, f64) {
        (self.grid_left, self.grid_top)
    }

    pub fn position(&mut self, width: u64, height: u64, anchor_info: Option<AnchorInfo>, grid_left: f64, grid_top: f64) {
        self.grid.resize(width, height);
        self.anchor_info = anchor_info;
        self.grid_left = grid_left;
        self.grid_top = grid_top;
        self.send_updated_position();
    }

    pub fn resize(&mut self, width: u64, height: u64) {
        self.grid.resize(width, height);
        self.send_updated_position();
    }

    fn draw_grid_line_cell(
        &mut self,
        row_index: u64,
        column_pos: &mut u64,
        cell: GridLineCell,
        defined_styles: &HashMap<u64, Arc<Style>>,
        previous_style: &mut Option<Arc<Style>>,
    ) {
        let style = match cell.highlight_id {
            Some(0) => None,
            Some(style_id) => defined_styles.get(&style_id).cloned(),
            None => previous_style.clone(),
        };

        let mut text = cell.text;

        if let Some(times) = cell.repeat {
            text = text.repeat(times as usize);
        }

        let cell_start_index = column_pos.clone();
        if text.is_empty() {
            if let Some(cell) = self.grid.get_cell_mut(*column_pos, row_index) {
                *cell = Some(("".to_string(), style.clone()));
            }
            *column_pos += 1;
        } else {
            for (i, character) in text.graphemes(true).enumerate() {
                if let Some(cell) = self.grid.get_cell_mut(i as u64 + *column_pos, row_index) {
                    *cell = Some((character.to_string(), style.clone()));
                }
            }
            *column_pos += text.graphemes(true).count() as u64;
        }

        let row = self.grid.row(row_index).unwrap();

        let mut draw_command_start_index = cell_start_index;
        for possible_start_index in (cell_start_index.checked_sub(3).unwrap_or(0)..cell_start_index).rev() {
            if let Some((_, possible_start_style)) = &row[possible_start_index as usize] {
                if &style == possible_start_style {
                    draw_command_start_index = possible_start_index;
                    continue;
                }
            }
            break;
        }


        let cell_end_index = column_pos.clone();
        let mut draw_command_end_index = column_pos.clone();
        for possible_end_index in cell_end_index..(cell_end_index + 3).min(self.grid.width - 1) {
            if let Some((_, possible_end_style)) = &row[possible_end_index as usize] {
                if &style == possible_end_style {
                    draw_command_end_index = possible_end_index;
                    continue;
                }
            }
            break;
        }

        let mut text = String::new();
        for x in draw_command_start_index..draw_command_end_index {
            let (character, _) = row[x as usize].as_ref().unwrap();
            text.push_str(character);
        }

        self.send_command(WindowDrawCommand::Cell {
            text,
            cell_width: draw_command_end_index - draw_command_start_index,
            window_left: draw_command_start_index,
            window_top: row_index,
            style: style.clone()
        });

        *previous_style = style;
    }

    pub fn draw_grid_line(
        &mut self,
        row: u64,
        column_start: u64,
        cells: Vec<GridLineCell>,
        defined_styles: &HashMap<u64, Arc<Style>>,
        previous_style: &mut Option<Arc<Style>>,
    ) {
        if row < self.grid.height {
            let mut column_pos = column_start;
            for cell in cells {
                self.draw_grid_line_cell(
                    row,
                    &mut column_pos,
                    cell,
                    defined_styles,
                    previous_style,
                );
            }
        } else {
            warn!("Draw command out of bounds");
        }
    }

    pub fn scroll_region(
        &mut self,
        top: u64,
        bot: u64,
        left: u64,
        right: u64,
        rows: i64,
        cols: i64,
    ) {
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
                        }
                    }
                }
            }
        }

        self.send_command(WindowDrawCommand::Scroll {
            top, bot, left, right, rows, cols
        });
    }

    pub fn clear(&mut self) {
        self.grid.clear();
        self.send_command(WindowDrawCommand::Clear);
    }

    pub fn hide(&self) {
        self.send_command(WindowDrawCommand::Hide);
    }

    pub fn show(&self) {
        self.send_command(WindowDrawCommand::Show);
    }

    pub fn close(&self) {
        self.send_command(WindowDrawCommand::Close);
    }
}
