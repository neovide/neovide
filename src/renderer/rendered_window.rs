use std::{cell::RefCell, rc::Rc, sync::Arc};

use skia_safe::{
    canvas::SaveLayerRec, BlendMode, Canvas, Color, Matrix, Paint, Picture, PictureRecorder, Rect,
};

use crate::{
    cmd_line::CmdLineSettings,
    editor::{AnchorInfo, SortOrder, Style, WindowType},
    profiling::{tracy_plot, tracy_zone},
    renderer::{animation_utils::*, GridRenderer, RendererSettings},
    settings::SETTINGS,
    units::{to_skia_rect, GridPos, GridRect, GridScale, GridSize, PixelRect, PixelVec},
    utils::RingBuffer,
};

#[derive(Clone, Debug, PartialEq)]
pub struct LineFragment {
    pub text: String,
    pub window_left: u64,
    pub width: u64,
    pub style: Option<Arc<Style>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ViewportMargins {
    pub top: u64,
    pub bottom: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum WindowDrawCommand {
    Position {
        grid_position: (f64, f64),
        grid_size: (u64, u64),
        anchor_info: Option<AnchorInfo>,
        window_type: WindowType,
    },
    DrawLine {
        row: usize,
        line_fragments: Vec<LineFragment>,
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
        left: u64,
        right: u64,
    },
    SortOrder(SortOrder),
}

#[derive(Clone)]
struct Line {
    line_fragments: Vec<LineFragment>,
    background_picture: Option<Picture>,
    foreground_picture: Option<Picture>,
    has_transparency: bool,
    is_valid: bool,
}

pub struct RenderedWindow {
    pub id: u64,
    pub hidden: bool,
    pub anchor_info: Option<AnchorInfo>,
    window_type: WindowType,

    pub grid_size: GridSize<u32>,

    scrollback_lines: RingBuffer<Option<Rc<RefCell<Line>>>>,
    actual_lines: RingBuffer<Option<Rc<RefCell<Line>>>>,
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
}

impl WindowDrawDetails {
    pub fn event_grid_id(&self) -> u64 {
        if SETTINGS.get::<CmdLineSettings>().no_multi_grid {
            0
        } else {
            self.id
        }
    }
}

impl RenderedWindow {
    pub fn new(id: u64, grid_position: GridPos<i32>, grid_size: GridSize<u32>) -> RenderedWindow {
        RenderedWindow {
            id,
            hidden: false,
            anchor_info: None,
            window_type: WindowType::Editor,

            grid_size,

            actual_lines: RingBuffer::new(grid_size.height as usize, None),
            scrollback_lines: RingBuffer::new(2 * grid_size.height as usize, None),
            scroll_delta: 0,
            viewport_margins: ViewportMargins { top: 0, bottom: 0 },

            grid_start_position: grid_position.try_cast().unwrap(),
            grid_current_position: grid_position.try_cast().unwrap(),
            grid_destination: grid_position.try_cast().unwrap(),
            position_t: 2.0, // 2.0 is out of the 0.0 to 1.0 range and stops animation.

            scroll_animation: CriticallyDampedSpringAnimation::new(),
        }
    }

    pub fn pixel_region(&self, grid_scale: GridScale) -> PixelRect<f32> {
        GridRect::<f32>::from_origin_and_size(
            self.grid_current_position,
            self.grid_size.try_cast().unwrap(),
        ) * grid_scale
    }

    fn get_target_position(&self, grid_rect: &GridRect<f32>) -> GridPos<f32> {
        let destination = self.grid_destination + grid_rect.min.to_vector();

        if self.anchor_info.is_none() {
            return destination;
        }

        let mut grid_size: GridSize<f32> = self.grid_size.try_cast().unwrap();

        if matches!(self.window_type, WindowType::Message { .. }) {
            // The message grid size is always the full window size, so use the relative position to
            // calculate the actual grid size
            grid_size.height -= self.grid_destination.y;
        }
        // If a floating window is partially outside the grid, then move it in from the right, but
        // ensure that the left edge is always visible.
        let x = destination
            .x
            .min(grid_rect.max.x - grid_size.width)
            .max(grid_rect.min.x);

        // For messages the last line is most important, (it shows press enter), so let the position go negative
        // Otherwise ensure that the window start row is within the screen
        let mut y = destination.y.min(grid_rect.max.y - grid_size.height);
        if !matches!(self.window_type, WindowType::Message { .. }) {
            y = y.max(grid_rect.min.y)
        }
        GridPos::<f32>::new(x, y)
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

        let scrolling = self
            .scroll_animation
            .update(dt, settings.scroll_animation_length);

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
        log::trace!(
            "region: {:?}, inner: {:?}, pics: {}",
            pixel_region,
            inner_region,
            pics
        );
        canvas.restore();
        canvas.restore();
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
    ) -> WindowDrawDetails {
        let pixel_region_box = self.pixel_region(grid_scale);
        let pixel_region = to_skia_rect(&pixel_region_box);

        root_canvas.save();
        root_canvas.clip_rect(pixel_region, None, Some(false));

        let paint = Paint::default()
            .set_anti_alias(false)
            .set_blend_mode(if self.anchor_info.is_some() {
                BlendMode::SrcOver
            } else {
                BlendMode::Src
            })
            .to_owned();

        let save_layer_rec = SaveLayerRec::default().bounds(&pixel_region).paint(&paint);
        root_canvas.save_layer(&save_layer_rec);

        let mut background_paint = Paint::default();
        background_paint.set_blend_mode(BlendMode::Src);
        background_paint.set_alpha(default_background.a());
        let background_layer_rec = SaveLayerRec::default()
            .bounds(&pixel_region)
            .paint(&background_paint);

        root_canvas.save_layer(&background_layer_rec);
        root_canvas.clear(default_background.with_a(255));
        self.draw_background_surface(root_canvas, pixel_region_box, grid_scale);
        root_canvas.restore();
        self.draw_foreground_surface(root_canvas, pixel_region_box, grid_scale);
        root_canvas.restore();

        root_canvas.restore();

        WindowDrawDetails {
            id: self.id,
            region: pixel_region_box,
        }
    }

    pub fn handle_window_draw_command(&mut self, draw_command: WindowDrawCommand) {
        match draw_command {
            WindowDrawCommand::Position {
                grid_position,
                grid_size,
                anchor_info,
                window_type,
            } => {
                tracy_zone!("position_cmd", 0);

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

                if self.hidden {
                    self.hidden = false;
                    self.position_t = 2.0; // We don't want to animate since the window is becoming visible,
                                           // so we set t to 2.0 to stop animations.
                    self.grid_start_position = grid_position;
                    self.grid_destination = grid_position;
                }
            }
            WindowDrawCommand::DrawLine {
                row,
                line_fragments,
            } => {
                tracy_zone!("draw_line_cmd", 0);

                let line = Line {
                    line_fragments,
                    background_picture: None,
                    foreground_picture: None,
                    has_transparency: false,
                    is_valid: false,
                };

                self.actual_lines[row] = Some(Rc::new(RefCell::new(line)));
            }
            WindowDrawCommand::Scroll {
                top,
                bottom,
                left,
                right,
                rows,
                cols,
            } => {
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
                self.scrollback_lines
                    .iter_mut()
                    .for_each(|line| *line = None);
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

            let max_delta = self.scrollback_lines.len() - self.grid_size.height as usize;
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

    fn iter_border_lines(&self) -> impl Iterator<Item = (isize, &Rc<RefCell<Line>>)> {
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
    fn iter_scrollable_lines(&self) -> impl Iterator<Item = (isize, &Rc<RefCell<Line>>)> {
        let scroll_offset_lines = self.scroll_animation.position.floor();
        let scroll_offset_lines = scroll_offset_lines as isize;
        let inner_size = self.actual_lines.len() as isize
            - self.viewport_margins.top as isize
            - self.viewport_margins.bottom as isize;

        let line_indices = if inner_size > 0 {
            0..inner_size + 1
        } else {
            0..0
        };

        line_indices.filter_map(move |i| {
            self.scrollback_lines[scroll_offset_lines + i]
                .as_ref()
                .map(|line| (i, line))
        })
    }

    fn iter_scrollable_lines_with_transform(
        &self,
        pixel_region: PixelRect<f32>,
        grid_scale: GridScale,
    ) -> impl Iterator<Item = (Matrix, &Rc<RefCell<Line>>)> {
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
    ) -> impl Iterator<Item = (Matrix, &Rc<RefCell<Line>>)> {
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

    pub fn prepare_lines(&mut self, grid_renderer: &mut GridRenderer, force: bool) {
        let scroll_offset_lines = self.scroll_animation.position.floor() as isize;
        let height = self.grid_size.height as isize;
        if height == 0 {
            return;
        }
        let grid_scale = grid_renderer.grid_scale;

        let mut prepare_line = |line: &Rc<RefCell<Line>>| {
            let mut line = line.borrow_mut();
            if line.is_valid && !force {
                return;
            }

            let mut recorder = PictureRecorder::new();

            let line_size = GridSize::new(self.grid_size.width, 1) * grid_scale;
            let grid_rect = Rect::from_wh(line_size.width, line_size.height);
            let canvas = recorder.begin_recording(grid_rect, None);

            let mut has_transparency = false;
            let mut custom_background = false;

            for line_fragment in line.line_fragments.iter() {
                let LineFragment {
                    window_left,
                    width,
                    style,
                    ..
                } = line_fragment;
                let grid_position = (i32::try_from(*window_left).unwrap(), 0).into();
                let background_info = grid_renderer.draw_background(
                    canvas,
                    grid_position,
                    i32::try_from(*width).unwrap(),
                    style,
                );
                custom_background |= background_info.custom_color;
                has_transparency |= background_info.transparent;
            }
            let background_picture =
                custom_background.then_some(recorder.finish_recording_as_picture(None).unwrap());

            let canvas = recorder.begin_recording(grid_rect, None);
            let mut foreground_drawn = false;
            for line_fragment in &line.line_fragments {
                let LineFragment {
                    text,
                    window_left,
                    width,
                    style,
                } = line_fragment;
                let grid_position = (i32::try_from(*window_left).unwrap(), 0).into();

                foreground_drawn |= grid_renderer.draw_foreground(
                    canvas,
                    text,
                    grid_position,
                    i32::try_from(*width).unwrap(),
                    style,
                );
            }
            let foreground_picture =
                foreground_drawn.then_some(recorder.finish_recording_as_picture(None).unwrap());

            line.background_picture = background_picture;
            line.foreground_picture = foreground_picture;
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

        for line in self
            .actual_lines
            .iter_range_mut(0..self.viewport_margins.top as isize)
            .flatten()
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
