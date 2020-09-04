use std::collections::HashMap;
use std::sync::Arc;
use std::mem::swap;

use log::{trace, warn};
use unicode_segmentation::UnicodeSegmentation;

use super::grid::CharacterGrid;
use super::style::Style;
use crate::bridge::{GridLineCell, WindowAnchor};

#[derive(new, Debug, Clone)]
pub enum DrawCommand {
    Cell {
        text: String,
        cell_width: u64,
        grid_position: (u64, u64),
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
    Resize,
    Clear
}

pub struct Window {
    pub grid_id: u64,
    pub grid: CharacterGrid,
    pub hidden: bool,
    pub anchor_grid_id: Option<u64>,
    pub anchor_type: WindowAnchor,
    pub anchor_row: f64,
    pub anchor_column: f64,
    pub queued_draw_commands: Vec<DrawCommand>
}

impl Window {
    pub fn new(
        grid_id: u64,
        width: u64,
        height: u64,
        anchor_grid_id: Option<u64>,
        anchor_type: WindowAnchor,
        anchor_row: f64,
        anchor_column: f64,
    ) -> Window {
        Window {
            grid_id,
            anchor_grid_id,
            anchor_type,
            anchor_row,
            anchor_column,
            grid: CharacterGrid::new((width, height)),
            hidden: false,
            queued_draw_commands: Vec::new()
        }
    }

    pub fn resize(&mut self, width: u64, height: u64) {
        self.grid.resize(width, height);
        self.queued_draw_commands.push(DrawCommand::Resize)
    }

    pub fn clear(&mut self) {
        self.grid.clear();
        self.queued_draw_commands.push(DrawCommand::Clear);
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

        let mut draw_command_start_index = column_pos.clone();
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
        loop {
            if draw_command_start_index > 0 {
                if let Some((_, previous_style)) = &row[draw_command_start_index as usize - 1] {
                    if &style == previous_style {
                        draw_command_start_index = draw_command_start_index - 1;
                        continue;
                    }
                }
            }
            break;
        }


        let mut draw_command_end_index = column_pos.clone() - 1;
        loop {
            if draw_command_end_index < self.grid.width - 1 {
                if let Some((_, next_style)) = &row[draw_command_end_index as usize] {
                    if &style == next_style {
                        draw_command_end_index = draw_command_end_index + 1;
                        continue;
                    }
                }
            }
            break;
        }

        let mut text = String::new();
        for x in draw_command_start_index..draw_command_end_index {
            let (character, _) = row[x as usize].as_ref().unwrap();
            text.push_str(character);
        }

        self.queued_draw_commands.push(DrawCommand::Cell {
            text,
            cell_width: draw_command_end_index - draw_command_start_index,
            grid_position: (draw_command_start_index, row_index),
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

        self.queued_draw_commands.push(DrawCommand::Scroll {
            top, bot, left, right, rows, cols
        });
    }

    pub fn build_draw_commands(&mut self) -> Vec<DrawCommand> {

        let mut draw_commands = Vec::new();
        swap(&mut self.queued_draw_commands, &mut draw_commands);

        trace!("Draw commands sent");
        draw_commands
    }
}
