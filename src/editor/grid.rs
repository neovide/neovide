use std::sync::Arc;

use super::style::Style;

pub type GridCell = Option<(String, Option<Arc<Style>>)>;

pub struct CharacterGrid {
    pub width: u64,
    pub height: u64,

    characters: Vec<GridCell>,
}

impl CharacterGrid {
    pub fn new(size: (u64, u64)) -> CharacterGrid {
        let (width, height) = size;
        let cell_count = (width * height) as usize;
        CharacterGrid {
            characters: vec![None; cell_count],
            width,
            height,
        }
    }

    pub fn resize(&mut self, width: u64, height: u64) {
        let new_cell_count = (width * height) as usize;
        let default_cell: GridCell = None;
        let mut new_characters = vec![default_cell; new_cell_count];

        for x in 0..self.width.min(width) {
            for y in 0..self.height.min(height) {
                if let Some(existing_cell) = self.get_cell(x, y) {
                    new_characters[(x + y * width) as usize] = existing_cell.clone();
                }
            }
        }

        self.width = width;
        self.height = height;
        self.characters = new_characters;
    }

    pub fn clear(&mut self) {
        self.set_characters_all(None);
    }

    fn cell_index(&self, x: u64, y: u64) -> Option<usize> {
        if x >= self.width || y >= self.height {
            None
        } else {
            Some((x + y * self.width) as usize)
        }
    }

    pub fn get_cell(&self, x: u64, y: u64) -> Option<&GridCell> {
        self.cell_index(x, y).map(|idx| &self.characters[idx])
    }

    pub fn get_cell_mut(&mut self, x: u64, y: u64) -> Option<&mut GridCell> {
        self.cell_index(x, y)
            .map(move |idx| &mut self.characters[idx])
    }

    pub fn set_characters_all(&mut self, value: GridCell) {
        self.characters.clear();
        self.characters
            .resize_with((self.width * self.height) as usize, || {
                value.as_ref().cloned()
            });
    }

    pub fn row(&self, row_index: u64) -> Option<&[GridCell]> {
        if row_index < self.height {
            Some(&self.characters[(row_index * self.width) as usize..((row_index + 1) * self.width) as usize])
        } else {
            None
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
        size: (u64, u64),
        x: u64,
        y: u64,
        area: usize,
        index: usize,
    }

    impl Context {
        fn new() -> Self {
            let size = (
                (thread_rng().gen::<u64>() % 500) + 1,
                (thread_rng().gen::<u64>() % 500) + 1,
            );
            let (x, y) = (
                thread_rng().gen::<u64>() % size.0,
                thread_rng().gen::<u64>() % size.1,
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
                area: (size.0 * size.1) as usize,
                index: (x + y * size.0) as usize,
            }
        }
    }

    #[test]
    fn test_new() {
        let context = Context::new();

        // RUN FUNCTION
        let character_grid = CharacterGrid::new(context.size);
        assert_eq!(character_grid.width, context.size.0);
        assert_eq!(character_grid.height, context.size.1);
        assert_eq!(character_grid.characters, vec![None; context.area]);
    }

    #[test]
    fn test_get_cell() {
        let context = Context::new();
        let mut character_grid = CharacterGrid::new(context.size);

        character_grid.characters[context.index] = Some((
            "foo".to_string(),
            Some(Arc::new(Style::new(context.none_colors.clone()))),
        ));
        let result = (
            "foo".to_string(),
            Some(Arc::new(Style::new(context.none_colors.clone()))),
        );

        // RUN FUNCTION
        assert_eq!(
            character_grid
                .get_cell(context.x, context.y)
                .unwrap()
                .as_ref()
                .unwrap(),
            &result
        );
    }

    #[test]
    fn test_get_cell_mut() {
        let context = Context::new();
        let mut character_grid = CharacterGrid::new(context.size);

        character_grid.characters[context.index] = Some((
            "foo".to_string(),
            Some(Arc::new(Style::new(context.none_colors.clone()))),
        ));
        let result = (
            "bar".to_string(),
            Some(Arc::new(Style::new(context.none_colors.clone()))),
        );

        // RUN FUNCTION
        let cell = character_grid.get_cell_mut(context.x, context.y).unwrap();
        *cell = Some((
            "bar".to_string(),
            Some(Arc::new(Style::new(context.none_colors.clone()))),
        ));

        assert_eq!(
            character_grid
                .get_cell_mut(context.x, context.y)
                .unwrap()
                .as_ref()
                .unwrap(),
            &result
        );
    }

    #[test]
    fn test_set_characters_all() {
        let context = Context::new();
        let grid_cell = Some((
            "foo".to_string(),
            Some(Arc::new(Style::new(context.none_colors))),
        ));
        let mut character_grid = CharacterGrid::new(context.size);

        // RUN FUNCTION
        character_grid.set_characters_all(grid_cell.clone());
        assert_eq!(
            character_grid.characters,
            vec![grid_cell.clone(); context.area]
        );
    }

    #[test]
    fn test_clear() {
        let context = Context::new();
        let mut character_grid = CharacterGrid::new(context.size);

        let grid_cell = Some((
            "foo".to_string(),
            Some(Arc::new(Style::new(context.none_colors))),
        ));
        character_grid.characters = vec![grid_cell.clone(); context.area];

        // RUN FUNCTION
        character_grid.clear();

        assert_eq!(character_grid.width, context.size.0);
        assert_eq!(character_grid.height, context.size.1);
        assert_eq!(character_grid.characters, vec![None; context.area]);
    }

    #[test]
    fn test_resize() {
        let context = Context::new();
        let mut character_grid = CharacterGrid::new(context.size);
        let (width, height) = (
            (thread_rng().gen::<u64>() % 500) + 1,
            (thread_rng().gen::<u64>() % 500) + 1,
        );
        let new_area = (width * height) as usize;

        let grid_cell = Some((
            "foo".to_string(),
            Some(Arc::new(Style::new(context.none_colors))),
        ));
        character_grid.characters = vec![grid_cell.clone(); context.area];

        // RUN FUNCTION
        character_grid.resize(width, height);

        assert_eq!(character_grid.width, width);
        assert_eq!(character_grid.height, height);
        assert_eq!(character_grid.characters, vec![grid_cell.clone(); new_area]);
    }
}
