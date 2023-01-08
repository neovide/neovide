use std::sync::Arc;

use crate::editor::style::Style;

pub type GridCell = (String, Option<Arc<Style>>);

#[macro_export]
macro_rules! default_cell {
    () => {
        (" ".to_owned(), None)
    };
}

#[derive(Clone)]
struct GridLine {
    characters: Vec<GridCell>,
}

impl GridLine {
    pub fn new(length: usize) -> GridLine {
        GridLine {
            characters: vec![default_cell!(); length],
        }
    }
}

pub struct CharacterGrid {
    pub width: usize,
    pub height: usize,

    lines: Vec<GridLine>,
}

impl CharacterGrid {
    pub fn new((width, height): (usize, usize)) -> CharacterGrid {
        CharacterGrid {
            width,
            height,
            lines: vec![GridLine::new(width); height],
        }
    }

    pub fn resize(&mut self, (width, height): (usize, usize)) {
        let mut new_lines = vec![GridLine::new(width); height];

        for x in 0..self.width.min(width) {
            for (y, new_line) in new_lines
                .iter_mut()
                .enumerate()
                .take(self.height.min(height))
            {
                if let Some(existing_cell) = self.get_cell(x, y) {
                    new_line.characters[x] = existing_cell.clone();
                }
            }
        }

        self.width = width;
        self.height = height;
        self.lines = new_lines;
    }

    pub fn clear(&mut self) {
        self.set_all_characters(default_cell!());
    }

    pub fn get_cell(&self, x: usize, y: usize) -> Option<&GridCell> {
        self.lines
            .get(y)
            .and_then(|line| line.characters.get(x))
    }

    pub fn get_cell_mut(&mut self, x: usize, y: usize) -> Option<&mut GridCell> {
        self.lines
            .get_mut(y)
            .and_then(|line| line.characters.get_mut(x))
    }

    pub fn set_all_characters(&mut self, value: GridCell) {
        for line in &mut self.lines {
            for grid in &mut line.characters {
                *grid = value.clone()
            }
        }
    }

    pub fn row(&self, row_index: usize) -> Option<&[GridCell]> {
        if row_index < self.height {
            Some(&self.lines[row_index].characters[..])
        } else {
            None
        }
    }

    pub fn scroll_region(
        &mut self,
        top: usize,
        bottom: usize,
        left: usize,
        right: usize,
        rows: isize,
        cols: isize,
    ) {
        let mut top_to_bottom;
        let mut bottom_to_top;
        let y_iter: &mut dyn Iterator<Item = usize> = if rows > 0 {
            top_to_bottom = (top as isize + rows) as usize..bottom;
            &mut top_to_bottom
        } else {
            bottom_to_top = (top..(bottom as isize + rows) as usize).rev();
            &mut bottom_to_top
        };

        for y in y_iter {
            let dest_y = y as isize - rows;
            let mut cols_left;
            let mut cols_right;
            if dest_y >= 0 && dest_y < self.height as isize {
                let x_iter: &mut dyn Iterator<Item = usize> = if cols > 0 {
                    cols_left = (left as isize + cols) as usize..right;
                    &mut cols_left
                } else {
                    cols_right = (left..(right as isize + cols) as usize).rev();
                    &mut cols_right
                };

                for x in x_iter {
                    let dest_x = ((x as isize) - cols) as usize;
                    let cell_data = self.get_cell(x, y).cloned();

                    if let Some(cell_data) = cell_data {
                        if let Some(dest_cell) = self.get_cell_mut(dest_x, dest_y as usize) {
                            *dest_cell = cell_data;
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::style::Colors;
    use rand::*;

    #[derive(Debug)]
    struct Context {
        none_colors: Colors,
        size: (usize, usize),
        x: usize,
        y: usize,
    }

    impl Context {
        fn new() -> Self {
            let size = (
                (thread_rng().gen::<usize>() % 500) + 1,
                (thread_rng().gen::<usize>() % 500) + 1,
            );
            let (x, y) = (
                thread_rng().gen::<usize>() % size.0,
                thread_rng().gen::<usize>() % size.1,
            );
            Self {
                none_colors: Colors {
                    foreground: None,
                    background: None,
                    special: None,
                },
                size,
                x,
                y,
            }
        }
    }

    fn assert_all_cells_equal_to(context: &Context, grid: &CharacterGrid, cell: &GridCell) {
        for x in 0..context.size.0 {
            for y in 0..context.size.1 {
                assert_eq!(grid.get_cell(x, y), Some(cell));
            }
        }
    }

    #[test]
    fn new_constructs_grid() {
        let context = Context::new();

        // RUN FUNCTION
        let character_grid = CharacterGrid::new(context.size);
        assert_eq!(character_grid.width, context.size.0);
        assert_eq!(character_grid.height, context.size.1);
        assert_all_cells_equal_to(&context, &character_grid, &default_cell!());
    }

    #[test]
    fn get_cell_returns_expected_cell() {
        let context = Context::new();
        let mut character_grid = CharacterGrid::new(context.size);

        character_grid.lines[context.y].characters[context.x] = (
            "foo".to_string(),
            Some(Arc::new(Style::new(context.none_colors.clone()))),
        );
        let result = (
            "foo".to_string(),
            Some(Arc::new(Style::new(context.none_colors.clone()))),
        );

        // RUN FUNCTION
        assert_eq!(
            character_grid.get_cell(context.x, context.y).unwrap(),
            &result
        );
    }

    #[test]
    fn get_cell_mut_modifiers_grid_properly() {
        let context = Context::new();
        let mut character_grid = CharacterGrid::new(context.size);

        character_grid.lines[context.y].characters[context.x] = (
            "foo".to_string(),
            Some(Arc::new(Style::new(context.none_colors.clone()))),
        );
        let result = (
            "bar".to_string(),
            Some(Arc::new(Style::new(context.none_colors.clone()))),
        );

        // RUN FUNCTION
        let cell = character_grid.get_cell_mut(context.x, context.y).unwrap();
        *cell = (
            "bar".to_string(),
            Some(Arc::new(Style::new(context.none_colors.clone()))),
        );

        assert_eq!(
            character_grid.get_cell_mut(context.x, context.y).unwrap(),
            &result
        );
    }

    #[test]
    fn set_all_characters_sets_all_cells_to_given_character() {
        let context = Context::new();
        let grid_cell = (
            "foo".to_string(),
            Some(Arc::new(Style::new(context.none_colors.clone()))),
        );
        let mut character_grid = CharacterGrid::new(context.size);

        // RUN FUNCTION
        character_grid.set_all_characters(grid_cell.clone());
        assert_all_cells_equal_to(&context, &character_grid, &grid_cell);
    }

    #[test]
    fn clear_empties_buffer() {
        let context = Context::new();
        let mut character_grid = CharacterGrid::new(context.size);

        let grid_cell = (
            "foo".to_string(),
            Some(Arc::new(Style::new(context.none_colors.clone()))),
        );
        character_grid.set_all_characters(grid_cell.clone());

        // RUN FUNCTION
        character_grid.clear();

        assert_eq!(character_grid.width, context.size.0);
        assert_eq!(character_grid.height, context.size.1);
        assert_all_cells_equal_to(&context, &character_grid, &default_cell!());
    }

    #[test]
    fn resize_clears_and_resizes_grid() {
        let context = Context::new();
        let mut character_grid = CharacterGrid::new(context.size);
        let (width, height) = (
            (thread_rng().gen::<usize>() % 500) + 1,
            (thread_rng().gen::<usize>() % 500) + 1,
        );

        let grid_cell = (
            "foo".to_string(),
            Some(Arc::new(Style::new(context.none_colors))),
        );
        character_grid.set_all_characters(grid_cell.clone());

        // RUN FUNCTION
        character_grid.resize((width, height));

        assert_eq!(character_grid.width, width);
        assert_eq!(character_grid.height, height);

        let (original_width, original_height) = context.size;
        for x in 0..original_width.min(width) {
            for y in 0..original_height.min(height) {
                assert_eq!(character_grid.get_cell(x, y).unwrap(), &grid_cell);
            }
        }

        for x in original_width..width {
            for y in original_height..height {
                assert_eq!(character_grid.get_cell(x, y).unwrap(), &default_cell!());
            }
        }
    }
}
