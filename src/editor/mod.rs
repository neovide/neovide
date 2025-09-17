mod cursor;
mod draw_command_batcher;
mod grid;
mod intro;
mod style;
mod window;

use std::{collections::HashMap, sync::Arc, thread};

use log::{error, trace, warn};
use tokio::sync::mpsc::unbounded_channel;

use winit::event_loop::EventLoopProxy;

#[cfg(target_os = "macos")]
use winit::window::Theme;

#[cfg(target_os = "macos")]
use skia_safe::Color4f;

use crate::{
    bridge::{GridLineCell, GuiOption, NeovimHandler, RedrawEvent, WindowAnchor},
    profiling::{tracy_named_frame, tracy_zone},
    renderer::{DrawCommand, WindowDrawCommand},
    running_tracker::RunningTracker,
    settings::Settings,
    units::{GridRect, GridSize},
    window::{EventPayload, WindowCommand, WindowSettings},
};

#[cfg(target_os = "macos")]
use crate::{cmd_line::CmdLineSettings, frame::Frame};

pub use cursor::{Cursor, CursorMode, CursorShape};
pub use draw_command_batcher::DrawCommandBatcher;
pub use style::{Colors, Style, UnderlineStyle};
pub use window::*;

use intro::{IntroMessageExtender, IntroProcessing};

const MODE_CMDLINE: u64 = 4;
pub const MSG_ZINDEX: u64 = 200; // See the documenation for nvim_open_win

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SortOrder {
    pub z_index: u64,
    composition_order: u64,
}

impl Ord for SortOrder {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // The windows are sorted primarily by z_index, and inside the z_index by
        // composition_order. The composition_order is the window creation order with the special
        // case that every time a floating window is activated it gets the highest priority for
        // its z_index.
        let a = (self.z_index, (self.composition_order as i64));
        let b = (other.z_index, (other.composition_order as i64));
        a.cmp(&b)
    }
}

impl PartialOrd for SortOrder {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnchorInfo {
    pub anchor_grid_id: u64,
    pub anchor_type: WindowAnchor,
    pub anchor_left: f64,
    pub anchor_top: f64,
    pub sort_order: SortOrder,
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
            WindowAnchor::Absolute => (grid_left, grid_top),
        }
    }
}

pub struct Editor {
    pub windows: HashMap<u64, Window>,
    pub cursor: Cursor,
    pub defined_styles: HashMap<u64, Arc<Style>>,
    pub mode_list: Vec<CursorMode>,
    pub draw_command_batcher: DrawCommandBatcher,
    pub current_mode_index: Option<u64>,
    pub ui_ready: bool,
    event_loop_proxy: EventLoopProxy<EventPayload>,
    #[allow(dead_code)]
    settings: Arc<Settings>,
    composition_order: u64,
    intro_message_extender: IntroMessageExtender,
}

impl Editor {
    pub fn new(event_loop_proxy: EventLoopProxy<EventPayload>, settings: Arc<Settings>) -> Self {
        Editor {
            windows: HashMap::new(),
            cursor: Cursor::new(),
            defined_styles: HashMap::new(),
            mode_list: Vec::new(),
            draw_command_batcher: DrawCommandBatcher::new(),
            current_mode_index: None,
            ui_ready: false,
            settings,
            event_loop_proxy,
            composition_order: 0,
            intro_message_extender: IntroMessageExtender::new(),
        }
    }

    pub fn handle_redraw_event(
        &mut self,
        winit_window_id: winit::window::WindowId,
        event: RedrawEvent,
    ) {
        match event {
            RedrawEvent::SetTitle { mut title } => {
                tracy_zone!("EditorSetTitle");
                if title.is_empty() {
                    title = "Neovide".to_string()
                }
                let _ = self
                    .event_loop_proxy
                    .send_event(WindowCommand::TitleChanged(title).into());
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
                    .queue(DrawCommand::ModeChanged(mode));
            }
            RedrawEvent::MouseOn => {
                tracy_zone!("EditorMouseOn");
                let _ = self
                    .event_loop_proxy
                    .send_event(WindowCommand::SetMouseEnabled(true).into());
            }
            RedrawEvent::MouseOff => {
                tracy_zone!("EditorMouseOff");
                let _ = self
                    .event_loop_proxy
                    .send_event(WindowCommand::SetMouseEnabled(false).into());
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
                tracy_named_frame!("neovim draw command flush");
                self.send_cursor_info();
                {
                    trace!("send_batch");
                    self.draw_command_batcher
                        .send_batch(winit_window_id, &self.event_loop_proxy);
                }
            }
            RedrawEvent::DefaultColorsSet { colors } => {
                tracy_zone!("EditorDefaultColorsSet");

                // Set the dark/light theme of window, so the titlebar text gets correct color.
                #[cfg(target_os = "macos")]
                if self.settings.get::<CmdLineSettings>().frame == Frame::Transparent {
                    let _ = self.event_loop_proxy.send_event(
                        WindowCommand::ThemeChanged(window_theme_for_background(colors.background))
                            .into(),
                    );
                }

                self.draw_command_batcher
                    .queue(DrawCommand::DefaultStyleChanged(Style::new(colors)));
                self.redraw_screen();
                self.draw_command_batcher
                    .send_batch(winit_window_id, &self.event_loop_proxy);
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
                self.set_ui_ready();
                self.draw_grid_line(grid, row, column_start, &cells);
                self.handle_intro_banner_for_line(grid, row, &cells);
            }
            RedrawEvent::Clear { grid } => {
                tracy_zone!("EditorClear");
                let window = self.windows.get_mut(&grid);
                if let Some(window) = window {
                    window.clear(&mut self.draw_command_batcher);
                }
                self.intro_message_extender.reset(grid);
            }
            RedrawEvent::Destroy { grid } => {
                tracy_zone!("EditorDestroy");
                self.intro_message_extender.reset(grid);
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
                    window.scroll_region(
                        &mut self.draw_command_batcher,
                        GridRect::from_min_max((left, top), (right, bottom)),
                        GridSize::new(columns, rows),
                    );
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
                z_index,
                comp_index,
                screen_row,
                screen_col,
                ..
            } => {
                tracy_zone!("EditorWindowFloatPosition");
                let anchor_type = if comp_index.is_some() {
                    WindowAnchor::Absolute
                } else {
                    self.composition_order += 1;
                    anchor
                };
                let sort_order = SortOrder {
                    z_index,
                    composition_order: comp_index.unwrap_or(self.composition_order),
                };
                let anchor = AnchorInfo {
                    anchor_grid_id: anchor_grid,
                    anchor_type,
                    anchor_left,
                    anchor_top,
                    sort_order,
                };
                self.set_window_float_position(grid, anchor, screen_col, screen_row)
            }
            RedrawEvent::WindowHide { grid } => {
                tracy_zone!("EditorWindowHide");
                let window = self.windows.get_mut(&grid);
                if let Some(window) = window {
                    window.anchor_info = None;
                    window.hide(&mut self.draw_command_batcher);
                }
            }
            RedrawEvent::WindowClose { grid } => {
                tracy_zone!("EditorWindowClose");
                self.close_window(grid)
            }
            RedrawEvent::MessageSetPosition {
                grid,
                row,
                scrolled,
                z_index,
                comp_index,
                ..
            } => {
                tracy_zone!("EditorMessageSetPosition");
                self.set_message_position(grid, row, scrolled, z_index, comp_index)
            }
            RedrawEvent::WindowViewport {
                grid,
                // Don't send viewport events if they don't have a scroll delta
                scroll_delta: Some(scroll_delta),
                ..
            } => {
                tracy_zone!("EditorWindowViewport");
                self.set_ui_ready();
                self.draw_command_batcher.queue(DrawCommand::Window {
                    grid_id: grid,
                    command: WindowDrawCommand::Viewport { scroll_delta },
                });
            }
            RedrawEvent::WindowViewportMargins {
                grid,
                top,
                bottom,
                left,
                right,
            } => {
                tracy_zone!("EditorWindowViewportMargins");
                self.draw_command_batcher.queue(DrawCommand::Window {
                    grid_id: grid,
                    command: WindowDrawCommand::ViewportMargins {
                        top,
                        bottom,
                        left,
                        right,
                    },
                });
            }
            // Interpreting suspend as a window minimize request
            RedrawEvent::Suspend => {
                let _ = self
                    .event_loop_proxy
                    .send_event(WindowCommand::Minimize.into());
            }
            RedrawEvent::NeovideSetRedraw(enable) => self.draw_command_batcher.set_enabled(
                enable,
                winit_window_id,
                &self.event_loop_proxy,
            ),
            RedrawEvent::NeovideIntroBannerAllowed(allowed) => {
                self.intro_message_extender.set_sponsor_allowed(
                    allowed,
                    &mut self.windows,
                    &mut self.draw_command_batcher,
                );
            }
            _ => {}
        };
    }

    fn close_window(&mut self, grid: u64) {
        if let Some(window) = self.windows.remove(&grid) {
            window.close(&mut self.draw_command_batcher);
        }
    }

    fn resize_window(&mut self, grid: u64, width: u64, height: u64) {
        if let Some(window) = self.windows.get_mut(&grid) {
            window.resize(&mut self.draw_command_batcher, (width, height));
            if let Some(anchor_info) = &window.anchor_info {
                let anchor_info = anchor_info.clone();

                self.set_window_float_position(grid, anchor_info, None, None)
            }
        } else {
            let window = Window::new(
                grid,
                WindowType::Editor,
                None,
                (0.0, 0.0),
                (width, height),
                &mut self.draw_command_batcher,
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
            window.position(
                &mut self.draw_command_batcher,
                None,
                (width, height),
                (start_left as f64, start_top as f64),
            );
            window.show(&mut self.draw_command_batcher);
        } else {
            let new_window = Window::new(
                grid,
                WindowType::Editor,
                None,
                (start_left as f64, start_top as f64),
                (width, height),
                &mut self.draw_command_batcher,
            );
            self.windows.insert(grid, new_window);
        }
    }

    fn set_window_float_position(
        &mut self,
        grid: u64,
        anchor: AnchorInfo,
        screen_col: Option<u64>,
        screen_row: Option<u64>,
    ) {
        if anchor.anchor_grid_id == grid {
            warn!("NeoVim requested a window to float relative to itself. This is not supported.");
            return;
        }

        let parent_position = self.get_window_top_left(anchor.anchor_grid_id);
        if let Some(window) = self.windows.get_mut(&grid) {
            let width = window.get_width();
            let height = window.get_height();
            let neovim_composed = anchor.anchor_type == WindowAnchor::Absolute;
            let (left, top, sort_order) =
                if neovim_composed {
                    // NOTE: screen_col is None when the window is just resized
                    if let (Some(screen_col), Some(screen_row)) = (screen_col, screen_row) {
                        (
                            screen_col as f64,
                            screen_row as f64,
                            anchor.sort_order.clone(),
                        )
                    } else {
                        let (left, top) = window.get_grid_position();
                        (left, top, anchor.sort_order.clone())
                    }
                } else {
                    let (mut modified_left, mut modified_top) = anchor
                        .anchor_type
                        .modified_top_left(anchor.anchor_left, anchor.anchor_top, width, height);

                    if let Some((parent_left, parent_top)) = parent_position {
                        modified_left += parent_left;
                        modified_top += parent_top;
                    }

                    // Only update the sort order if it's the first position request (no anchor_info), or
                    // the z_index changes
                    let sort_order = if let Some(old_anchor) = &window.anchor_info {
                        if anchor.sort_order.z_index == old_anchor.sort_order.z_index {
                            old_anchor.sort_order.clone()
                        } else {
                            anchor.sort_order.clone()
                        }
                    } else {
                        anchor.sort_order.clone()
                    };
                    (modified_left, modified_top, sort_order)
                };
            let mut anchor = anchor;
            anchor.sort_order = sort_order;

            window.position(
                &mut self.draw_command_batcher,
                Some(anchor),
                (width, height),
                (left, top),
            );
            window.show(&mut self.draw_command_batcher);
        } else {
            error!("Attempted to float window that does not exist.");
        }
    }

    fn set_message_position(
        &mut self,
        grid: u64,
        grid_top: u64,
        scrolled: bool,
        z_index: Option<u64>,
        comp_index: Option<u64>,
    ) {
        // HACK: workaround https://github.com/neovide/neovide/issues/3150 by ignoring grid id 0.
        // The real grid id should always be something else. But Neovim 0.11.3 sends an extra
        // msg_set_pos with grid id 0.
        if grid == 0 {
            return;
        }
        let z_index = z_index.unwrap_or(MSG_ZINDEX); // From the Neovim source code
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
            sort_order: SortOrder {
                z_index,
                composition_order: comp_index.unwrap_or(self.composition_order),
            },
        };

        if let Some(window) = self.windows.get_mut(&grid) {
            window.window_type = WindowType::Message { scrolled };
            window.position(
                &mut self.draw_command_batcher,
                Some(anchor_info),
                (parent_width, window.get_height()),
                (0.0, grid_top as f64),
            );
            window.show(&mut self.draw_command_batcher);
        } else {
            let new_window = Window::new(
                grid,
                WindowType::Message { scrolled },
                Some(anchor_info),
                (0.0, grid_top as f64),
                (parent_width, 1),
                &mut self.draw_command_batcher,
            );
            self.windows.insert(grid, new_window);
        }
    }

    fn get_window_top_left(&self, grid: u64) -> Option<(f64, f64)> {
        let window = self.windows.get(&grid)?;
        let window_anchor_info = &window.anchor_info;

        match window_anchor_info {
            Some(AnchorInfo {
                anchor_type: WindowAnchor::Absolute,
                ..
            }) => Some(window.get_grid_position()),
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
        let mut window = self.windows.get_mut(&grid);
        if let Some(window) = &mut window {
            if let Some(anchor) = window.anchor_info.as_mut() {
                // Neovim moves a window to the top of the layer each time the cursor enters it, so do the same here as well
                self.composition_order += 1;
                anchor.sort_order.composition_order = self.composition_order;
                self.draw_command_batcher.queue(DrawCommand::Window {
                    grid_id: grid,
                    command: WindowDrawCommand::SortOrder(anchor.sort_order.clone()),
                });
            }
        }

        if self.settings.get::<WindowSettings>().cursor_hack {
            if let Some(Window {
                window_type: WindowType::Message { .. },
                ..
            }) = window
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
                        "Cursor unexpectedly sent to message buffer {grid} ({grid_left}, {grid_top})"
                    );
                    return;
                }
            }
        }

        self.cursor.parent_window_id = grid;
        self.cursor.grid_position = (grid_left, grid_top);
    }

    fn draw_grid_line(&mut self, grid: u64, row: u64, column_start: u64, cells: &[GridLineCell]) {
        if let Some(window) = self.windows.get_mut(&grid) {
            window.draw_grid_line(
                &mut self.draw_command_batcher,
                row,
                column_start,
                cells.to_vec(),
                &self.defined_styles,
            );
        }
    }

    fn handle_intro_banner_for_line(&mut self, grid: u64, row: u64, cells: &[GridLineCell]) {
        if !self.intro_message_extender.sponsor_allowed() {
            return;
        }

        match self.intro_message_extender.preprocess_line(grid, cells) {
            IntroProcessing::Skip => return,
            IntroProcessing::ClearBanner => {
                self.intro_message_extender.maybe_hide_banner(
                    grid,
                    &mut self.windows,
                    &mut self.draw_command_batcher,
                );
                return;
            }
            IntroProcessing::Process => {}
        }

        let line_text = grid_line_cells_to_text(cells);
        let sponsor_banner_row = self
            .intro_message_extender
            .banner_injection_row(grid, row, &line_text);

        self.maybe_inject_intro_banner(grid, sponsor_banner_row);
        if sponsor_banner_row.is_none() {
            self.intro_message_extender.maybe_hide_banner(
                grid,
                &mut self.windows,
                &mut self.draw_command_batcher,
            );
        }
    }

    fn maybe_inject_intro_banner(&mut self, grid: u64, banner_row: Option<u64>) {
        if let Some(start_row) = banner_row {
            if let Some(window) = self.windows.get_mut(&grid) {
                self.intro_message_extender.inject_banner(
                    grid,
                    window,
                    start_row,
                    &mut self.draw_command_batcher,
                );
            }
        }
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
            .queue(DrawCommand::UpdateCursor(self.cursor.clone()));
    }

    fn set_option(&mut self, gui_option: GuiOption) {
        trace!("Option set {:?}", &gui_option);

        match gui_option {
            GuiOption::GuiFont(guifont) => {
                if guifont == *"*" {
                    let _ = self
                        .event_loop_proxy
                        .send_event(WindowCommand::ListAvailableFonts.into());
                } else {
                    self.draw_command_batcher
                        .queue(DrawCommand::FontChanged(guifont));

                    self.redraw_screen();
                }
            }
            GuiOption::LineSpace(linespace) => {
                self.draw_command_batcher
                    .queue(DrawCommand::LineSpaceChanged(linespace as f32));

                self.redraw_screen();
            }
            _ => (),
        }
    }

    fn redraw_screen(&mut self) {
        for window in self.windows.values() {
            window.redraw(&mut self.draw_command_batcher);
        }
    }

    fn set_ui_ready(&mut self) {
        if !self.ui_ready {
            self.ui_ready = true;
            self.draw_command_batcher.queue(DrawCommand::UIReady);
        }
    }
}

fn grid_line_cells_to_text(cells: &[GridLineCell]) -> String {
    let mut text = String::new();
    for cell in cells {
        let repeat = cell.repeat.unwrap_or(1);
        for _ in 0..repeat {
            text.push_str(&cell.text);
        }
    }
    text
}

pub fn start_editor_handler(
    winit_window_id: winit::window::WindowId,
    event_loop_proxy: EventLoopProxy<EventPayload>,
    running_tracker: RunningTracker,
    settings: Arc<Settings>,
) -> NeovimHandler {
    let (redraw_event_sender, mut redraw_event_receiver) = unbounded_channel();
    let (ui_command_sender, ui_command_receiver) = unbounded_channel();
    let handler = NeovimHandler::new(
        redraw_event_sender,
        ui_command_sender,
        ui_command_receiver,
        event_loop_proxy.clone(),
        running_tracker,
        settings.clone(),
    );
    thread::spawn(move || {
        let mut editor = Editor::new(event_loop_proxy, settings.clone());

        while let Some(editor_command) = redraw_event_receiver.blocking_recv() {
            editor.handle_redraw_event(winit_window_id, editor_command);
        }
    });
    handler
}

/// Based on formula in https://graphicdesign.stackexchange.com/questions/62368/automatically-select-a-foreground-color-based-on-a-background-color
/// Check if the color is light or dark
#[cfg(target_os = "macos")]
fn is_light_color(color: &Color4f) -> bool {
    0.2126 * color.r + 0.7152 * color.g + 0.0722 * color.b > 0.5
}

/// Get the proper dark/light theme for a background_color.
#[cfg(target_os = "macos")]
fn window_theme_for_background(background_color: Option<Color4f>) -> Option<Theme> {
    background_color?;

    match background_color.unwrap() {
        color if is_light_color(&color) => Some(Theme::Light),
        _ => Some(Theme::Dark),
    }
}
