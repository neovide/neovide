use std::{cell::RefCell, rc::Rc};

use skia_safe::{
    Canvas, Color, Color4f, Matrix, Paint, Path, PathBuilder, Picture, PictureRecorder, Rect,
};

use crate::{
    bridge::WindowAnchor,
    cmd_line::CmdLineSettings,
    editor::{AnchorInfo, Line, LineFragment, SortOrder, WindowType},
    profiling::{tracy_plot, tracy_zone},
    renderer::{GridRenderer, RendererSettings, animation_utils::*},
    settings::Settings,
    units::{GridPos, GridRect, GridScale, GridSize, PixelPos, PixelRect, PixelVec, to_skia_rect},
    utils::RingBuffer,
};

pub const BASE_GRID_ID: u64 = 1;
pub const NO_MULTIGRID_GRID_ID: u64 = 0;

// Window layouts can leave a tiny remainder to the right of the last full
// grid cell when the content width is not an exact multiple of the cell
// width. We extend the last column's background slightly into that gap, capped
// to a few cell widths so the line never appears visibly stretched if the grid
// briefly lags a resize.
const MAX_TRAILING_FILL_CELLS: f32 = 1.0;

#[derive(Debug)]
pub struct ViewportMargins {
    pub top: u64,
    pub bottom: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WindowDrawCommand {
    Position {
        grid_position: (f64, f64),
        grid_size: (u64, u64),
        anchor_info: Option<AnchorInfo>,
        window_type: WindowType,
    },
    DrawLine {
        row: usize,
        line: Line,
    },
    Scroll {
        top: u64,
        bottom: u64,
        left: u64,
        right: u64,
        rows: i64,
        cols: i64,
    },
    Clear,
    Show,
    Hide,
    Close,
    Viewport {
        scroll_delta: f64,
    },
    ViewportMargins {
        top: u64,
        bottom: u64,
        #[allow(unused)]
        left: u64,
        #[allow(unused)]
        right: u64,
    },
    SortOrder(SortOrder),
}

struct RenderedLine {
    line: Line,
    background_picture: Option<Picture>,
    foreground_picture: Option<Picture>,
    boxchar_picture: Option<(Picture, PixelPos<f32>)>,
    trailing_background: Option<Color4f>,
    has_transparency: bool,
    is_valid: bool,
}

struct TrailingFillRect {
    rect: Rect,
    color: Color4f,
}

pub struct RenderedWindow {
    pub id: u64,
    valid: bool,
    pub hidden: bool,
    pub anchor_info: Option<AnchorInfo>,
    pub window_type: WindowType,

    pub grid_size: GridSize<u32>,

    scrollback_lines: RingBuffer<Option<Rc<RefCell<RenderedLine>>>>,
    actual_lines: RingBuffer<Option<Rc<RefCell<RenderedLine>>>>,
    scroll_delta: isize,
    pub viewport_margins: ViewportMargins,

    grid_start_position: GridPos<f32>,
    pub grid_current_position: GridPos<f32>,
    grid_destination: GridPos<f32>,
    position_t: f32,

    pub scroll_animation: CriticallyDampedSpringAnimation,
}

#[derive(Clone, Debug)]
pub struct WindowDrawDetails {
    pub id: u64,
    pub region: PixelRect<f32>,
    pub grid_size: GridSize<u32>,
    pub window_type: WindowType,
}

impl WindowDrawDetails {
    pub fn event_grid_id(&self, settings: &Settings) -> u64 {
        if settings.get::<CmdLineSettings>().no_multi_grid { NO_MULTIGRID_GRID_ID } else { self.id }
    }
}

impl RenderedWindow {
    pub fn new(id: u64) -> RenderedWindow {
        let grid_size = GridSize::ZERO;
        let grid_position = GridPos::ZERO;
        RenderedWindow {
            id,
            valid: false,
            hidden: false,
            anchor_info: None,
            window_type: WindowType::Editor,

            grid_size,

            actual_lines: RingBuffer::new(grid_size.height as usize, None),
            scrollback_lines: RingBuffer::new(2 * grid_size.height as usize, None),
            scroll_delta: 0,
            viewport_margins: ViewportMargins { top: 0, bottom: 0 },

            grid_start_position: grid_position,
            grid_current_position: grid_position,
            grid_destination: grid_position,
            position_t: 2.0, // 2.0 is out of the 0.0 to 1.0 range and stops animation.

            scroll_animation: CriticallyDampedSpringAnimation::new(),
        }
    }

    pub fn pixel_region(&self, grid_scale: GridScale) -> PixelRect<f32> {
        // Round to the same fraction as the desination to avoid glitches when rendering box
        // characters.
        let fract = (self.grid_destination * grid_scale).fract();
        let pos = (self.grid_current_position * grid_scale - fract).round() + fract.to_vector();
        PixelRect::<f32>::from_origin_and_size(pos.into(), self.grid_size() * grid_scale)
    }

    fn grid_size(&self) -> GridSize<u32> {
        let mut size = self.grid_size;
        if matches!(self.window_type, WindowType::Message { .. }) {
            // Neovim reports message grids with the full default-grid height even
            // when they are anchored near the bottom. Only the rows from the
            // message start row down to the bottom edge are actually visible.
            size.height = size.height.saturating_sub(self.grid_destination.y.max(0.0) as u32);
        }
        size
    }

    fn get_target_position(&self, grid_rect: &GridRect<f32>) -> GridPos<f32> {
        let destination = self.grid_destination + grid_rect.min.to_vector();

        match self.anchor_info {
            None => destination,
            Some(AnchorInfo { anchor_type: WindowAnchor::Absolute, .. }) => destination,
            _ => {
                let grid_size: GridSize<f32> = self.grid_size().try_cast().unwrap();
                // If a floating window is partially outside the grid, then move it in from the right, but
                // ensure that the left edge is always visible.
                let x = destination.x.min(grid_rect.max.x - grid_size.width).max(grid_rect.min.x);

                // For messages the last line is most important, (it shows press enter), so let the position go negative
                // Otherwise ensure that the window start row is within the screen
                let mut y = destination.y.min(grid_rect.max.y - grid_size.height);
                if !matches!(self.window_type, WindowType::Message { .. }) {
                    y = y.max(grid_rect.min.y)
                }
                GridPos::<f32>::new(x, y)
            }
        }
    }

    /// Returns `true` if the window has been animated in this step.
    pub fn animate(
        &mut self,
        settings: &RendererSettings,
        grid_rect: &GridRect<f32>,
        dt: f32,
    ) -> bool {
        let mut animating = false;

        if self.position_t > 1.0 - f32::EPSILON {
            // We are at destination, move t out of 0-1 range to stop the animation.
            self.position_t = 2.0;
        } else {
            animating = true;
            self.position_t = (self.position_t + dt / settings.position_animation_length).min(1.0);
        }

        let prev_position = self.grid_current_position;
        self.grid_current_position = ease_point(
            ease_out_expo,
            self.grid_start_position,
            self.get_target_position(grid_rect),
            self.position_t,
        );
        animating |= self.grid_current_position != prev_position;

        let scrolling = self.scroll_animation.update(dt, settings.scroll_animation_length);

        animating |= scrolling;

        if scrolling {
            tracy_plot!("Scroll position {}", self.scroll_animation.position.into());
        }

        animating
    }

    pub fn draw_background_surface(
        &mut self,
        canvas: &Canvas,
        pixel_region: PixelRect<f32>,
        grid_scale: GridScale,
    ) {
        let inner_region = self.inner_region(pixel_region, grid_scale);

        canvas.save();
        canvas.clip_rect(to_skia_rect(&pixel_region), None, false);
        for (matrix, line) in self.iter_border_lines_with_transform(pixel_region, grid_scale) {
            let line = line.borrow();
            if let Some(background_picture) = &line.background_picture {
                canvas.draw_picture(background_picture, Some(&matrix), None);
            }
        }
        canvas.restore();

        canvas.save();
        canvas.clip_rect(inner_region, None, false);
        let mut pics = 0;
        for (matrix, line) in self.iter_scrollable_lines_with_transform(pixel_region, grid_scale) {
            let line = line.borrow();
            if let Some(background_picture) = &line.background_picture {
                canvas.draw_picture(background_picture, Some(&matrix), None);
                pics += 1;
            }
        }

        log::trace!("region: {pixel_region:?}, inner: {inner_region:?}, pics: {pics}");
        canvas.restore();

        self.draw_trailing_background_surface(canvas, pixel_region, grid_scale);
    }

    pub fn draw_foreground_surface(
        &mut self,
        canvas: &Canvas,
        pixel_region: PixelRect<f32>,
        grid_scale: GridScale,
    ) {
        for (matrix, line) in self.iter_border_lines_with_transform(pixel_region, grid_scale) {
            let line = line.borrow();
            if let Some(foreground_picture) = &line.foreground_picture {
                canvas.draw_picture(foreground_picture, Some(&matrix), None);
            }
        }
        canvas.save();
        canvas.clip_rect(self.inner_region(pixel_region, grid_scale), None, false);
        for (matrix, line) in self.iter_scrollable_lines_with_transform(pixel_region, grid_scale) {
            let line = line.borrow();
            if let Some(foreground_picture) = &line.foreground_picture {
                canvas.draw_picture(foreground_picture, Some(&matrix), None);
            }
        }
        canvas.restore();

        for (mut matrix, line) in self.iter_border_lines_with_transform(pixel_region, grid_scale) {
            let line = line.borrow();
            if let Some((boxchar_picture, position)) = &line.boxchar_picture {
                let deltax = pixel_region.min.x - position.x;
                matrix.set_translate_x(deltax);
                canvas.draw_picture(boxchar_picture, Some(&matrix), None);
            }
        }
        canvas.save();
        canvas.clip_rect(self.inner_region(pixel_region, grid_scale), None, false);
        for (mut matrix, line) in
            self.iter_scrollable_lines_with_transform(pixel_region, grid_scale)
        {
            let line = line.borrow();
            if let Some((boxchar_picture, position)) = &line.boxchar_picture {
                let deltax = pixel_region.min.x - position.x;
                matrix.set_translate_x(deltax);
                canvas.draw_picture(boxchar_picture, Some(&matrix), None);
            }
        }
        canvas.restore();
    }

    pub fn has_transparency(&self) -> bool {
        let scroll_offset_lines = self.scroll_animation.position.floor() as isize;
        if self.scrollback_lines.is_empty() {
            return false;
        }
        self.scrollback_lines
            .iter_range(
                scroll_offset_lines..scroll_offset_lines + self.grid_size.height as isize + 1,
            )
            .flatten()
            .any(|line| line.borrow().has_transparency)
    }

    pub fn draw(
        &mut self,
        root_canvas: &Canvas,
        default_background: Color,
        grid_scale: GridScale,
        content_region: Option<PixelRect<f32>>,
        rightmost_window: bool,
    ) -> WindowDrawDetails {
        let pixel_region_box = self.pixel_region(grid_scale);
        let draw_region_box = self.expanded_pixel_region(
            pixel_region_box,
            content_region,
            grid_scale,
            rightmost_window,
        );
        let pixel_region = to_skia_rect(&draw_region_box);

        if !self.valid {
            return WindowDrawDetails {
                id: self.id,
                region: pixel_region_box,
                grid_size: self.grid_size,
                window_type: self.window_type,
            };
        }

        root_canvas.save();
        root_canvas.clip_rect(pixel_region, None, Some(false));
        root_canvas.clear(default_background);

        self.draw_background_surface(root_canvas, draw_region_box, grid_scale);
        self.draw_foreground_surface(root_canvas, draw_region_box, grid_scale);

        root_canvas.restore();

        WindowDrawDetails {
            id: self.id,
            region: draw_region_box,
            grid_size: self.grid_size,
            window_type: self.window_type,
        }
    }

    pub fn expanded_pixel_region(
        &self,
        pixel_region: PixelRect<f32>,
        content_region: Option<PixelRect<f32>>,
        grid_scale: GridScale,
        rightmost_window: bool,
    ) -> PixelRect<f32> {
        let Some(content_region) = content_region else {
            return pixel_region;
        };

        let mut region = pixel_region;
        let right_gap = content_region.max.x - region.max.x;
        if rightmost_window
            && right_gap > 0.0
            && right_gap <= grid_scale.width() * MAX_TRAILING_FILL_CELLS + f32::EPSILON
        {
            region.max.x = content_region.max.x;
        }

        region
    }

    pub fn draw_trailing_background_surface(
        &self,
        canvas: &Canvas,
        pixel_region: PixelRect<f32>,
        grid_scale: GridScale,
    ) {
        let mut paint = Paint::default();
        paint.set_anti_alias(false);
        paint.set_blend_mode(skia_safe::BlendMode::SrcOver);

        // the trailing fill follows the same clipping model as the normal
        // background pass. fixed rows like border and margins rows can
        // paint across the full window region, but scrollable rows need
        // stay inside the inner viewport. So keeping those as separate
        // clip scopes prevents the buffered scroll rows from leaking into
        // fixed UI rows.
        canvas.save();
        canvas.clip_rect(to_skia_rect(&pixel_region), None, false);

        for fill in self.trailing_fill_rects(pixel_region, grid_scale) {
            paint.set_color4f(fill.color, None);
            canvas.draw_rect(fill.rect, &paint);
        }

        canvas.restore();
    }

    pub fn trailing_fill_path_and_bounds(
        &self,
        pixel_region: PixelRect<f32>,
        grid_scale: GridScale,
    ) -> Option<(Path, Rect)> {
        let mut builder = PathBuilder::new();
        let mut bounds = None;

        for fill in self.trailing_fill_rects(pixel_region, grid_scale) {
            self.push_trailing_fill_rect_path(&mut builder, &mut bounds, fill.rect);
        }

        bounds.map(|bounds| (builder.detach(), bounds))
    }

    fn trailing_fill_rects(
        &self,
        pixel_region: PixelRect<f32>,
        grid_scale: GridScale,
    ) -> Vec<TrailingFillRect> {
        let base_region = self.pixel_region(grid_scale);
        let inner_region = self.inner_region(pixel_region, grid_scale);
        let extra_width = (pixel_region.max.x - base_region.max.x)
            .min(grid_scale.width() * MAX_TRAILING_FILL_CELLS);

        if extra_width <= 0.0 {
            return Vec::new();
        }

        let mut fills = Vec::new();
        for (i, line) in self.iter_border_lines() {
            let line = line.borrow();
            let Some(color) = line.trailing_background else {
                continue;
            };

            fills.push(TrailingFillRect {
                rect: self.trailing_fill_rect(
                    base_region.max.x,
                    extra_width,
                    pixel_region.min.y + i as f32 * grid_scale.height(),
                    grid_scale.height(),
                ),
                color,
            });
        }

        // this fill is part of the rendered grid background, not a separete
        // overlay. when the window is mid-scroll, the scrollable rows can
        // sit at a fractional cell offset, so this fill has to use that
        // same pixel offset too.
        //
        // See https://github.com/neovide/neovide/pull/3387
        let scroll_offset_lines = self.scroll_animation.position.floor();
        let scroll_offset = scroll_offset_lines - self.scroll_animation.position;
        let scroll_offset_pixels = (scroll_offset * grid_scale.height()).round();
        for (i, line) in self.iter_scrollable_lines() {
            let line = line.borrow();
            let Some(color) = line.trailing_background else {
                continue;
            };

            let y = pixel_region.min.y
                + scroll_offset_pixels
                + (i + self.viewport_margins.top as isize) as f32 * grid_scale.height();
            let top = y.max(inner_region.top);
            let bottom = (y + grid_scale.height()).min(inner_region.bottom);
            if bottom <= top {
                continue;
            }

            fills.push(TrailingFillRect {
                rect: self.trailing_fill_rect(base_region.max.x, extra_width, top, bottom - top),
                color,
            });
        }

        fills
    }

    fn trailing_fill_rect(&self, left: f32, width: f32, top: f32, height: f32) -> Rect {
        Rect::from_xywh(left, top, width, height)
    }

    fn push_trailing_fill_rect_path(
        &self,
        builder: &mut PathBuilder,
        bounds: &mut Option<Rect>,
        rect: Rect,
    ) {
        if rect.is_empty() {
            return;
        }

        builder
            .move_to((rect.left, rect.top))
            .line_to((rect.right, rect.top))
            .line_to((rect.right, rect.bottom))
            .line_to((rect.left, rect.bottom))
            .close();

        *bounds = Some(match *bounds {
            Some(current) => Rect::join2(current, rect),
            None => rect,
        });
    }

    fn line_for_row(&self, row: u32) -> Option<Rc<RefCell<RenderedLine>>> {
        if self.actual_lines.is_empty() {
            return None;
        }

        let row = row as isize;
        let height = self.grid_size.height as isize;
        if row < 0 || row >= height {
            return None;
        }

        let top_margin = self.viewport_margins.top as isize;
        let bottom_margin = self.viewport_margins.bottom as isize;
        let bottom_start = height.saturating_sub(bottom_margin);

        if row < top_margin || row >= bottom_start {
            return self.actual_lines[row].as_ref().cloned();
        }

        let inner_row = row - top_margin;
        let scroll_offset = self.scroll_animation.position.floor() as isize;
        self.scrollback_lines[scroll_offset + inner_row].as_ref().cloned()
    }

    pub fn line_text_range(&self, row: u32, start_col: u32, end_col: u32) -> Option<String> {
        let line = self.line_for_row(row)?;
        let line = line.borrow();
        let cells = line.line.cells()?;
        if cells.is_empty() {
            return Some(String::new());
        }

        let max_col = cells.len().saturating_sub(1) as u32;
        let start = start_col.min(max_col);
        let end = end_col.min(max_col);
        let (start, end) = if start <= end { (start, end) } else { (end, start) };

        let mut text = String::new();
        for col in start..=end {
            text.push_str(&cells[col as usize]);
        }

        let trimmed_len = text.trim_end_matches(' ').len();
        text.truncate(trimmed_len);

        Some(text)
    }

    pub fn grid_row_rect(
        &self,
        row: u32,
        col_start: u32,
        col_end: u32,
        grid_scale: GridScale,
    ) -> Option<Rect> {
        let height = self.grid_size.height;
        let width = self.grid_size.width;
        if height == 0 || width == 0 {
            return None;
        }

        let max_row = height - 1;
        let max_col = width - 1;
        let row = row.min(max_row);
        let start_col = col_start.min(max_col);
        let end_col = col_end.min(max_col);
        let (start_col, end_col) =
            if start_col <= end_col { (start_col, end_col) } else { (end_col, start_col) };

        let pixel_region = self.pixel_region(grid_scale);
        let line_height = grid_scale.height();
        let col_width = grid_scale.width();
        let base_x = pixel_region.min.x;
        let base_y = pixel_region.min.y;

        let scroll_offset_pixels =
            (self.scroll_animation.position.floor() - self.scroll_animation.position) * line_height;
        let top_margin = self.viewport_margins.top as u32;
        let bottom_margin = self.viewport_margins.bottom as u32;
        let bottom_start = height.saturating_sub(bottom_margin);

        let y = if row < top_margin || row >= bottom_start {
            base_y + row as f32 * line_height
        } else {
            base_y + scroll_offset_pixels + row as f32 * line_height
        };
        let x = base_x + start_col as f32 * col_width;
        let w = (end_col - start_col + 1) as f32 * col_width;

        Some(Rect::from_xywh(x, y, w, line_height))
    }

    pub fn handle_window_draw_command(&mut self, draw_command: WindowDrawCommand) {
        match draw_command {
            WindowDrawCommand::Position { grid_position, grid_size, anchor_info, window_type } => {
                tracy_zone!("position_cmd", 0);

                self.valid = true;

                let new_grid_size: GridSize<u32> =
                    GridSize::<u64>::from(grid_size).try_cast().unwrap();
                let grid_position: GridPos<f32> =
                    GridPos::<f64>::from(grid_position).try_cast().unwrap();

                if self.grid_destination != grid_position {
                    if self.grid_start_position.x.abs() > f32::EPSILON
                        || self.grid_start_position.y.abs() > f32::EPSILON
                    {
                        self.position_t = 0.0; // Reset animation as we have a new destination.
                        self.grid_start_position = self.grid_current_position;
                    } else {
                        // We don't want to animate since the window is animating out of the start location,
                        // so we set t to 2.0 to stop animations.
                        self.position_t = 2.0;
                        self.grid_start_position = grid_position;
                    }
                    self.grid_destination = grid_position;
                }

                let height = new_grid_size.height as usize;
                self.actual_lines.resize(height, None);
                self.grid_size = new_grid_size;

                self.scrollback_lines.resize(2 * height, None);
                self.scrollback_lines.clone_from_iter(&self.actual_lines);
                self.scroll_delta = 0;

                if height != self.actual_lines.len() {
                    self.scroll_animation.reset();
                }

                self.anchor_info = anchor_info;
                self.window_type = window_type;
            }
            WindowDrawCommand::DrawLine { row, line } => {
                tracy_zone!("draw_line_cmd", 0);

                if self.actual_lines.is_empty() || !self.valid {
                    log::warn!(
                        "Ignoring DrawLine for grid {} row {} because the window is not ready yet",
                        self.id,
                        row
                    );
                    return;
                }

                let line = RenderedLine {
                    line,
                    background_picture: None,
                    foreground_picture: None,
                    boxchar_picture: None,
                    trailing_background: None,
                    has_transparency: false,
                    is_valid: false,
                };

                self.actual_lines[row] = Some(Rc::new(RefCell::new(line)));
            }
            WindowDrawCommand::Scroll { top, bottom, left, right, rows, cols } => {
                tracy_zone!("scroll_cmd", 0);
                if top == 0
                    && bottom == u64::from(self.grid_size.height)
                    && left == 0
                    && right == u64::from(self.grid_size.width)
                    && cols == 0
                {
                    self.actual_lines.rotate(rows as isize);
                }
            }
            WindowDrawCommand::Clear => {
                tracy_zone!("clear_cmd", 0);
                self.scroll_delta = 0;
                self.scrollback_lines.iter_mut().for_each(|line| *line = None);
                self.scroll_animation.reset();
            }
            WindowDrawCommand::Show => {
                tracy_zone!("show_cmd", 0);
                if self.hidden {
                    self.hidden = false;
                    self.position_t = 2.0; // We don't want to animate since the window is becoming visible,
                    // so we set t to 2.0 to stop animations.
                    self.grid_start_position = self.grid_destination;
                    self.scroll_animation.reset();
                }
            }
            WindowDrawCommand::Hide => {
                tracy_zone!("hide_cmd", 0);
                self.hidden = true;
            }
            WindowDrawCommand::Viewport { scroll_delta } => {
                log::trace!("Handling Viewport {}", self.id);
                self.scroll_delta = scroll_delta.round() as isize;
            }
            WindowDrawCommand::ViewportMargins { top, bottom, .. } => {
                self.viewport_margins = ViewportMargins { top, bottom }
            }
            WindowDrawCommand::SortOrder(sort_order) => {
                if let Some(anchor_info) = self.anchor_info.as_mut() {
                    anchor_info.sort_order = sort_order;
                }
            }
            _ => {}
        };
    }

    pub fn flush(&mut self, renderer_settings: &RendererSettings) {
        if !self.valid {
            return;
        }
        // If the borders are changed, reset the scrollback to only fit the inner view
        let inner_range = self.viewport_margins.top as isize
            ..(self.actual_lines.len() - self.viewport_margins.bottom as usize) as isize;
        let inner_size = inner_range.len();
        let inner_view = self.actual_lines.iter_range(inner_range);
        if inner_size != self.scrollback_lines.len() / 2 {
            self.scrollback_lines.resize(2 * inner_size, None);
            self.scrollback_lines.clone_from_iter(inner_view);
            self.scroll_delta = 0;
            self.scroll_animation.reset();
            return;
        }

        let scroll_delta = self.scroll_delta;
        self.scrollback_lines.rotate(scroll_delta);

        self.scrollback_lines.clone_from_iter(inner_view);

        if scroll_delta != 0 {
            let mut scroll_offset = self.scroll_animation.position;

            let max_delta =
                self.scrollback_lines.len().saturating_sub(self.grid_size.height as usize);
            log::trace!(
                "Scroll offset {scroll_offset}, delta {scroll_delta}, max_delta {max_delta}"
            );
            // Do a limited scroll with empty lines when scrolling far
            if scroll_delta.unsigned_abs() > max_delta {
                let far_lines = renderer_settings
                    .scroll_animation_far_lines
                    .min(self.actual_lines.len() as u32) as isize;

                scroll_offset = -(far_lines * scroll_delta.signum()) as f32;
                let empty_lines = if scroll_delta > 0 {
                    -far_lines..0
                } else {
                    self.actual_lines.len() as isize..self.actual_lines.len() as isize + far_lines
                };
                for i in empty_lines {
                    self.scrollback_lines[i] = None;
                }
            // And even when scrolling in steps, we can't let it drift too far, since the
            // buffer size is limited
            } else {
                scroll_offset -= scroll_delta as f32;
                scroll_offset = scroll_offset.clamp(-(max_delta as f32), max_delta as f32);
            }
            self.scroll_animation.position = scroll_offset;
            log::trace!("Current scroll {scroll_offset}");
        }
        self.scroll_delta = 0;
    }

    fn iter_border_lines(&self) -> impl Iterator<Item = (isize, &Rc<RefCell<RenderedLine>>)> {
        let top_border_indices = 0..self.viewport_margins.top as isize;
        let actual_line_count = self.actual_lines.len() as isize;
        let bottom_border_indices =
            actual_line_count - self.viewport_margins.bottom as isize..actual_line_count;

        top_border_indices
            .chain(bottom_border_indices)
            .filter_map(move |i| self.actual_lines[i].as_ref().map(|line| (i, line)))
    }

    // Iterates over the scrollable lines (excluding the viewport margins). Includes the index for
    // the given line being scrolled
    fn iter_scrollable_lines(&self) -> impl Iterator<Item = (isize, &Rc<RefCell<RenderedLine>>)> {
        let scroll_offset_lines = self.scroll_animation.position.floor();
        let scroll_offset_lines = scroll_offset_lines as isize;
        let inner_size = self.actual_lines.len() as isize
            - self.viewport_margins.top as isize
            - self.viewport_margins.bottom as isize;

        let line_indices = if inner_size > 0 { 0..inner_size + 1 } else { 0..0 };

        line_indices.filter_map(move |i| {
            self.scrollback_lines[scroll_offset_lines + i].as_ref().map(|line| (i, line))
        })
    }

    fn iter_scrollable_lines_with_transform(
        &self,
        pixel_region: PixelRect<f32>,
        grid_scale: GridScale,
    ) -> impl Iterator<Item = (Matrix, &Rc<RefCell<RenderedLine>>)> {
        let scroll_offset_lines = self.scroll_animation.position.floor();
        let scroll_offset = scroll_offset_lines - self.scroll_animation.position;
        let scroll_offset_pixels = (scroll_offset * grid_scale.height()).round();

        self.iter_scrollable_lines().map(move |(i, line)| {
            let mut matrix = Matrix::new_identity();
            matrix.set_translate((
                pixel_region.min.x,
                pixel_region.min.y
                    + (scroll_offset_pixels
                        + ((i + self.viewport_margins.top as isize) as f32 * grid_scale.height())),
            ));
            (matrix, line)
        })
    }

    fn iter_border_lines_with_transform(
        &self,
        pixel_region: PixelRect<f32>,
        grid_scale: GridScale,
    ) -> impl Iterator<Item = (Matrix, &Rc<RefCell<RenderedLine>>)> {
        self.iter_border_lines().map(move |(i, line)| {
            let mut matrix = Matrix::new_identity();
            matrix.set_translate((
                pixel_region.min.x,
                pixel_region.min.y + (i as f32 * grid_scale.height()),
            ));
            (matrix, line)
        })
    }

    /// Returns the rect containing the region of the window that does not have borders above and
    /// below it. Note: This does not take into account the borders on the left and the right of
    /// the window.
    pub fn inner_region(&self, pixel_region: PixelRect<f32>, grid_scale: GridScale) -> Rect {
        let line_height = grid_scale.height();

        let adjusted_region = PixelRect::new(
            pixel_region.min + PixelVec::new(0., self.viewport_margins.top as f32 * line_height),
            pixel_region.max - PixelVec::new(0., self.viewport_margins.bottom as f32 * line_height),
        );

        to_skia_rect(&adjusted_region)
    }

    pub fn prepare_lines(&mut self, grid_renderer: &mut GridRenderer, opacity: f32, force: bool) {
        let scroll_offset_lines = self.scroll_animation.position.floor() as isize;
        let height = self.grid_size.height as isize;
        if height == 0 {
            return;
        }
        let grid_scale = grid_renderer.grid_scale;

        let mut prepare_line = |line: &Rc<RefCell<RenderedLine>>| {
            let mut line = line.borrow_mut();
            let position = self.grid_destination * grid_renderer.grid_scale;
            let boxchar_moved = match line.boxchar_picture {
                None => false,
                Some((_, p)) if p == position => false,
                _ => true,
            };
            // This can be optimized, only the boxchars need to be redrawn when the window moves
            if line.is_valid && !force && !boxchar_moved {
                return;
            }

            let mut recorder = PictureRecorder::new();

            let line_size = GridSize::new(self.grid_size.width, 1) * grid_scale;
            let grid_rect = Rect::from_wh(line_size.width, line_size.height);
            let canvas = recorder.begin_recording(grid_rect, false);

            let mut has_transparency = false;
            let mut custom_background = false;

            for line_fragment in line.line.fragments() {
                let LineFragment { cells, style, .. } = line_fragment;
                let background_info = grid_renderer.draw_background(canvas, cells, style, opacity);
                custom_background |= background_info.custom_color;
                has_transparency |= background_info.transparent;
            }
            let background_picture =
                custom_background.then_some(recorder.finish_recording_as_picture(None).unwrap());

            let text_canvas = recorder.begin_recording(grid_rect, false);
            let mut boxchar_recorder = PictureRecorder::new();
            let boxchar_canvas =
                boxchar_recorder.begin_recording(grid_rect.with_offset((position.x, 0.0)), false);
            let mut text_drawn = false;
            let mut boxchar_drawn = false;
            for line_fragment in line.line.fragments() {
                let (frag_text_drawn, frag_box_drawn) = grid_renderer.draw_foreground(
                    text_canvas,
                    boxchar_canvas,
                    &line_fragment,
                    position,
                );
                text_drawn |= frag_text_drawn;
                boxchar_drawn |= frag_box_drawn;
            }
            let foreground_picture =
                text_drawn.then_some(recorder.finish_recording_as_picture(None).unwrap());
            let boxchar_picture = boxchar_drawn
                .then_some((boxchar_recorder.finish_recording_as_picture(None).unwrap(), position));

            let trailing_background = line
                .line
                .fragments()
                .filter(|fragment| fragment.cells.end == self.grid_size.width)
                .last()
                .map(|fragment| grid_renderer.background_paint_color(fragment.style, opacity));

            line.background_picture = background_picture;
            line.foreground_picture = foreground_picture;
            line.boxchar_picture = boxchar_picture;
            line.trailing_background = trailing_background;
            line.has_transparency = has_transparency;
            line.is_valid = true;
        };

        if !self.scrollback_lines.is_empty() {
            for line in self
                .scrollback_lines
                .iter_range_mut(scroll_offset_lines..scroll_offset_lines + height + 1)
                .flatten()
            {
                prepare_line(line)
            }
        }

        for line in
            self.actual_lines.iter_range_mut(0..self.viewport_margins.top as isize).flatten()
        {
            prepare_line(line)
        }
        let actual_line_count = self.actual_lines.len() as isize;
        for line in self
            .actual_lines
            .iter_range_mut(
                actual_line_count - self.viewport_margins.bottom as isize..actual_line_count,
            )
            .flatten()
        {
            prepare_line(line)
        }
    }
}
