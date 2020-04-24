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
        let width = size.0;
        let height = size.1;
        let cell_count = (width * height) as usize;
        CharacterGrid {
            characters: vec![None; cell_count],
            dirty: vec![true; cell_count],
            width,
            height,
            should_clear: true,
        }
    }

    pub fn resize(&mut self, width: u64, height: u64) {
        trace!("Editor resized");
        self.width = width;
        self.height = height;
        self.clear();
    }

    pub fn clear(&mut self) {
        trace!("Editor cleared");
        self.characters.clear();
        self.dirty.clear();

        let cell_count = (self.width * self.height) as usize;
        self.characters.resize_with(cell_count, || None);
        self.dirty.resize_with(cell_count, || true);
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

    pub fn rows(&self) -> impl Iterator<Item = &[GridCell]> {
        (0..self.height).map(move |row| {
            &self.characters[(row * self.width) as usize..((row + 1) * self.width) as usize]
        })
    }
}
