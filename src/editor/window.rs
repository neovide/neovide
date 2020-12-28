use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use log::warn;
use unicode_segmentation::UnicodeSegmentation;

use super::grid::CharacterGrid;
use super::style::Style;
use super::{AnchorInfo, DrawCommand, DrawCommandBatcher};
use crate::bridge::GridLineCell;

#[derive(new, Clone)]
pub enum WindowDrawCommand {
    Position {
        grid_left: f64,
        grid_top: f64,
        width: u64,
        height: u64,
        floating: bool,
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
    Close,
    Viewport {
        top_line: f64,
        bottom_line: f64,
    }
}

impl fmt::Debug for WindowDrawCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WindowDrawCommand::Position {
                grid_left,
                grid_top,
                ..
            } => write!(
                formatter,
                "Position {{ left: {}, right: {} }}",
                grid_left, grid_top
            ),
            WindowDrawCommand::Cell { .. } => write!(formatter, "Cell"),
            WindowDrawCommand::Scroll { .. } => write!(formatter, "Scroll"),
            WindowDrawCommand::Clear => write!(formatter, "Clear"),
            WindowDrawCommand::Show => write!(formatter, "Show"),
            WindowDrawCommand::Hide => write!(formatter, "Hide"),
            WindowDrawCommand::Close => write!(formatter, "Close"),
            WindowDrawCommand::Viewport { 
                top_line, 
                bottom_line 
            } => write!(
                formatter, 
                "Viewport {{ top: {}, bottom: {} }}",
                top_line, bottom_line),
        }
    }
}

pub struct Window {
    grid_id: u64,
    grid: CharacterGrid,

    pub anchor_info: Option<AnchorInfo>,

    grid_left: f64,
    grid_top: f64,

    draw_command_batcher: Arc<DrawCommandBatcher>,
}

impl Window {
    pub fn new(
        grid_id: u64,
        width: u64,
        height: u64,
        anchor_info: Option<AnchorInfo>,
        grid_left: f64,
        grid_top: f64,
        draw_command_batcher: Arc<DrawCommandBatcher>,
    ) -> Window {
        let window = Window {
            grid_id,
            grid: CharacterGrid::new((width, height)),
            anchor_info,
            grid_left,
            grid_top,
            draw_command_batcher,
        };
        window.send_updated_position();
        window
    }

    fn send_command(&self, command: WindowDrawCommand) {
        self.draw_command_batcher
            .queue(DrawCommand::Window {
                grid_id: self.grid_id,
                command,
            })
            .ok();
    }

    fn send_updated_position(&self) {
        self.send_command(WindowDrawCommand::Position {
            grid_left: self.grid_left,
            grid_top: self.grid_top,
            width: self.grid.width,
            height: self.grid.height,
            floating: self.anchor_info.is_some(),
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

    pub fn position(
        &mut self,
        width: u64,
        height: u64,
        anchor_info: Option<AnchorInfo>,
        grid_left: f64,
        grid_top: f64,
    ) {
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

    fn modify_grid(
        &mut self,
        row_index: u64,
        column_pos: &mut u64,
        cell: GridLineCell,
        defined_styles: &HashMap<u64, Arc<Style>>,
        previous_style: &mut Option<Arc<Style>>,
    ) {
        // Get the defined style from the style list
        let style = match cell.highlight_id {
            Some(0) => None,
            Some(style_id) => defined_styles.get(&style_id).cloned(),
            None => previous_style.clone(),
        };

        // Compute text
        let mut text = cell.text;
        if let Some(times) = cell.repeat {
            text = text.repeat(times as usize);
        }

        // Insert the contents of the cell into the grid.
        if text.is_empty() {
            if let Some(cell) = self.grid.get_cell_mut(*column_pos, row_index) {
                *cell = Some((" ".to_string(), style.clone()));
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

        *previous_style = style;
    }

    // Send a draw command for the given row starting from current_start up until the next style
    // change. If the current_start is the same as line_start, this will also work backwards in the
    // line in order to ensure that ligatures before the beginning of the grid cell are also
    // updated.
    fn send_draw_command(
        &self,
        row_index: u64,
        line_start: u64,
        current_start: u64,
    ) -> Option<u64> {
        let row = self.grid.row(row_index).unwrap();

        let (_, style) = &row[current_start as usize].as_ref()?;

        let mut draw_command_start_index = current_start;
        if current_start == line_start {
            // Locate contiguous same styled cells before the inserted cells.
            // This way any ligatures are correctly rerendered.
            // This could be sped up if we knew what characters were a part of a ligature, but in the
            // current system we do not.
            for possible_start_index in (0..current_start).rev() {
                if let Some((_, possible_start_style)) = &row[possible_start_index as usize] {
                    if style == possible_start_style {
                        draw_command_start_index = possible_start_index;
                        continue;
                    }
                }
                break;
            }
        }

        let mut draw_command_end_index = current_start;
        for possible_end_index in draw_command_start_index..self.grid.width {
            if let Some((_, possible_end_style)) = &row[possible_end_index as usize] {
                if style == possible_end_style {
                    draw_command_end_index = possible_end_index;
                    continue;
                }
            }
            break;
        }

        // Build up the actual text to be rendered including the contiguously styled bits.
        let mut text = String::new();
        for x in draw_command_start_index..(draw_command_end_index + 1) {
            let (character, _) = row[x as usize].as_ref().unwrap();
            text.push_str(character);
        }

        // Send a window draw command to the current window.
        self.send_command(WindowDrawCommand::Cell {
            text,
            cell_width: draw_command_end_index - draw_command_start_index + 1,
            window_left: draw_command_start_index,
            window_top: row_index,
            style: style.clone(),
        });

        Some(draw_command_end_index + 1)
    }

    pub fn draw_grid_line(
        &mut self,
        row: u64,
        column_start: u64,
        cells: Vec<GridLineCell>,
        defined_styles: &HashMap<u64, Arc<Style>>,
    ) {
        let mut previous_style = None;
        if row < self.grid.height {
            let mut column_pos = column_start;
            for cell in cells {
                self.modify_grid(
                    row,
                    &mut column_pos,
                    cell,
                    defined_styles,
                    &mut previous_style,
                );
            }

            let mut current_start = column_start;
            while current_start < column_pos {
                if let Some(next_start) = self.send_draw_command(row, column_start, current_start) {
                    current_start = next_start;
                } else {
                    break;
                }
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

        self.send_command(WindowDrawCommand::Scroll {
            top,
            bot,
            left,
            right,
            rows,
            cols,
        });

        // Scrolls must not only translate the rendered texture, but also must move the grid data
        // accordingly so that future renders work correctly.
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
    }

    pub fn clear(&mut self) {
        self.grid.clear();
        self.send_command(WindowDrawCommand::Clear);
    }

    pub fn redraw(&self) {
        self.send_command(WindowDrawCommand::Clear);
        for row in 0..self.grid.height {
            let mut current_start = 0;
            while current_start < self.grid.width {
                if let Some(next_start) = self.send_draw_command(row, 0, current_start) {
                    current_start = next_start;
                } else {
                    break;
                }
            }
        }
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

    pub fn update_viewport(&self, top_line: f64, bottom_line: f64) {
        self.send_command(WindowDrawCommand::Viewport {
            top_line, bottom_line
        });
    }
}
