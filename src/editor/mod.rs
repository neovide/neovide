mod cursor;
mod grid;
mod style;
mod window;

use std::collections::{HashSet, HashMap};
use std::sync::Arc;

use log::{trace, error};
use parking_lot::Mutex;
use skulpin::skia_safe::colors;

use crate::bridge::{EditorMode, GuiOption, RedrawEvent, WindowAnchor};
use crate::redraw_scheduler::REDRAW_SCHEDULER;
pub use window::*;
pub use cursor::{Cursor, CursorMode, CursorShape};
pub use grid::CharacterGrid;
pub use style::{Colors, Style};

lazy_static! {
    pub static ref EDITOR: Arc<Mutex<Editor>> = Arc::new(Mutex::new(Editor::new()));
}

pub struct RenderInfo {
    windows: Vec<WindowRenderInfo>,
    closed_window_ids: Vec<u64>,
}

pub struct WindowRenderInfo {
    pub grid_id: u64,
    pub grid_position: (u64, u64),
    pub width: u64,
    pub height: u64,
    pub should_clear: bool,
    pub draw_commands: Vec<DrawCommand>,
    pub child_windows: Vec<WindowRenderInfo>
}

pub struct Editor {
    pub title: String,
    pub windows: HashMap<u64, Window>,
    pub closed_window_ids: HashSet<u64>,
    pub mouse_enabled: bool,
    pub guifont: Option<String>,
    pub cursor: Cursor,
    pub default_style: Arc<Style>,
    pub defined_styles: HashMap<u64, Arc<Style>>,
    pub previous_style: Option<Arc<Style>>,
    pub mode_list: Vec<CursorMode>,
    pub current_mode: EditorMode,
}

impl Editor {
    pub fn new() -> Editor {
        Editor {
            title: "Neovide".to_string(),
            windows: HashMap::new(),
            closed_window_ids: HashSet::new(),
            mouse_enabled: true,
            guifont: None,
            cursor: Cursor::new(),
            default_style: Arc::new(Style::new(Colors::new(
                Some(colors::WHITE),
                Some(colors::BLACK),
                Some(colors::GREY),
            ))),
            defined_styles: HashMap::new(),
            previous_style: None,
            mode_list: Vec::new(),
            current_mode: EditorMode::Unknown(String::from("")),
        }
    }

    pub fn handle_redraw_event(&mut self, event: RedrawEvent) {
        match event {
            RedrawEvent::SetTitle { title } => self.title = title,
            RedrawEvent::ModeInfoSet { cursor_modes } => self.mode_list = cursor_modes,
            RedrawEvent::OptionSet { gui_option } => self.set_option(gui_option),
            RedrawEvent::ModeChange { mode, mode_index } => {
                if let Some(cursor_mode) = self.mode_list.get(mode_index as usize) {
                    self.cursor.change_mode(cursor_mode, &self.defined_styles);
                    self.current_mode = mode
                }
            }
            RedrawEvent::MouseOn => {
                self.mouse_enabled = true;
            }
            RedrawEvent::MouseOff => {
                self.mouse_enabled = false;
            }
            RedrawEvent::BusyStart => {
                trace!("Cursor off");
                self.cursor.enabled = false;
            }
            RedrawEvent::BusyStop => {
                trace!("Cursor on");
                self.cursor.enabled = true;
            }
            RedrawEvent::Flush => {
                trace!("Image flushed");
                REDRAW_SCHEDULER.queue_next_frame();
            }
            RedrawEvent::DefaultColorsSet { colors } => {
                self.default_style = Arc::new(Style::new(colors))
            }
            RedrawEvent::HighlightAttributesDefine { id, style } => {
                self.defined_styles.insert(id, Arc::new(style));
            }
            RedrawEvent::CursorGoto { grid, row, column } => self.set_cursor_position(grid, row, column),
            RedrawEvent::Resize { grid, width, height } => {
                self.windows.get_mut(&grid).map(|window| window.resize(width, height));
            },
            RedrawEvent::GridLine {
                grid,
                row,
                column_start,
                cells
            } => {
                self.windows.get_mut(&grid).map(|window| window.draw_grid_line(row, column_start, cells, &self.defined_styles, &mut self.previous_style));
            },
            RedrawEvent::Clear { grid } => {
                self.windows.get_mut(&grid).map(|window| window.grid.clear());
            },
            RedrawEvent::Scroll {
                grid,
                top,
                bottom,
                left,
                right,
                rows,
                columns
            } => {
                self.windows.get_mut(&grid).map(|window| window.scroll_region(top, bottom, left, right, rows, columns));
            },
            RedrawEvent::WindowPosition { grid, window, start_row, start_column, width, height } => self.set_window_position(grid, window, start_row, start_column, width, height),
            RedrawEvent::WindowFloatPosition { grid, window, anchor, anchor_grid, anchor_row, anchor_column, .. } => self.set_window_float_position(grid, window, anchor_grid, anchor, anchor_row, anchor_column),
            RedrawEvent::WindowHide { grid } => {
                self.windows.get_mut(&grid).map(|window| window.hidden = true);
            },
            RedrawEvent::WindowClose { grid } => self.close_window(grid),
            _ => {}
        };
    }

    fn close_window(&mut self, grid: u64) {
        self.windows.remove(&grid);
        self.closed_window_ids.insert(grid);
    }

    fn set_window_position(&mut self, grid: u64, window_id: u64, start_row: u64, start_column: u64, width: u64, height: u64) {
        match self.windows.get_mut(&grid) {
            Some(window) => {
                window.hidden = false;
                window.anchor_grid_id = None;
                window.anchor_type = WindowAnchor::NorthWest;
                window.anchor_row = start_row;
                window.anchor_column = start_column;
                window.resize(width, height);
            },
            None => {
                let new_window = Window::new(window_id, grid, width, height, None, WindowAnchor::NorthWest, start_row, start_column);
                self.windows.insert(grid, new_window);
            }
        }
    }

    fn set_window_float_position(&mut self, grid: u64, window_id: u64, anchor_grid: u64, anchor_type: WindowAnchor, anchor_row: u64, anchor_column: u64) {
        if let Some(window) = self.windows.get_mut(&grid) {
            window.hidden = false;
            window.anchor_grid_id = Some(anchor_grid);
            window.anchor_type = anchor_type;
            window.anchor_row = anchor_row;
            window.anchor_column = anchor_column;
        } else {
            error!("Attempted to float window that does not exist.");
        }

        if let Some(anchor_window) = self.windows.get_mut(&anchor_grid) {
            anchor_window.children.insert(grid);
        }
    }

    fn get_window_top_left(&self, grid: u64) -> Option<(u64, u64)> {
        let window = self.windows.get(&grid)?;

        match window.anchor_grid_id {
            Some(anchor_grid) => {
                let (parent_anchor_row, parent_anchor_column) = self.get_window_top_left(anchor_grid)?;
                match window.anchor_type {
                    WindowAnchor::NorthWest => {
                        Some((parent_anchor_row + window.anchor_row, parent_anchor_column + window.anchor_column))
                    },
                    WindowAnchor::NorthEast => {
                        Some((parent_anchor_row + window.anchor_row, parent_anchor_column + window.anchor_column - window.grid.width))
                    },
                    WindowAnchor::SouthWest => {
                        Some((parent_anchor_row + window.anchor_row - window.grid.height, parent_anchor_column + window.anchor_column))
                    },
                    WindowAnchor::SouthEast => {
                        Some((parent_anchor_row + window.anchor_row - window.grid.height, parent_anchor_column + window.anchor_column - window.grid.width))
                    },
                }
            },
            None => Some((window.anchor_row, window.anchor_column))
        }
    }

    fn set_cursor_position(&self, grid: u64, row: u64, column: u64) {
        match self.get_window_top_left(grid) {
            Some((window_row, window_column)) => {
                self.cursor.position = (window_row + row, window_column + column);

                if let Some(window) = self.windows.get(&grid) {
                    self.cursor.character = match window.grid.get_cell(column, row) {
                        Some(Some((character, _))) => character.clone(),
                        _ => ' '.to_string(),
                    };

                    self.cursor.double_width = match window.grid.get_cell(column + 1, row) {
                        Some(Some((character, _))) => character.is_empty(),
                        _ => false,
                    };
                }
            },
            None => {
                self.cursor.position = (row, column);
                self.cursor.double_width = false;
                self.cursor.character = " ".to_string();
            }
        }
    }

    fn set_option(&mut self, gui_option: GuiOption) {
        trace!("Option set {:?}", &gui_option);
        if let GuiOption::GuiFont(guifont) = gui_option {
            self.guifont = Some(guifont);
        }
    }

    fn build_window_render_info(&mut self, grid: u64) -> Option<WindowRenderInfo> {
        let grid_position = self.get_window_top_left(grid)?;
        let (draw_commands, should_clear) = {
            let mut window = self.windows.get_mut(&grid)?;
            window.build_draw_commands()
        };

        let window = self.windows.get(&grid)?;
        let child_windows = window.children.iter().filter_map(|child_id| self.build_window_render_info(*child_id)).collect();

        Some(WindowRenderInfo {
            grid_id: grid,
            grid_position,
            width: window.grid.width,
            height: window.grid.height,
            should_clear,
            draw_commands,
            child_windows
        })
    }

    pub fn build_render_info(&mut self) -> RenderInfo {
        let mut windows = Vec::new();

        for window in self.windows.values() {
            if !window.hidden && window.anchor_grid_id.is_none() {
                if let Some(window_render_info) = self.build_window_render_info(window.grid_id) {
                    windows.push(window_render_info);
                }
            }
        }

        let closed_window_ids = self.closed_window_ids.iter().copied().collect();
        self.closed_window_ids.clear();

        RenderInfo {
            windows, closed_window_ids
        }
    }
}
