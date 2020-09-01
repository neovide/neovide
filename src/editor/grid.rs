use log::trace;
use std::sync::Arc;

use super::style::Style;

pub type GridCell = Option<(String, Option<Arc<Style>>)>;

pub struct CharacterGrid {
    pub width: u64,
    pub height: u64,
    pub should_clear: bool,

    dirty: Vec<bool>,
    characters: Vec<GridCell>,
}

impl CharacterGrid {
    pub fn new(size: (u64, u64)) -> CharacterGrid {
        let (width, height) = size;
        let cell_count = (width * height) as usize;
        CharacterGrid {
            characters: vec![None; cell_count],
            dirty: vec![true; cell_count],
            width,
            height,
            should_clear: false,
        }
    }

    pub fn resize(&mut self, width: u64, height: u64) {
        trace!("Editor resized");
        let new_cell_count = (width * height) as usize;
        let new_dirty = vec![false; new_cell_count];
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
        self.dirty = new_dirty;
        self.characters = new_characters;
    }

    pub fn clear(&mut self) {
        trace!("Editor cleared");
        self.set_characters_all(None);
        self.set_dirty_all(true);
        self.should_clear = true;
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

    pub fn is_dirty_cell(&self, x: u64, y: u64) -> bool {
        if let Some(idx) = self.cell_index(x, y) {
            self.dirty[idx]
        } else {
            false
        }
    }

    pub fn set_dirty_cell(&mut self, x: u64, y: u64) {
        if let Some(idx) = self.cell_index(x, y) {
            self.dirty[idx] = true;
        }
    }

    pub fn set_dirty_all(&mut self, value: bool) {
        self.dirty.clear();
        self.dirty
            .resize_with((self.width * self.height) as usize, || value);
    }

    pub fn set_characters_all(&mut self, value: GridCell) {
        self.characters.clear();
        self.characters
            .resize_with((self.width * self.height) as usize, || {
                value.as_ref().cloned()
            });
    }

    pub fn rows(&self) -> impl Iterator<Item = &[GridCell]> {
        (0..self.height).map(move |row| {
            &self.characters[(row * self.width) as usize..((row + 1) * self.width) as usize]
        })
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
        assert_eq!(character_grid.should_clear, true);
        assert_eq!(character_grid.characters, vec![None; context.area]);
        assert_eq!(character_grid.dirty, vec![true; context.area]);
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
    fn test_is_dirty_cell() {
        let context = Context::new();
        let mut character_grid = CharacterGrid::new(context.size);
        character_grid.dirty[context.index] = false;

        // RUN FUNCTION
        assert!(!character_grid.is_dirty_cell(context.x, context.y));
    }

    #[test]
    fn test_set_dirty_cell() {
        let context = Context::new();
        let mut character_grid = CharacterGrid::new(context.size);
        character_grid.dirty = vec![false; context.area];

        // RUN FUNCTION
        character_grid.set_dirty_cell(context.x, context.y);
        assert!(character_grid.dirty[context.index]);
    }

    #[test]
    fn test_set_dirty_all() {
        let context = Context::new();
        let mut character_grid = CharacterGrid::new(context.size);

        // RUN FUNCTION
        character_grid.set_dirty_all(false);
        assert_eq!(character_grid.dirty, vec![false; context.area]);
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
        character_grid.dirty = vec![false; context.area];
        character_grid.characters = vec![grid_cell.clone(); context.area];
        character_grid.should_clear = false;

        // RUN FUNCTION
        character_grid.clear();

        assert_eq!(character_grid.width, context.size.0);
        assert_eq!(character_grid.height, context.size.1);
        assert_eq!(character_grid.should_clear, true);
        assert_eq!(character_grid.characters, vec![None; context.area]);
        assert_eq!(character_grid.dirty, vec![true; context.area]);
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
        character_grid.dirty = vec![false; context.area];
        character_grid.characters = vec![grid_cell.clone(); context.area];
        character_grid.should_clear = false;

        // RUN FUNCTION
        character_grid.resize(width, height);

        assert_eq!(character_grid.width, width);
        assert_eq!(character_grid.height, height);
        assert_eq!(character_grid.should_clear, true);
        assert_eq!(character_grid.characters, vec![None; new_area]);
        assert_eq!(character_grid.dirty, vec![true; new_area]);
    }

    #[test]
    fn test_rows() {
        let context = Context::new();
        let character_grid = CharacterGrid::new(context.size);
        let mut end = 0;

        // RUN FUNCTION
        for (row_index, row) in character_grid.rows().enumerate() {
            assert_eq!(row.len(), context.size.0 as usize);
            end = row_index;
        }

        assert_eq!(end, (context.size.1 - 1) as usize);
    }
}
