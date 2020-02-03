use std::sync::Arc;
use log::trace;

use super::style::Style;

type GridCell = Option<(String, Option<Arc<Style>>)>;

pub struct CharacterGrid {
    pub characters: Vec<GridCell>,
    pub width: u64,
    pub height: u64,
    pub should_clear: bool,

    dirty: Vec<bool>,
}

impl CharacterGrid {
    pub fn new() -> CharacterGrid {
        CharacterGrid {
            characters: vec![],
            dirty: vec![],
            width: 0,
            height: 0,
            should_clear: true,
        }
    }

    pub fn resize(&mut self, new_size: (u64, u64)) {
        trace!("Editor resized");
        self.width = new_size.0;
        self.height = new_size.1;
        self.clear();
    }

    pub fn clear(&mut self) {
        trace!("Editor cleared");
        let cell_count = (self.width * self.height) as usize;
        self.characters = vec![None; cell_count];
        self.dirty = vec![true; cell_count];
        self.should_clear = true;
    }

    pub fn cell_index(&self, x: u64, y: u64) -> Option<usize> {
        if x >= self.width || y >= self.height {
            None
        } else {
            Some((x + y * self.width) as usize)
        }
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
        self.dirty.resize(self.dirty.len(), value);
    }

    pub fn rows<'a>(&'a self) -> Vec<&'a [GridCell]> {
        (0..self.height)
            .map(|row| {
                &self.characters[(row * self.width) as usize..((row + 1) * self.width) as usize]
            })
            .collect()
    }
}
