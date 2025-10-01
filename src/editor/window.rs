use std::{collections::HashMap, sync::Arc};

use log::warn;
use unicode_segmentation::UnicodeSegmentation;

use crate::{
    bridge::GridLineCell,
    editor::{grid::CharacterGrid, style::Style, AnchorInfo, DrawCommand, DrawCommandBatcher},
    renderer::{box_drawing, Line, LineFragment, WindowDrawCommand},
    units::{GridRect, GridSize},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WindowType {
    Editor,
    Message { scrolled: bool },
}

pub struct Window {
    grid_id: u64,
    grid: CharacterGrid,
    pub window_type: WindowType,

    pub anchor_info: Option<AnchorInfo>,
    grid_position: (f64, f64),
}

impl Window {
    pub fn new(
        grid_id: u64,
        window_type: WindowType,
        anchor_info: Option<AnchorInfo>,
        grid_position: (f64, f64),
        grid_size: (u64, u64),
        draw_command_batcher: &mut DrawCommandBatcher,
    ) -> Window {
        let window = Window {
            grid_id,
            grid: CharacterGrid::new((grid_size.0 as usize, grid_size.1 as usize)),
            window_type,
            anchor_info,
            grid_position,
        };
        window.send_updated_position(draw_command_batcher);
        window
    }

    fn send_command(&self, batcher: &mut DrawCommandBatcher, command: WindowDrawCommand) {
        batcher.queue(DrawCommand::Window {
            grid_id: self.grid_id,
            command,
        });
    }

    fn send_updated_position(&self, batcher: &mut DrawCommandBatcher) {
        self.send_command(
            batcher,
            WindowDrawCommand::Position {
                grid_position: self.grid_position,
                grid_size: (self.grid.width as u64, self.grid.height as u64),
                anchor_info: self.anchor_info.clone(),
                window_type: self.window_type,
            },
        );
    }

    pub fn get_cursor_grid_cell(
        &self,
        window_left: u64,
        window_top: u64,
    ) -> (String, Option<Arc<Style>>, bool) {
        let grid_cell = self
            .grid
            .get_cell(window_left as usize, window_top as usize)
            .map_or((" ".to_string(), None), |(character, style)| {
                (character.clone(), style.clone())
            });

        let double_width = self
            .grid
            .get_cell(window_left as usize + 1, window_top as usize)
            .map(|(character, _)| character.is_empty())
            .unwrap_or_default();

        (grid_cell.0, grid_cell.1, double_width)
    }

    pub fn get_width(&self) -> u64 {
        self.grid.width as u64
    }

    pub fn get_height(&self) -> u64 {
        self.grid.height as u64
    }

    pub fn get_grid_position(&self) -> (f64, f64) {
        self.grid_position
    }

    pub fn position(
        &mut self,
        batcher: &mut DrawCommandBatcher,
        anchor_info: Option<AnchorInfo>,
        grid_size: (u64, u64),
        grid_position: (f64, f64),
    ) {
        self.grid
            .resize((grid_size.0 as usize, grid_size.1 as usize));
        self.anchor_info = anchor_info;
        self.grid_position = grid_position;
        self.send_updated_position(batcher);
    }

    pub fn resize(&mut self, batcher: &mut DrawCommandBatcher, new_size: (u64, u64)) {
        self.grid.resize((new_size.0 as usize, new_size.1 as usize));
        self.send_updated_position(batcher);
    }

    fn modify_grid(
        &mut self,
        row_index: usize,
        column_pos: &mut usize,
        cell: GridLineCell,
        defined_styles: &HashMap<u64, Arc<Style>>,
        previous_style: &mut Option<Arc<Style>>,
    ) {
        // Get the defined style from the style list.
        let style = match cell.highlight_id {
            Some(0) => None,
            Some(style_id) => defined_styles.get(&style_id).cloned(),
            None => previous_style.clone(),
        };

        // Compute text.
        let mut text = cell.text;
        if let Some(times) = cell.repeat {
            // Repeats of zero times should be ignored, they are mostly useful for terminal Neovim
            // to distinguish between empty lines and lines ending with spaces.
            if times == 0 {
                return;
            }
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

    // Build a line fragment for the given row starting from current_start up until the next style
    // change or double width character.
    fn build_line_fragment(
        &self,
        row_index: usize,
        start: usize,
        text: &mut String,
    ) -> (usize, LineFragment) {
        let row = self.grid.row(row_index).unwrap();

        let (_, style) = &row[start];

        let mut width = 0;
        let mut last_box_char = None;
        let mut text_range = text.len() as u32..text.len() as u32;

        for (character, possible_end_style) in row.iter().take(self.grid.width).skip(start) {
            // Style doesn't match. Draw what we've got.
            if style != possible_end_style {
                break;
            }

            // Box drawing characters are rendered specially; break up the segment such that
            // repeated box drawing characters are in a segment by themselves
            if box_drawing::is_box_char(character) {
                if text_range.is_empty() {
                    last_box_char = Some(character)
                }
                if (!text_range.is_empty() && last_box_char.is_none())
                    || last_box_char != Some(character)
                {
                    // either we have non-box chars accumulated or this is a different box char
                    // from what we have seen before. Either way, render what we have
                    break;
                }
            } else if last_box_char.is_some() {
                // render the list of box chars we have accumulated so far
                break;
            }

            width += 1;
            // The previous character is double width, so send this as its own draw command.
            if character.is_empty() {
                break;
            }

            // Add the grid cell to the cells to render.
            text.push_str(character);
            text_range.end += character.len() as u32;
        }

        let line_fragment = LineFragment {
            text: text_range,
            window_left: start as u64,
            width: width as u64,
            style: style.clone(),
        };

        (start + width, line_fragment)
    }

    // Redraw line by calling build_line_fragment starting at 0
    // until current_start is greater than the grid width and sending the resulting
    // fragments as a batch.
    fn redraw_line(&self, batcher: &mut DrawCommandBatcher, row: usize) {
        let mut current_start = 0;
        let mut line_fragments = Vec::new();
        let mut text = String::new();
        while current_start < self.grid.width {
            let (next_start, line_fragment) =
                self.build_line_fragment(row, current_start, &mut text);
            current_start = next_start;
            line_fragments.push(line_fragment);
        }
        let line = Line {
            text,
            fragments: line_fragments,
        };
        self.send_command(batcher, WindowDrawCommand::DrawLine { row, line });
    }

    pub fn draw_grid_line(
        &mut self,
        batcher: &mut DrawCommandBatcher,
        row: u64,
        column_start: u64,
        cells: Vec<GridLineCell>,
        defined_styles: &HashMap<u64, Arc<Style>>,
    ) {
        let mut previous_style = None;
        let row = row as usize;
        if row < self.grid.height {
            let mut column_pos = column_start as usize;
            for cell in cells {
                self.modify_grid(
                    row,
                    &mut column_pos,
                    cell,
                    defined_styles,
                    &mut previous_style,
                );
            }

            self.redraw_line(batcher, row);
        } else {
            warn!("Draw command out of bounds");
        }
    }

    pub fn scroll_region(
        &mut self,
        batcher: &mut DrawCommandBatcher,
        region: GridRect<u64>,
        size: GridSize<i64>,
    ) {
        let top = region.min.y;
        let bottom = region.max.y;
        let left = region.min.x;
        let right = region.max.x;
        let rows = size.height;
        let cols = size.width;
        // Scrolls must move the data and send a WindowDrawCommand to move the rendered texture so
        // that future renders draw correctly
        let is_pure_updown = self.grid.scroll_region(
            top as usize,
            bottom as usize,
            left as usize,
            right as usize,
            rows as isize,
            cols as isize,
        );

        self.send_command(
            batcher,
            WindowDrawCommand::Scroll {
                top,
                bottom,
                left,
                right,
                rows,
                cols,
            },
        );

        // There's no need to send any updates for pure up/down scrolling, the actual new lines
        // will be sent later
        if !is_pure_updown {
            let mut top = top as isize;
            let mut bottom = bottom as isize;
            // Send only the scrolled lines
            // neovim will send the rest later
            if rows > 0 {
                bottom -= rows as isize;
            } else {
                top -= rows as isize;
            }

            for row in top..bottom {
                self.redraw_line(batcher, row as usize);
            }
        }
    }

    pub fn clear(&mut self, batcher: &mut DrawCommandBatcher) {
        self.grid.clear();
        self.send_command(batcher, WindowDrawCommand::Clear);
    }

    pub fn redraw(&self, batcher: &mut DrawCommandBatcher) {
        self.send_command(batcher, WindowDrawCommand::Clear);
        // Draw the lines from the bottom up so that underlines don't get overwritten by the line
        // below.
        for row in (0..self.grid.height).rev() {
            self.redraw_line(batcher, row);
        }
    }

    pub fn hide(&self, batcher: &mut DrawCommandBatcher) {
        self.send_command(batcher, WindowDrawCommand::Hide);
    }

    pub fn show(&self, batcher: &mut DrawCommandBatcher) {
        self.send_command(batcher, WindowDrawCommand::Show);
    }

    pub fn close(&self, batcher: &mut DrawCommandBatcher) {
        self.send_command(batcher, WindowDrawCommand::Close);
    }
}
