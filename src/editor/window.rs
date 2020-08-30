use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use log::trace;
use unicode_segmentation::UnicodeSegmentation;

use super::grid::CharacterGrid;
use super::style::Style;
use crate::bridge::{GridLineCell, WindowAnchor};

#[derive(new, Debug, Clone)]
pub struct DrawCommand {
    pub text: String,
    pub cell_width: u64,
    pub grid_position: (u64, u64),
    pub style: Option<Arc<Style>>,
}

pub struct Window {
    pub grid_id: u64,
    pub grid: CharacterGrid,
    pub hidden: bool,
    pub anchor_grid_id: Option<u64>,
    pub anchor_type: WindowAnchor,
    pub anchor_row: f64,
    pub anchor_column: f64,
    pub children: HashSet<u64>,
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
            children: HashSet::new(),
        }
    }

    pub fn resize(&mut self, width: u64, height: u64) {
        self.grid.resize(width, height);
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
            println!("Draw command out of bounds");
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
                            self.grid.set_dirty_cell(dest_x as u64, dest_y as u64);
                        }
                    }
                }
            }
        }
        trace!("Region scrolled");
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
}
