use std::{collections::HashMap, ops::Range, sync::Arc};

use log::warn;

use crate::{
    bridge::GridLineCell,
    editor::{grid::CharacterGrid, style::Style, AnchorInfo, DrawCommand, DrawCommandBatcher},
    renderer::{box_drawing, WindowDrawCommand},
    units::{GridRect, GridSize},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WindowType {
    Editor,
    Message { scrolled: bool },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub text: String,
    fragments: Vec<LineFragmentData>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LineFragmentData {
    text_range: Range<u32>,
    style: Option<Arc<Style>>,
    cells: Range<u32>,
    words: Vec<WordData>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct WordData {
    text_offset: u32,
    cell: u32,
    cluster_sizes: Vec<u8>,
}

pub struct LineFragment<'a> {
    pub style: &'a Option<Arc<Style>>,
    pub text: &'a str,
    pub cells: &'a Range<u32>,

    data: &'a LineFragmentData,
}

pub struct Word<'a> {
    pub text: &'a str,
    pub cell: u32,

    cluster_sizes: &'a [u8],
}

impl Line {
    pub fn fragments(&self) -> impl Iterator<Item = LineFragment<'_>> {
        self.fragments.iter().map(|fragment| {
            let range = fragment.text_range.start as usize..fragment.text_range.end as usize;
            LineFragment {
                style: &fragment.style,
                text: &self.text[range],
                cells: &fragment.cells,
                data: fragment,
            }
        })
    }
}

impl LineFragment<'_> {
    pub fn words(&self) -> impl Iterator<Item = Word<'_>> {
        self.data.words.iter().map(|word| {
            let size: usize = word.cluster_sizes.iter().map(|v| *v as usize).sum();
            let cluster_sizes = &word.cluster_sizes;
            let start = word.text_offset as usize;
            let end = start + size;
            let text = &self.text[start..end];
            Word {
                text,
                cell: word.cell,
                cluster_sizes,
            }
        })
    }
}

impl<'a> Word<'a> {
    pub fn new(text: &'a str, cluster_sizes: &'a [u8]) -> Self {
        Self {
            text,
            cell: 0,
            cluster_sizes,
        }
    }

    pub fn grapheme_clusters(&self) -> impl Iterator<Item = (usize, &'a str)> + Clone {
        self.cluster_sizes
            .iter()
            .enumerate()
            .filter(|(_, size)| **size > 0)
            .scan(0, |current_pos, (cell_nr, size)| {
                let start = *current_pos;
                *current_pos += *size as u32;
                Some((cell_nr, &self.text[start as usize..*current_pos as usize]))
            })
    }
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

        let text = cell.text;
        if let Some(times) = cell.repeat {
            // Repeats of zero times should be ignored, they are mostly useful for terminal Neovim
            // to distinguish between empty lines and lines ending with spaces.
            if times == 0 {
                return;
            }

            for _ in 0..times.saturating_sub(1) {
                if let Some(cell) = self.grid.get_cell_mut(*column_pos, row_index) {
                    *cell = (text.clone(), style.clone());
                }
                *column_pos += 1;
            }
        };
        if let Some(cell) = self.grid.get_cell_mut(*column_pos, row_index) {
            *cell = (text, style.clone());
        }
        *column_pos += 1;

        *previous_style = style;
    }

    // Build a line fragment for the given row starting from current_start up until the next style
    // change or double width character.
    fn build_line_fragment(
        &self,
        row_index: usize,
        start: usize,
        text: &mut String,
    ) -> (usize, LineFragmentData) {
        let row = self.grid.row(row_index).unwrap();

        let (_, style) = &row[start];

        let mut width = 0u32;
        let mut last_box_char = None;
        let mut text_range = text.len() as u32..text.len() as u32;
        let mut words = Vec::new();
        let mut current_word = WordData::default();

        for (cluster, possible_end_style) in row.iter().take(self.grid.width).skip(start) {
            // Style doesn't match. Draw what we've got.
            if style != possible_end_style {
                break;
            }

            // Box drawing characters are rendered specially; break up the segment such that
            // repeated box drawing characters are in a segment by themselves
            if box_drawing::is_box_char(cluster) {
                if text_range.is_empty() {
                    last_box_char = Some(cluster)
                }
                if (!text_range.is_empty() && last_box_char.is_none())
                    || last_box_char != Some(cluster)
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

            // We can't deal with clusters that are longer than 255 bytes, so replace them with spaces.
            // This should only happen if Neovim sends corrupted lines, or maybe in some pathological
            // Unicode combining sequence.
            let cluster = if cluster.len() > 255 { " " } else { cluster };

            if cluster.is_empty() {
                // For double-width char, the empty cell should be part of the current word as a 0 cluster size
                // Or ignored when it's part of whitespace
                if !current_word.cluster_sizes.is_empty() {
                    current_word.cluster_sizes.push(0);
                }
                continue;
            }

            let is_whitespace = cluster
                .chars()
                .next()
                .is_some_and(|char| char.is_whitespace());
            if is_whitespace {
                if !current_word.cluster_sizes.is_empty() {
                    // Finish the current word
                    words.push(current_word);
                    current_word = WordData::default();
                }
            } else if current_word.cluster_sizes.is_empty() {
                // Properly initialize a new word
                current_word.cell = width - 1;
                current_word.cluster_sizes.push(cluster.len() as u8);
                current_word.text_offset = text.len() as u32 - text_range.start;
            } else {
                current_word.cluster_sizes.push(cluster.len() as u8);
            }

            // Add the grid cell to the cells to render.
            text.push_str(cluster);
            text_range.end += cluster.len() as u32;
        }
        if !current_word.cluster_sizes.is_empty() {
            words.push(current_word);
        }

        let line_fragment = LineFragmentData {
            text_range,
            cells: start as u32..start as u32 + width,
            style: style.clone(),
            words,
        };

        (start + width as usize, line_fragment)
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

    pub fn draw_centered_text_line(
        &mut self,
        batcher: &mut DrawCommandBatcher,
        row: usize,
        text: &str,
    ) {
        if row >= self.grid.height {
            return;
        }

        for column in 0..self.grid.width {
            if let Some(cell) = self.grid.get_cell_mut(column, row) {
                *cell = (" ".to_string(), None);
            }
        }

        if text.is_empty() {
            self.redraw_line(batcher, row);
            return;
        }

        let text_width = text.chars().count();
        let start_column = if text_width >= self.grid.width {
            0
        } else {
            (self.grid.width - text_width) / 2
        };

        for (offset, ch) in text.chars().enumerate() {
            let column = start_column + offset;
            if column >= self.grid.width {
                break;
            }

            if let Some(cell) = self.grid.get_cell_mut(column, row) {
                *cell = (ch.to_string(), None);
            }
        }

        self.redraw_line(batcher, row);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::style::{Colors, Style};
    use skia_safe::{colors, Color4f};
    use std::sync::Arc;

    fn make_style(color: Color4f) -> Arc<Style> {
        Arc::new(Style::new(Colors {
            foreground: Some(color),
            background: Some(Color4f::new(0.0, 0.0, 0.0, 1.0)),
            special: Some(Color4f::new(0.0, 0.0, 0.0, 1.0)),
        }))
    }

    fn make_window<const WIDTH: usize, const HEIGHT: usize>(
        rows: [[(&str, Option<Color4f>); WIDTH]; HEIGHT],
    ) -> Window {
        let mut grid = CharacterGrid::new((WIDTH, HEIGHT));
        for (y, row) in rows.iter().enumerate() {
            for (x, (ch, color)) in row.iter().enumerate() {
                *grid.get_cell_mut(x, y).unwrap() = (ch.to_string(), color.map(make_style));
            }
        }
        Window {
            grid_id: 1,
            grid,
            window_type: WindowType::Editor,
            anchor_info: None,
            grid_position: (0.0, 0.0),
        }
    }

    #[test]
    fn test_build_line_fragment_macro_basic() {
        let window = make_window([[
            ("a", Some(colors::RED)),
            ("b", Some(colors::RED)),
            ("c", Some(colors::RED)),
        ]]);

        let mut text = String::new();
        let (next_start, fragment) = window.build_line_fragment(0, 0, &mut text);

        assert_eq!(next_start, 3);
        assert_eq!(fragment.cells, 0..3);
        assert_eq!(fragment.text_range, 0..3);
        assert_eq!(fragment.style, Some(make_style(colors::RED)));
        assert_eq!(fragment.words.len(), 1);

        // There should be no more fragments
        assert_eq!(next_start, window.grid.width);
    }

    #[test]
    fn test_build_line_fragment_macro_style_change() {
        let window = make_window([[
            ("x", Some(colors::RED)),
            ("y", Some(colors::GREEN)),
            ("z", Some(colors::GREEN)),
        ]]);

        let mut text = String::new();
        let (next_start, fragment) = window.build_line_fragment(0, 0, &mut text);

        assert_eq!(next_start, 1);
        assert_eq!(fragment.cells, 0..1);
        assert_eq!(fragment.text_range, 0..1);
        assert_eq!(fragment.style, Some(make_style(colors::RED)));
        assert_eq!(fragment.words.len(), 1);

        let (next_start, fragment) = window.build_line_fragment(0, next_start, &mut text);
        assert_eq!(next_start, 3);
        assert_eq!(fragment.cells, 1..3);
        assert_eq!(fragment.text_range, 1..3);
        assert_eq!(fragment.style, Some(make_style(colors::GREEN)));
        assert_eq!(fragment.words.len(), 1);

        // All fragments should be covered
        assert_eq!(next_start, window.grid.width);
    }

    #[test]
    fn test_build_line_fragment_box_chars() {
        // Use Unicode box drawing char ─ (U+2500)
        let window = make_window([[
            ("─", Some(colors::BLUE)),
            ("─", Some(colors::BLUE)),
            ("│", Some(colors::BLUE)),
        ]]);

        let mut text = String::new();
        let (next_start, fragment) = window.build_line_fragment(0, 0, &mut text);

        // Should group the two ─ chars together, then break for │
        assert_eq!(next_start, 2);
        assert_eq!(fragment.cells, 0..2);
        assert_eq!(fragment.text_range, 0..6);
        assert_eq!(fragment.style, Some(make_style(colors::BLUE)));
        assert_eq!(fragment.words.len(), 1);

        let (next_start, fragment) = window.build_line_fragment(0, next_start, &mut text);
        assert_eq!(next_start, 3);
        assert_eq!(fragment.cells, 2..3);
        assert_eq!(fragment.text_range, 6..9);
        assert_eq!(fragment.style, Some(make_style(colors::BLUE)));
        assert_eq!(fragment.words.len(), 1);

        assert_eq!(next_start, window.grid.width);
    }

    #[test]
    fn test_build_line_fragment_multiple_words_with_spaces() {
        // "  foo bar  baz"
        let window = make_window([[
            (" ", Some(colors::GREEN)),
            (" ", Some(colors::GREEN)),
            ("f", Some(colors::GREEN)),
            ("o", Some(colors::GREEN)),
            ("o", Some(colors::GREEN)),
            (" ", Some(colors::GREEN)),
            ("b", Some(colors::GREEN)),
            ("a", Some(colors::GREEN)),
            ("r", Some(colors::GREEN)),
            (" ", Some(colors::GREEN)),
            (" ", Some(colors::GREEN)),
            ("b", Some(colors::GREEN)),
            ("a", Some(colors::GREEN)),
            ("z", Some(colors::GREEN)),
        ]]);

        let mut text = String::new();
        let (next_start, fragment) = window.build_line_fragment(0, 0, &mut text);

        assert_eq!(next_start, 14);
        assert_eq!(fragment.cells, 0..14);
        assert_eq!(fragment.text_range, 0..14);
        assert_eq!(fragment.style, Some(make_style(colors::GREEN)));
        assert_eq!(text, "  foo bar  baz");

        // There should be three words: "foo", "bar", "baz"
        assert_eq!(fragment.words.len(), 3);

        // "foo"
        assert_eq!(fragment.words[0].cell, 2);
        assert_eq!(fragment.words[0].text_offset, 2);
        assert_eq!(fragment.words[0].cluster_sizes, vec![1, 1, 1]);

        // "bar"
        assert_eq!(fragment.words[1].cell, 6);
        assert_eq!(fragment.words[1].text_offset, 6);
        assert_eq!(fragment.words[1].cluster_sizes, vec![1, 1, 1]);

        // "baz"
        assert_eq!(fragment.words[2].cell, 11);
        assert_eq!(fragment.words[2].text_offset, 11);
        assert_eq!(fragment.words[2].cluster_sizes, vec![1, 1, 1]);

        assert_eq!(next_start, window.grid.width);
    }

    #[test]
    fn test_build_line_fragment_double_width() {
        // U+4E00 is a common double-width CJK char (一)
        // Double width: char, then empty string, both with same style
        let window = make_window([[
            ("一", Some(colors::RED)),
            ("", Some(colors::RED)),
            ("c", Some(colors::RED)),
        ]]);

        let mut text = String::new();
        let (next_start, fragment) = window.build_line_fragment(0, 0, &mut text);

        // The fragment should include all three cells (0..3), text range 0..4 ("一" is 3 bytes, "c" is 1)
        assert_eq!(next_start, 3);
        assert_eq!(fragment.cells, 0..3);
        assert_eq!(fragment.text_range, 0..4);
        assert_eq!(fragment.style, Some(make_style(colors::RED)));
        assert_eq!(fragment.words.len(), 1); // "一c" is a single word
        assert_eq!(text, "一c");

        assert_eq!(next_start, window.grid.width);
    }

    #[test]
    fn test_build_line_fragment_double_width_space_between_words() {
        // "a", double-width space (U+3000), "", "b"
        let window = make_window([[
            ("a", Some(colors::RED)),
            ("　", Some(colors::RED)), // U+3000 IDEOGRAPHIC SPACE
            ("", Some(colors::RED)),
            ("b", Some(colors::RED)),
        ]]);

        let mut text = String::new();
        let (next_start, fragment) = window.build_line_fragment(0, 0, &mut text);

        // The fragment should include all four cells (0..4), text range 0..5 ("a" 1, "　" 3, "b" 1)
        assert_eq!(next_start, 4);
        assert_eq!(fragment.cells, 0..4);
        assert_eq!(fragment.text_range, 0..5);
        assert_eq!(fragment.style, Some(make_style(colors::RED)));
        assert_eq!(text, "a　b");

        // The words should be ["a"] and ["b"], the double-width space and its empty cell are not part of any word
        assert_eq!(fragment.words.len(), 2);
        // First word: "a"
        assert_eq!(fragment.words[0].cell, 0);
        assert_eq!(fragment.words[0].text_offset, 0);
        assert_eq!(fragment.words[0].cluster_sizes, vec![1]);
        // Second word: "b"
        assert_eq!(fragment.words[1].cell, 3);
        assert_eq!(fragment.words[1].text_offset, 4);
        assert_eq!(fragment.words[1].cluster_sizes, vec![1]);

        assert_eq!(next_start, window.grid.width);
    }

    #[test]
    fn test_build_line_fragment_double_width_in_middle_of_word() {
        // "a", "一" (double-width), "", "b"
        let window = make_window([[
            ("a", Some(colors::RED)),
            ("一", Some(colors::RED)),
            ("", Some(colors::RED)),
            ("b", Some(colors::RED)),
        ]]);

        let mut text = String::new();
        let (next_start, fragment) = window.build_line_fragment(0, 0, &mut text);

        // The fragment should include all four cells (0..4), text range 0..5 ("a" 1, "一" 3, "b" 1)
        assert_eq!(next_start, 4);
        assert_eq!(fragment.cells, 0..4);
        assert_eq!(fragment.text_range, 0..5);
        assert_eq!(fragment.style, Some(make_style(colors::RED)));
        assert_eq!(text, "a一b");

        // The word should be ["a一b"], with cluster_sizes [1, 3, 0, 1]
        assert_eq!(fragment.words.len(), 1);
        assert_eq!(fragment.words[0].cell, 0);
        assert_eq!(fragment.words[0].text_offset, 0);
        assert_eq!(fragment.words[0].cluster_sizes, vec![1, 3, 0, 1]);

        assert_eq!(next_start, window.grid.width);
    }
}
