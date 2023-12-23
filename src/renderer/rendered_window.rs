use std::{cell::RefCell, ops::Range, rc::Rc, sync::Arc};

use skia_safe::{
    canvas::SaveLayerRec,
    image_filters::blur,
    scalar,
    utils::shadow_utils::{draw_shadow, ShadowFlags},
    BlendMode, Canvas, ClipOp, Color, Contains, Matrix, Paint, Path, Picture, PictureRecorder,
    Point, Point3, Rect,
};

use crate::{
    cmd_line::CmdLineSettings,
    dimensions::Dimensions,
    editor::{AnchorInfo, Style, WindowType},
    profiling::tracy_zone,
    renderer::{animation_utils::*, GridRenderer, RendererSettings},
    settings::SETTINGS,
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
}

#[derive(Clone)]
struct Line {
    line_fragments: Vec<LineFragment>,
    background_picture: Option<Picture>,
    foreground_picture: Option<Picture>,
    has_transparency: bool,
    is_border: bool,
    is_valid: bool,
}

pub struct RenderedWindow {
    pub vertical_position: f32,

    pub id: u64,
    pub hidden: bool,
    pub anchor_info: Option<AnchorInfo>,
    window_type: WindowType,

    pub grid_size: Dimensions,

    scrollback_lines: RingBuffer<Option<Rc<RefCell<Line>>>>,
    actual_lines: RingBuffer<Option<Rc<RefCell<Line>>>>,
    scroll_delta: isize,
    pub top_border: Range<isize>,
    pub bottom_border: Range<isize>,

    grid_start_position: Point,
    pub grid_current_position: Point,
    grid_destination: Point,
    position_t: f32,

    pub scroll_animation: CriticallyDampedSpringAnimation,

    has_transparency: bool,
}

#[derive(Clone, Debug)]
pub struct WindowDrawDetails {
    pub id: u64,
    pub region: Rect,
    pub floating_order: Option<u64>,
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
    pub fn new(id: u64, grid_position: Point, grid_size: Dimensions) -> RenderedWindow {
        RenderedWindow {
            vertical_position: 0.0,
            id,
            hidden: false,
            anchor_info: None,
            window_type: WindowType::Editor,

            grid_size,

            actual_lines: RingBuffer::new(grid_size.height as usize, None),
            scrollback_lines: RingBuffer::new(2 * grid_size.height as usize, None),
            scroll_delta: 0,
            top_border: 0..0,
            bottom_border: 0..0,

            grid_start_position: grid_position,
            grid_current_position: grid_position,
            grid_destination: grid_position,
            position_t: 2.0, // 2.0 is out of the 0.0 to 1.0 range and stops animation.

            scroll_animation: CriticallyDampedSpringAnimation::new(),

            has_transparency: false,
        }
    }

    pub fn pixel_region(&self, font_dimensions: Dimensions) -> Rect {
        let current_pixel_position = Point::new(
            self.grid_current_position.x * font_dimensions.width as f32,
            self.grid_current_position.y * font_dimensions.height as f32,
        );

        let image_size: (i32, i32) = (self.grid_size * font_dimensions).into();

        Rect::from_point_and_size(current_pixel_position, image_size)
    }

    fn get_target_position(&self, outer_size: &Dimensions, padding_as_grid: &Rect) -> Point {
        let destination = Point {
            x: self.grid_destination.x + padding_as_grid.left,
            y: self.grid_destination.y + padding_as_grid.top,
        };

        if self.anchor_info.is_none() {
            return destination;
        }

        // Note the rect is always as far top/left as possible, which means that the right and
        // bottom paddings might be bigger than requested. This is done in order to avoid the text
        // moving around when the window is resized.
        let valid_rect = Rect {
            left: padding_as_grid.left,
            right: padding_as_grid.left + outer_size.width as scalar,
            top: padding_as_grid.top,
            bottom: padding_as_grid.top + outer_size.height as scalar,
        };

        let mut grid_size = Point::new(self.grid_size.width as f32, self.grid_size.height as f32);
        if matches!(self.window_type, WindowType::Message { .. }) {
            // The message grid size is always the full window size, so use the relative position to
            // calculate the actual grid size
            grid_size.y -= self.grid_destination.y;
        }

        let x = destination
            .x
            .min(valid_rect.right - grid_size.x)
            .max(valid_rect.left);

        // For messages the last line is most important, (it shows press enter), so let the position go negative
        // Otherwise ensure that the window start row is within the screen
        let mut y = destination.y.min(valid_rect.bottom - grid_size.y);
        if matches!(self.window_type, WindowType::Message { .. }) {
            y = y.max(valid_rect.top)
        }
        Point { x, y }
    }

    /// Returns `true` if the window has been animated in this step.
    pub fn animate(
        &mut self,
        settings: &RendererSettings,
        outer_size: &Dimensions,
        padding_as_grid: &Rect,
        dt: f32,
    ) -> bool {
        let mut animating = false;

        if 1.0 - self.position_t < std::f32::EPSILON {
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
            self.get_target_position(outer_size, padding_as_grid),
            self.position_t,
        );
        animating |= self.grid_current_position != prev_position;

        animating |= self
            .scroll_animation
            .update(dt, settings.scroll_animation_length);

        animating
    }

    pub fn draw_surface(
        &mut self,
        canvas: &Canvas,
        pixel_region: &Rect,
        font_dimensions: Dimensions,
        default_background: Color,
    ) {
        let scroll_offset_lines = self.scroll_animation.position.floor();
        let scroll_offset = scroll_offset_lines - self.scroll_animation.position;
        let scroll_offset_lines = scroll_offset_lines as isize;
        let scroll_offset_pixels = (scroll_offset * font_dimensions.height as f32).round() as isize;
        let line_height = font_dimensions.height as f32;
        let mut has_transparency = false;

        let lines: Vec<(Matrix, &Rc<RefCell<Line>>)> = if !self.scrollback_lines.is_empty() {
            (0..self.grid_size.height as isize + 1)
                .filter_map(|i| {
                    self.scrollback_lines[scroll_offset_lines + i]
                        .as_ref()
                        .map(|line| (i, line))
                })
                .map(|(i, line)| {
                    let mut matrix = Matrix::new_identity();
                    matrix.set_translate((
                        pixel_region.left(),
                        pixel_region.top()
                            + (scroll_offset_pixels
                                + ((i + self.top_border.len() as isize)
                                    * font_dimensions.height as isize))
                                as f32,
                    ));
                    (matrix, line)
                })
                .collect()
        } else {
            Vec::new()
        };

        let border_lines: Vec<_> = self
            .top_border
            .clone()
            .chain(self.bottom_border.clone())
            .filter_map(|i| {
                self.actual_lines[i]
                    .as_ref()
                    .and_then(|line| line.borrow().is_border.then_some((i, line)))
            })
            .map(|(i, line)| {
                let mut matrix = Matrix::new_identity();
                matrix.set_translate((
                    pixel_region.left(),
                    pixel_region.top() + (i * font_dimensions.height as isize) as f32,
                ));
                (matrix, line)
            })
            .collect();

        let inner_region = Rect::from_xywh(
            pixel_region.x(),
            pixel_region.y() + self.top_border.len() as f32 * line_height,
            pixel_region.width(),
            pixel_region.height()
                - (self.top_border.len() + self.bottom_border.len()) as f32 * line_height,
        );

        let mut background_paint = Paint::default();
        background_paint.set_blend_mode(BlendMode::Src);
        background_paint.set_alpha(default_background.a());

        let save_layer_rec = SaveLayerRec::default()
            .bounds(pixel_region)
            .paint(&background_paint);
        canvas.save_layer(&save_layer_rec);
        canvas.clear(default_background.with_a(255));
        for (matrix, line) in &border_lines {
            let line = line.borrow();
            if let Some(background_picture) = &line.background_picture {
                has_transparency |= line.has_transparency;
                canvas.draw_picture(background_picture, Some(matrix), None);
            }
        }
        canvas.save();
        canvas.clip_rect(inner_region, None, false);
        for (matrix, line) in &lines {
            let line = line.borrow();
            if let Some(background_picture) = &line.background_picture {
                has_transparency |= line.has_transparency;
                canvas.draw_picture(background_picture, Some(matrix), None);
            }
        }
        canvas.restore();
        canvas.restore();

        for (matrix, line) in &border_lines {
            let line = line.borrow();
            if let Some(foreground_picture) = &line.foreground_picture {
                canvas.draw_picture(foreground_picture, Some(matrix), None);
            }
        }
        canvas.save();
        canvas.clip_rect(inner_region, None, false);
        for (matrix, line) in &lines {
            let line = line.borrow();
            if let Some(foreground_picture) = &line.foreground_picture {
                canvas.draw_picture(foreground_picture, Some(matrix), None);
            }
        }
        canvas.restore();
        self.has_transparency = has_transparency;
    }

    fn has_transparency(&self) -> bool {
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
        settings: &RendererSettings,
        default_background: Color,
        font_dimensions: Dimensions,
        previous_floating_rects: &mut Vec<Rect>,
    ) -> WindowDrawDetails {
        let has_transparency = default_background.a() != 255 || self.has_transparency();

        let pixel_region = self.pixel_region(font_dimensions);
        let transparent_floating = self.anchor_info.is_some() && has_transparency;

        if self.anchor_info.is_some()
            && settings.floating_shadow
            && !previous_floating_rects
                .iter()
                .any(|rect| rect.contains(pixel_region))
        {
            root_canvas.save();
            let shadow_path = Path::rect(pixel_region, None);
            // We clip using the Difference op to make sure that the shadow isn't rendered inside
            // the window itself.
            root_canvas.clip_path(&shadow_path, Some(ClipOp::Difference), None);
            // The light angle is specified in degrees from the vertical, so we first convert them
            // to radians and then use sin/cos to get the y and z components of the light
            let light_angle_radians = settings.light_angle_degrees.to_radians();
            draw_shadow(
                root_canvas,
                &shadow_path,
                // Specifies how far from the root canvas the shadow casting rect is. We just use
                // the z component here to set it a constant distance away.
                Point3::new(0., 0., settings.floating_z_height),
                // Because we use the DIRECTIONAL_LIGHT shadow flag, this specifies the angle that
                // the light is coming from.
                Point3::new(0., -light_angle_radians.sin(), light_angle_radians.cos()),
                // This is roughly equal to the apparent radius of the light .
                5.,
                Color::from_argb((0.03 * 255.) as u8, 0, 0, 0),
                Color::from_argb((0.35 * 255.) as u8, 0, 0, 0),
                // Directional Light flag is necessary to make the shadow render consistently
                // across various sizes of floating windows. It effects how the light direction is
                // processed.
                Some(ShadowFlags::DIRECTIONAL_LIGHT),
            );
            root_canvas.restore();
            previous_floating_rects.push(pixel_region);
        }

        root_canvas.save();
        root_canvas.clip_rect(pixel_region, None, Some(false));
        let need_blur = transparent_floating && settings.floating_blur;

        if need_blur {
            if let Some(blur) = blur(
                (
                    settings.floating_blur_amount_x,
                    settings.floating_blur_amount_y,
                ),
                None,
                None,
                None,
            ) {
                let paint = Paint::default()
                    .set_anti_alias(false)
                    .set_blend_mode(BlendMode::Src)
                    .to_owned();
                let save_layer_rec = SaveLayerRec::default()
                    .backdrop(&blur)
                    .bounds(&pixel_region)
                    .paint(&paint);
                root_canvas.save_layer(&save_layer_rec);
                root_canvas.restore();
            }
        }

        let paint = Paint::default()
            .set_anti_alias(false)
            .set_color(Color::from_argb(255, 255, 255, default_background.a()))
            .set_blend_mode(if self.anchor_info.is_some() {
                BlendMode::SrcOver
            } else {
                BlendMode::Src
            })
            .to_owned();

        let save_layer_rec = SaveLayerRec::default().bounds(&pixel_region).paint(&paint);
        root_canvas.save_layer(&save_layer_rec);
        self.draw_surface(
            root_canvas,
            &pixel_region,
            font_dimensions,
            default_background,
        );
        root_canvas.restore();

        root_canvas.restore();

        WindowDrawDetails {
            id: self.id,
            region: pixel_region,
            floating_order: self.anchor_info.as_ref().map(|v| v.sort_order),
        }
    }

    pub fn handle_window_draw_command(&mut self, draw_command: WindowDrawCommand) {
        match draw_command {
            WindowDrawCommand::Position {
                grid_position: (grid_left, grid_top),
                grid_size,
                anchor_info,
                window_type,
            } => {
                tracy_zone!("position_cmd", 0);

                let grid_left = grid_left.max(0.0) as f32;
                let grid_top = grid_top.max(0.0) as f32;
                let new_destination: Point = (grid_left, grid_top).into();
                let new_grid_size: Dimensions = grid_size.into();

                if self.grid_destination != new_destination {
                    if self.grid_start_position.x.abs() > f32::EPSILON
                        || self.grid_start_position.y.abs() > f32::EPSILON
                    {
                        self.position_t = 0.0; // Reset animation as we have a new destination.
                        self.grid_start_position = self.grid_current_position;
                    } else {
                        // We don't want to animate since the window is animating out of the start location,
                        // so we set t to 2.0 to stop animations.
                        self.position_t = 2.0;
                        self.grid_start_position = new_destination;
                    }
                    self.grid_destination = new_destination;
                }

                let height = new_grid_size.height as usize;
                self.actual_lines.resize(height, None);
                self.grid_size = new_grid_size;

                self.scrollback_lines.resize(2 * height, None);
                self.scrollback_lines.clone_from_iter(&self.actual_lines);
                self.scroll_delta = 0;

                self.anchor_info = anchor_info;
                self.window_type = window_type;

                if self.hidden {
                    self.hidden = false;
                    self.position_t = 2.0; // We don't want to animate since the window is becoming visible,
                                           // so we set t to 2.0 to stop animations.
                    self.grid_start_position = new_destination;
                    self.grid_destination = new_destination;
                }
                self.scroll_animation.reset();
            }
            WindowDrawCommand::DrawLine {
                row,
                line_fragments,
            } => {
                tracy_zone!("draw_line_cmd", 0);

                let check_border = |fragment: &LineFragment, check: &dyn Fn(&str) -> bool| {
                    fragment.style.as_ref().map_or(false, |style| {
                        style.infos.last().map_or(false, |info| {
                            // The specification seems to indicate that kind should be UI and
                            // then we only need to test ui_name. But at least for FloatTitle,
                            // that is not the case, the kind is set to syntax and hi_name is
                            // set.
                            check(&info.ui_name) || check(&info.hi_name)
                        })
                    })
                };

                let float_border =
                    |s: &str| matches!(s, "FloatBorder" | "FloatTitle" | "FloatFooter");
                let winbar = |s: &str| matches!(s, "WinBar" | "WinBarNC");

                // Lines with purly border highlight groups are considered borders.
                let mut is_border = line_fragments
                    .iter()
                    .map(|fragment| check_border(fragment, &float_border))
                    .all(|v| v);

                // And also lines with a winbar highlight anywhere
                is_border |= line_fragments
                    .iter()
                    .map(|fragment| check_border(fragment, &winbar))
                    .any(|v| v);

                self.actual_lines[row] = Some(Rc::new(RefCell::new(Line {
                    line_fragments,
                    background_picture: None,
                    foreground_picture: None,
                    has_transparency: false,
                    is_border,
                    is_valid: false,
                })));
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
                    && bottom == self.grid_size.height
                    && left == 0
                    && right == self.grid_size.width
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
            _ => {}
        };
    }

    fn calculate_borders(&mut self) {
        let top_border = self
            .actual_lines
            .iter()
            .take_while(|line| {
                if let Some(line) = line {
                    line.borrow().is_border
                } else {
                    false
                }
            })
            .count();
        let bottom_border = (top_border..self.actual_lines.len())
            .rev()
            .map(|i| self.actual_lines[i].as_ref())
            .take_while(|line| {
                if let Some(line) = line {
                    line.borrow().is_border
                } else {
                    false
                }
            })
            .count();
        self.top_border = 0..top_border as isize;
        self.bottom_border =
            (self.actual_lines.len() - bottom_border) as isize..self.actual_lines.len() as isize
    }

    pub fn flush(&mut self, renderer_settings: &RendererSettings) {
        self.calculate_borders();

        // If the borders are changed, reset the scrollback to only fit the inner view
        let inner_range = self.top_border.end..self.bottom_border.start;
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

    pub fn prepare_lines(&mut self, grid_renderer: &mut GridRenderer) {
        let scroll_offset_lines = self.scroll_animation.position.floor() as isize;
        let height = self.grid_size.height as isize;
        if height == 0 {
            return;
        }
        let font_dimensions = grid_renderer.font_dimensions;

        let mut prepare_line = |line: &Rc<RefCell<Line>>| {
            let mut line = line.borrow_mut();
            if line.is_valid {
                return;
            }

            let mut recorder = PictureRecorder::new();

            let grid_rect = Rect::from_wh(
                (self.grid_size.width * font_dimensions.width) as f32,
                font_dimensions.height as f32,
            );
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
                let grid_position = (*window_left, 0);
                let background_info =
                    grid_renderer.draw_background(canvas, grid_position, *width, style);
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
                let grid_position = (*window_left, 0);

                foreground_drawn |=
                    grid_renderer.draw_foreground(canvas, text, grid_position, *width, style);
            }
            let foreground_picture =
                foreground_drawn.then_some(recorder.finish_recording_as_picture(None).unwrap());

            line.background_picture = background_picture;
            line.foreground_picture = foreground_picture;
            line.has_transparency = has_transparency;
            line.is_valid = true;
        };

        for line in self
            .scrollback_lines
            .iter_range_mut(scroll_offset_lines..scroll_offset_lines + height + 1)
            .flatten()
        {
            prepare_line(line)
        }

        for line in self
            .actual_lines
            .iter_range_mut(self.top_border.clone())
            .flatten()
        {
            prepare_line(line)
        }
        for line in self
            .actual_lines
            .iter_range_mut(self.bottom_border.clone())
            .flatten()
        {
            prepare_line(line)
        }
    }
}
