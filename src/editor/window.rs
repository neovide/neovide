use std::collections::HashMap;
use std::sync::Arc;

use log::warn;
use unicode_segmentation::UnicodeSegmentation;

use super::grid::CharacterGrid;
use super::style::Style;
use super::{AnchorInfo, DrawCommand, DrawCommandBatcher};
use crate::bridge::GridLineCell;

#[derive(new, Clone, Debug)]
pub enum WindowDrawCommand {
    Position {
        grid_position: (f64, f64),
        grid_size: (u64, u64),
        floating_order: Option<u64>,
    },
    Cells {
        cells: Vec<String>,
        window_left: u64,
        window_top: u64,
        width: u64,
        style: Option<Arc<Style>>,
    },
    Scroll {
        top: u64,
        bottom: u64,
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
    },
}

pub enum WindowType {
    Editor,
    Message,
}

pub struct Window {
    grid_id: u64,
    grid: CharacterGrid,
    pub window_type: WindowType,

    pub anchor_info: Option<AnchorInfo>,
    grid_position: (f64, f64),

    draw_command_batcher: Arc<DrawCommandBatcher>,
}

impl Window {
    pub fn new(
        grid_id: u64,
        window_type: WindowType,
        anchor_info: Option<AnchorInfo>,
        grid_position: (f64, f64),
        grid_size: (u64, u64),
        draw_command_batcher: Arc<DrawCommandBatcher>,
    ) -> Window {
        let window = Window {
            grid_id,
            grid: CharacterGrid::new(grid_size),
            window_type,
            anchor_info,
            grid_position,
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
            grid_position: self.grid_position,
            grid_size: (self.grid.width, self.grid.height),
            floating_order: self.anchor_info.clone().map(|anchor| anchor.sort_order),
        });
    }

    pub fn get_cursor_character(&self, window_left: u64, window_top: u64) -> (String, bool) {
        let character = match self.grid.get_cell(window_left, window_top) {
            Some((character, _)) => character.clone(),
            _ => ' '.to_string(),
        };

        let double_width = match self.grid.get_cell(window_left + 1, window_top) {
            Some((character, _)) => character.is_empty(),
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
        self.grid_position
    }

    pub fn position(
        &mut self,
        anchor_info: Option<AnchorInfo>,
        grid_size: (u64, u64),
        grid_position: (f64, f64),
    ) {
        self.grid.resize(grid_size);
        self.anchor_info = anchor_info;
        self.grid_position = grid_position;
        self.send_updated_position();
        self.redraw();
    }

    pub fn resize(&mut self, new_size: (u64, u64)) {
        self.grid.resize(new_size);
        self.send_updated_position();
        self.redraw();
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
                *cell = (text, style.clone());
            }
            *column_pos += 1;
        } else {
            for character in text.graphemes(true) {
                if let Some(cell) = self.grid.get_cell_mut(*column_pos, row_index) {
                    *cell = (character.to_string(), style.clone());
                }
                *column_pos += 1;
            }
        }

        *previous_style = style;
    }

    // Send a draw command for the given row starting from current_start up until the next style
    // change or double width character.
    fn send_draw_command(&self, row_index: u64, start: u64) -> Option<u64> {
        let row = self.grid.row(row_index).unwrap();

        let (_, style) = &row[start as usize];

        let mut cells = Vec::new();
        let mut width = 0;
        for possible_end_index in start..self.grid.width {
            let (character, possible_end_style) = &row[possible_end_index as usize];

            // Style doesn't match. Draw what we've got
            if style != possible_end_style {
                break;
            }

            width += 1;
            // The previous character is double width, so send this as its own draw command
            if character.is_empty() {
                break;
            }

            // Add the grid cell to the cells to render
            cells.push(character.clone());
        }

        // Send a window draw command to the current window.
        self.send_command(WindowDrawCommand::Cells {
            cells,
            window_left: start,
            window_top: row_index,
            width,
            style: style.clone(),
        });

        Some(start + width)
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

            // Redraw the participating line by calling send_draw_command starting at 0
            // until current_start is greater than the grid width
            let mut current_start = 0;
            while current_start < self.grid.width {
                if let Some(next_start) = self.send_draw_command(row, current_start) {
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
        bottom: u64,
        left: u64,
        right: u64,
        rows: i64,
        cols: i64,
    ) {
        let y_iter: Box<dyn Iterator<Item = i64>> = if rows > 0 {
            Box::new((top as i64 + rows)..bottom as i64)
        } else {
            Box::new((top as i64..(bottom as i64 + rows)).rev())
        };

        self.send_command(WindowDrawCommand::Scroll {
            top,
            bottom,
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
                if let Some(next_start) = self.send_draw_command(row, current_start) {
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
            top_line,
            bottom_line,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel_utils::*;
    use std::collections::HashMap;
    use std::sync::mpsc::*;

    fn build_test_channels() -> (Receiver<Vec<DrawCommand>>, Arc<DrawCommandBatcher>) {
        let (batched_draw_command_sender, batched_draw_command_receiver) = channel();
        let logging_batched_draw_command_sender = LoggingSender::attach(
            batched_draw_command_sender,
            "batched_draw_command".to_owned(),
        );

        let draw_command_batcher =
            Arc::new(DrawCommandBatcher::new(logging_batched_draw_command_sender));

        (batched_draw_command_receiver, draw_command_batcher)
    }

    #[test]
    fn window_separator_modifies_grid_and_sends_draw_command() {
        let (batched_receiver, batched_sender) = build_test_channels();
        let mut window = Window::new(
            1,
            WindowType::Editor,
            None,
            (0.0, 0.0),
            (114, 64),
            batched_sender.clone(),
        );
        batched_sender
            .send_batch()
            .expect("Could not send batch of commands");
        batched_receiver.recv().expect("Could not receive commands");

        window.draw_grid_line(
            1,
            70,
            vec![GridLineCell {
                text: "|".to_owned(),
                highlight_id: None,
                repeat: None,
            }],
            &HashMap::new(),
        );

        assert_eq!(window.grid.get_cell(70, 1), Some(&("|".to_owned(), None)));

        batched_sender
            .send_batch()
            .expect("Could not send batch of commands");

        let sent_commands = batched_receiver.recv().expect("Could not receive commands");
        assert!(sent_commands.len() != 0);
    }
}
