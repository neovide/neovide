mod cursor;
mod draw_command_batcher;
mod grid;
mod style;
mod window;

use std::{collections::HashMap, rc::Rc, sync::Arc, thread};

use log::{error, trace};

use crate::{
    bridge::{GuiOption, ParallelCommand, RedrawEvent, UiCommand, WindowAnchor},
    event_aggregator::EVENT_AGGREGATOR,
    profiling::tracy_zone,
    redraw_scheduler::REDRAW_SCHEDULER,
    renderer::DrawCommand,
    window::WindowCommand,
};

pub use cursor::{Cursor, CursorMode, CursorShape};
pub use draw_command_batcher::DrawCommandBatcher;
pub use grid::CharacterGrid;
pub use style::{Colors, Style, UnderlineStyle};
pub use window::*;

const MODE_CMDLINE: u64 = 4;

#[derive(Clone, Debug)]
pub struct AnchorInfo {
    pub anchor_grid_id: u64,
    pub anchor_type: WindowAnchor,
    pub anchor_left: f64,
    pub anchor_top: f64,
    pub sort_order: u64,
}

impl WindowAnchor {
    fn modified_top_left(
        &self,
        grid_left: f64,
        grid_top: f64,
        width: u64,
        height: u64,
    ) -> (f64, f64) {
        match self {
            WindowAnchor::NorthWest => (grid_left, grid_top),
            WindowAnchor::NorthEast => (grid_left - width as f64, grid_top),
            WindowAnchor::SouthWest => (grid_left, grid_top - height as f64),
            WindowAnchor::SouthEast => (grid_left - width as f64, grid_top - height as f64),
        }
    }
}

#[derive(Clone, Debug)]
pub enum EditorCommand {
    NeovimRedrawEvent(RedrawEvent),
    RedrawScreen,
}

pub struct Editor {
    pub windows: HashMap<u64, Window>,
    pub cursor: Cursor,
    pub defined_styles: HashMap<u64, Arc<Style>>,
    pub mode_list: Vec<CursorMode>,
    pub draw_command_batcher: Rc<DrawCommandBatcher>,
    pub current_mode_index: Option<u64>,
}

impl Editor {
    pub fn new() -> Editor {
        Editor {
            windows: HashMap::new(),
            cursor: Cursor::new(),
            defined_styles: HashMap::new(),
            mode_list: Vec::new(),
            draw_command_batcher: Rc::new(DrawCommandBatcher::new()),
            current_mode_index: None,
        }
    }

    pub fn handle_editor_command(&mut self, command: EditorCommand) {
        match command {
            EditorCommand::NeovimRedrawEvent(event) => match event {
                RedrawEvent::SetTitle { title } => {
                    tracy_zone!("EditorSetTitle");
                    EVENT_AGGREGATOR.send(WindowCommand::TitleChanged(title));
                }
                RedrawEvent::ModeInfoSet { cursor_modes } => {
                    tracy_zone!("EditorModeInfoSet");
                    self.mode_list = cursor_modes;
                    if let Some(current_mode_i) = self.current_mode_index {
                        if let Some(current_mode) = self.mode_list.get(current_mode_i as usize) {
                            self.cursor.change_mode(current_mode, &self.defined_styles)
                        }
                    }
                }
                RedrawEvent::OptionSet { gui_option } => {
                    tracy_zone!("EditorOptionSet");
                    self.set_option(gui_option);
                }
                RedrawEvent::ModeChange { mode, mode_index } => {
                    tracy_zone!("ModeChange");
                    if let Some(cursor_mode) = self.mode_list.get(mode_index as usize) {
                        self.cursor.change_mode(cursor_mode, &self.defined_styles);
                        self.current_mode_index = Some(mode_index)
                    } else {
                        self.current_mode_index = None
                    }
                    self.draw_command_batcher
                        .queue(DrawCommand::ModeChanged(mode))
                        .ok();
                }
                RedrawEvent::MouseOn => {
                    tracy_zone!("EditorMouseOn");
                    EVENT_AGGREGATOR.send(WindowCommand::SetMouseEnabled(true));
                }
                RedrawEvent::MouseOff => {
                    tracy_zone!("EditorMouseOff");
                    EVENT_AGGREGATOR.send(WindowCommand::SetMouseEnabled(false));
                }
                RedrawEvent::BusyStart => {
                    tracy_zone!("EditorBusyStart");
                    trace!("Cursor off");
                    self.cursor.enabled = false;
                }
                RedrawEvent::BusyStop => {
                    tracy_zone!("EditorBusyStop");
                    trace!("Cursor on");
                    self.cursor.enabled = true;
                }
                RedrawEvent::Flush => {
                    tracy_zone!("EditorFlush");
                    trace!("Image flushed");
                    self.send_cursor_info();
                    {
                        trace!("send_batch");
                        self.draw_command_batcher.send_batch();
                    }
                    {
                        trace!("queue_next_frame");
                        REDRAW_SCHEDULER.queue_next_frame();
                    }
                }
                RedrawEvent::DefaultColorsSet { colors } => {
                    tracy_zone!("EditorDefaultColorsSet");
                    self.draw_command_batcher
                        .queue(DrawCommand::DefaultStyleChanged(Style::new(colors)))
                        .ok();
                    self.redraw_screen();
                    self.draw_command_batcher.send_batch();
                    REDRAW_SCHEDULER.queue_next_frame();
                }
                RedrawEvent::HighlightAttributesDefine { id, style } => {
                    tracy_zone!("EditorHighlightAttributesDefine");
                    self.defined_styles.insert(id, Arc::new(style));
                }
                RedrawEvent::CursorGoto {
                    grid,
                    column: left,
                    row: top,
                } => {
                    tracy_zone!("EditorCursorGoto");
                    self.set_cursor_position(grid, left, top);
                }
                RedrawEvent::Resize {
                    grid,
                    width,
                    height,
                } => {
                    tracy_zone!("EditorResize");
                    self.resize_window(grid, width, height);
                }
                RedrawEvent::GridLine {
                    grid,
                    row,
                    column_start,
                    cells,
                } => {
                    tracy_zone!("EditorGridLine");
                    let defined_styles = &self.defined_styles;
                    let window = self.windows.get_mut(&grid);
                    if let Some(window) = window {
                        window.draw_grid_line(row, column_start, cells, defined_styles);
                    }
                }
                RedrawEvent::Clear { grid } => {
                    tracy_zone!("EditorClear");
                    let window = self.windows.get_mut(&grid);
                    if let Some(window) = window {
                        window.clear();
                    }
                }
                RedrawEvent::Destroy { grid } => {
                    tracy_zone!("EditorDestroy");
                    self.close_window(grid)
                }
                RedrawEvent::Scroll {
                    grid,
                    top,
                    bottom,
                    left,
                    right,
                    rows,
                    columns,
                } => {
                    tracy_zone!("EditorScroll");
                    let window = self.windows.get_mut(&grid);
                    if let Some(window) = window {
                        window.scroll_region(top, bottom, left, right, rows, columns);
                    }
                }
                RedrawEvent::WindowPosition {
                    grid,
                    start_row,
                    start_column,
                    width,
                    height,
                } => {
                    tracy_zone!("EditorWindowPosition");
                    self.set_window_position(grid, start_column, start_row, width, height)
                }
                RedrawEvent::WindowFloatPosition {
                    grid,
                    anchor,
                    anchor_grid,
                    anchor_column: anchor_left,
                    anchor_row: anchor_top,
                    sort_order,
                    ..
                } => {
                    tracy_zone!("EditorWindowFloatPosition");
                    self.set_window_float_position(
                        grid,
                        anchor_grid,
                        anchor,
                        anchor_left,
                        anchor_top,
                        sort_order,
                    )
                }
                RedrawEvent::WindowHide { grid } => {
                    tracy_zone!("EditorWindowHide");
                    let window = self.windows.get(&grid);
                    if let Some(window) = window {
                        window.hide();
                    }
                }
                RedrawEvent::WindowClose { grid } => {
                    tracy_zone!("EditorWindowClose");
                    self.close_window(grid)
                }
                RedrawEvent::MessageSetPosition { grid, row, .. } => {
                    tracy_zone!("EditorMessageSetPosition");
                    self.set_message_position(grid, row)
                }
                RedrawEvent::WindowViewport {
                    grid,
                    // Don't send viewport events if they don't have a scroll delta
                    scroll_delta: Some(scroll_delta),
                    ..
                } => {
                    tracy_zone!("EditorWindowViewport");
                    self.send_updated_viewport(grid, scroll_delta)
                }
                RedrawEvent::ShowIntro { message } => {
                    EVENT_AGGREGATOR
                        .send(UiCommand::Parallel(ParallelCommand::ShowIntro { message }));
                }
                _ => {}
            },
            EditorCommand::RedrawScreen => {
                tracy_zone!("EditorRedrawScreen");
                self.redraw_screen();
            }
        };
    }

    fn close_window(&mut self, grid: u64) {
        if let Some(window) = self.windows.remove(&grid) {
            window.close();
            self.draw_command_batcher
                .queue(DrawCommand::CloseWindow(grid))
                .ok();
        }
    }

    fn resize_window(&mut self, grid: u64, width: u64, height: u64) {
        if let Some(window) = self.windows.get_mut(&grid) {
            window.resize((width, height));
            if let Some(anchor_info) = &window.anchor_info {
                let anchor_grid_id = anchor_info.anchor_grid_id;
                let anchor_type = anchor_info.anchor_type.clone();
                let anchor_left = anchor_info.anchor_left;
                let anchor_top = anchor_info.anchor_top;
                let sort_order = Some(anchor_info.sort_order);
                self.set_window_float_position(
                    grid,
                    anchor_grid_id,
                    anchor_type,
                    anchor_left,
                    anchor_top,
                    sort_order,
                )
            }
        } else {
            let window = Window::new(
                grid,
                WindowType::Editor,
                None,
                (0.0, 0.0),
                (width, height),
                self.draw_command_batcher.clone(),
            );
            self.windows.insert(grid, window);
        }
    }

    fn set_window_position(
        &mut self,
        grid: u64,
        start_left: u64,
        start_top: u64,
        width: u64,
        height: u64,
    ) {
        if let Some(window) = self.windows.get_mut(&grid) {
            window.position(None, (width, height), (start_left as f64, start_top as f64));
            window.show();
        } else {
            let new_window = Window::new(
                grid,
                WindowType::Editor,
                None,
                (start_left as f64, start_top as f64),
                (width, height),
                self.draw_command_batcher.clone(),
            );
            self.windows.insert(grid, new_window);
        }
    }

    fn set_window_float_position(
        &mut self,
        grid: u64,
        anchor_grid: u64,
        anchor_type: WindowAnchor,
        anchor_left: f64,
        anchor_top: f64,
        sort_order: Option<u64>,
    ) {
        let parent_position = self.get_window_top_left(anchor_grid);
        if let Some(window) = self.windows.get_mut(&grid) {
            let width = window.get_width();
            let height = window.get_height();
            let (mut modified_left, mut modified_top) =
                anchor_type.modified_top_left(anchor_left, anchor_top, width, height);

            if let Some((parent_left, parent_top)) = parent_position {
                modified_left += parent_left;
                modified_top += parent_top;
            }

            window.position(
                Some(AnchorInfo {
                    anchor_grid_id: anchor_grid,
                    anchor_type,
                    anchor_left,
                    anchor_top,
                    sort_order: sort_order.unwrap_or(grid),
                }),
                (width, height),
                (modified_left, modified_top),
            );
            window.show();
        } else {
            error!("Attempted to float window that does not exist.");
        }
    }

    fn set_message_position(&mut self, grid: u64, grid_top: u64) {
        let parent_width = self
            .windows
            .get(&1)
            .map(|parent| parent.get_width())
            .unwrap_or(1);

        let anchor_info = AnchorInfo {
            anchor_grid_id: 1, // Base Grid
            anchor_type: WindowAnchor::NorthWest,
            anchor_left: 0.0,
            anchor_top: grid_top as f64,
            sort_order: std::u64::MAX,
        };

        if let Some(window) = self.windows.get_mut(&grid) {
            window.window_type = WindowType::Message;
            window.position(
                Some(anchor_info),
                (parent_width, window.get_height()),
                (0.0, grid_top as f64),
            );
            window.show();
        } else {
            let new_window = Window::new(
                grid,
                WindowType::Message,
                Some(anchor_info),
                (0.0, grid_top as f64),
                (parent_width, 1),
                self.draw_command_batcher.clone(),
            );
            self.windows.insert(grid, new_window);
        }
    }

    fn get_window_top_left(&self, grid: u64) -> Option<(f64, f64)> {
        let window = self.windows.get(&grid)?;
        let window_anchor_info = &window.anchor_info;

        match window_anchor_info {
            Some(anchor_info) => {
                let (parent_anchor_left, parent_anchor_top) =
                    self.get_window_top_left(anchor_info.anchor_grid_id)?;

                let (anchor_modified_left, anchor_modified_top) =
                    anchor_info.anchor_type.modified_top_left(
                        anchor_info.anchor_left,
                        anchor_info.anchor_top,
                        window.get_width(),
                        window.get_height(),
                    );

                Some((
                    parent_anchor_left + anchor_modified_left,
                    parent_anchor_top + anchor_modified_top,
                ))
            }
            None => Some(window.get_grid_position()),
        }
    }

    fn set_cursor_position(&mut self, grid: u64, grid_left: u64, grid_top: u64) {
        if let Some(Window {
            window_type: WindowType::Message,
            ..
        }) = self.windows.get(&grid)
        {
            // When the user presses ":" to type a command, the cursor is sent to the gutter
            // in position 1 (right after the ":"). In all other cases, we want to skip
            // positioning to avoid confusing movements.
            let intentional = grid_left == 1;
            // If the cursor was already in this message, we can still move within it.
            let already_there = self.cursor.parent_window_id == grid;
            // This ^ check alone is a bit buggy though, since it fails when the cursor is
            // technically still in the edit window but "temporarily" at the cmdline. (#1207)
            let using_cmdline = self
                .current_mode_index
                .map(|current| current == MODE_CMDLINE)
                .unwrap_or(false);

            if !intentional && !already_there && !using_cmdline {
                trace!(
                    "Cursor unexpectedly sent to message buffer {} ({}, {})",
                    grid,
                    grid_left,
                    grid_top
                );
                return;
            }
        }

        self.cursor.parent_window_id = grid;
        self.cursor.grid_position = (grid_left, grid_top);
    }

    fn send_cursor_info(&mut self) {
        tracy_zone!("send_cursor_info");
        let (grid_left, grid_top) = self.cursor.grid_position;
        if let Some(window) = self.windows.get(&self.cursor.parent_window_id) {
            let (character, style, double_width) = window.get_cursor_grid_cell(grid_left, grid_top);
            self.cursor.grid_cell = (character, style);
            self.cursor.double_width = double_width;
        } else {
            self.cursor.double_width = false;
            self.cursor.grid_cell = (" ".to_string(), None);
        }
        self.draw_command_batcher
            .queue(DrawCommand::UpdateCursor(self.cursor.clone()))
            .ok();
    }

    fn set_option(&mut self, gui_option: GuiOption) {
        trace!("Option set {:?}", &gui_option);

        match gui_option {
            GuiOption::GuiFont(guifont) => {
                if guifont == *"*" {
                    EVENT_AGGREGATOR.send(WindowCommand::ListAvailableFonts);
                }

                self.draw_command_batcher
                    .queue(DrawCommand::FontChanged(guifont))
                    .ok();

                self.redraw_screen();
            }
            GuiOption::LineSpace(linespace) => {
                self.draw_command_batcher
                    .queue(DrawCommand::LineSpaceChanged(linespace))
                    .ok();

                self.redraw_screen();
            }
            _ => (),
        }
    }

    fn send_updated_viewport(&mut self, grid: u64, scroll_delta: f64) {
        if let Some(window) = self.windows.get_mut(&grid) {
            window.update_viewport(scroll_delta);
        } else {
            trace!("viewport event received before window initialized");
        }
    }

    fn redraw_screen(&mut self) {
        for window in self.windows.values() {
            window.redraw();
        }
    }
}

pub fn start_editor() {
    thread::spawn(move || {
        let mut editor = Editor::new();

        let mut editor_command_receiver = EVENT_AGGREGATOR.register_event::<EditorCommand>();
        while let Some(editor_command) = editor_command_receiver.blocking_recv() {
            editor.handle_editor_command(editor_command);
        }
    });
}
