use std::collections::HashMap;

use crate::bridge::GridLineCell;

use super::{window::Window, DrawCommandBatcher};

const INTRO_HEADER_PREFIX: &str = "NVIM ";
const INTRO_FINAL_LINE: &str = "type  :help Kuwasha<Enter>    for information";
const SPONSOR_MESSAGE_LINES: &[&str] =
    &["", "Sponsor Neovide: https://github.com/sponsors/neovide"];

#[derive(Default)]
pub(crate) struct IntroMessageExtender {
    per_grid: HashMap<u64, IntroState>,
    sponsor_allowed: bool,
}

#[derive(Default)]
struct IntroState {
    saw_intro_header: bool,
    banner_start_row: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IntroProcessing {
    Skip,
    Process,
    ClearBanner,
}

#[derive(Clone, Copy)]
enum IntroLineKind {
    Blank,
    Filler,
    HeaderCandidate,
    Other,
}

impl IntroState {
    fn mark_intro_possible(&mut self) {
        self.saw_intro_header = true;
    }

    fn remember_injection(&mut self, row: u64) {
        self.saw_intro_header = false;
        self.banner_start_row = Some(row);
    }

    fn has_visible_banner(&self) -> bool {
        self.banner_start_row.is_some()
    }

    fn take_banner_start_row(&mut self) -> Option<u64> {
        self.banner_start_row.take()
    }
}

impl IntroMessageExtender {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn reset(&mut self, grid: u64) {
        self.per_grid.remove(&grid);
    }

    pub(crate) fn preprocess_line(&mut self, grid: u64, cells: &[GridLineCell]) -> IntroProcessing {
        let state = self.per_grid.entry(grid).or_default();

        if state.saw_intro_header {
            return IntroProcessing::Process;
        }

        if state.has_visible_banner() {
            return IntroProcessing::ClearBanner;
        }

        match classify_intro_line_start(cells) {
            IntroLineKind::Blank | IntroLineKind::Filler => IntroProcessing::Skip,
            IntroLineKind::HeaderCandidate => IntroProcessing::Process,
            IntroLineKind::Other => IntroProcessing::Skip,
        }
    }

    pub(crate) fn banner_injection_row(
        &mut self,
        grid: u64,
        row: u64,
        line_text: &str,
    ) -> Option<u64> {
        if !self.sponsor_allowed {
            return None;
        }

        let state = self.per_grid.entry(grid).or_default();

        if !state.saw_intro_header && !is_intro_header(line_text) {
            return None;
        } else if !state.saw_intro_header {
            state.mark_intro_possible();
        }

        if !is_neovim_intro_final_line_text(line_text) {
            return None;
        }

        if state.has_visible_banner() {
            return None;
        }

        let sponsor_start_row = row + 1;
        Some(sponsor_start_row)
    }

    pub(crate) fn inject_banner(
        &mut self,
        grid: u64,
        window: &mut Window,
        sponsor_start_row: u64,
        batcher: &mut DrawCommandBatcher,
    ) {
        for (offset, line) in SPONSOR_MESSAGE_LINES.iter().enumerate() {
            let target_row = sponsor_start_row + offset as u64;
            if target_row >= window.get_height() {
                break;
            }

            window.draw_centered_text_line(batcher, target_row as usize, line);
        }

        if let Some(state) = self.per_grid.get_mut(&grid) {
            state.remember_injection(sponsor_start_row);
        }
    }

    pub(crate) fn maybe_hide_banner(
        &mut self,
        grid: u64,
        windows: &mut HashMap<u64, Window>,
        batcher: &mut DrawCommandBatcher,
    ) {
        if let Some(state) = self.per_grid.get_mut(&grid) {
            if let Some(start_row) = state.take_banner_start_row() {
                if let Some(window) = windows.get_mut(&grid) {
                    clear_banner_rows(window, start_row, batcher);
                }
            }
        }
    }

    pub(crate) fn set_sponsor_allowed(
        &mut self,
        allowed: bool,
        windows: &mut HashMap<u64, Window>,
        batcher: &mut DrawCommandBatcher,
    ) {
        if self.sponsor_allowed == allowed {
            return;
        }

        self.sponsor_allowed = allowed;
        if !allowed {
            let grids: Vec<u64> = self.per_grid.keys().copied().collect();
            for grid in grids {
                self.maybe_hide_banner(grid, windows, batcher);
            }
        }
    }

    pub(crate) fn sponsor_allowed(&self) -> bool {
        self.sponsor_allowed
    }
}

fn is_intro_header(text: &str) -> bool {
    text.trim_start().starts_with(INTRO_HEADER_PREFIX)
}

fn is_neovim_intro_final_line_text(text: &str) -> bool {
    text.trim() == INTRO_FINAL_LINE
}

fn classify_intro_line_start(cells: &[GridLineCell]) -> IntroLineKind {
    match first_non_whitespace_char(cells) {
        None => IntroLineKind::Blank,
        Some('~') => IntroLineKind::Filler,
        Some('N') => IntroLineKind::HeaderCandidate,
        _ => IntroLineKind::Other,
    }
}

fn first_non_whitespace_char(cells: &[GridLineCell]) -> Option<char> {
    for cell in cells {
        let repeat = cell.repeat.unwrap_or(1);
        for _ in 0..repeat {
            for ch in cell.text.chars() {
                if ch.is_whitespace() {
                    continue;
                }
                return Some(ch);
            }
        }
    }
    None
}

fn clear_banner_rows(window: &mut Window, start_row: u64, batcher: &mut DrawCommandBatcher) {
    for offset in 0..SPONSOR_MESSAGE_LINES.len() {
        let row = start_row + offset as u64;
        if row >= window.get_height() {
            break;
        }
        window.draw_centered_text_line(batcher, row as usize, "");
    }
}
