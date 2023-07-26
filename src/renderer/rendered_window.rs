use std::sync::{Arc, Mutex};

use skia_safe::{
    canvas::SaveLayerRec, image_filters::blur, BlendMode, Canvas, Color, Matrix, Paint, Picture,
    PictureRecorder, Point, Rect,
};

use crate::{
    dimensions::Dimensions,
    editor::Style,
    profiling::tracy_zone,
    renderer::{animation_utils::*, GridRenderer, RendererSettings},
};

#[derive(Clone, Debug)]
pub struct LineFragment {
    pub text: String,
    pub window_left: u64,
    pub width: u64,
    pub style: Option<Arc<Style>>,
}

#[derive(Clone, Debug)]
pub enum WindowDrawCommand {
    Position {
        grid_position: (f64, f64),
        grid_size: (u64, u64),
        floating_order: Option<u64>,
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

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct WindowPadding {
    pub top: u32,
    pub left: u32,
    pub right: u32,
    pub bottom: u32,
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
    pub vertical_position: f32,

    pub id: u64,
    pub hidden: bool,
    pub floating_order: Option<u64>,

    pub grid_size: Dimensions,

    scrollback_lines: Vec<Option<Arc<Mutex<Line>>>>,
    actual_lines: Vec<Option<Arc<Mutex<Line>>>>,
    actual_top_index: isize,
    scrollback_top_index: isize,
    scroll_delta: isize,

    grid_start_position: Point,
    pub grid_current_position: Point,
    grid_destination: Point,
    position_t: f32,

    pub scroll_animation: CriticallyDampedSpringAnimation,

    pub padding: WindowPadding,

    has_transparency: bool,
}

#[derive(Clone, Debug)]
pub struct WindowDrawDetails {
    pub id: u64,
    pub region: Rect,
    pub floating_order: Option<u64>,
}

impl RenderedWindow {
    pub fn new(
        id: u64,
        grid_position: Point,
        grid_size: Dimensions,
        padding: WindowPadding,
    ) -> RenderedWindow {
        RenderedWindow {
            vertical_position: 0.0,
            id,
            hidden: false,
            floating_order: None,

            grid_size,

            actual_lines: vec![None; grid_size.height as usize],
            scrollback_lines: vec![None; 2 * grid_size.height as usize],
            actual_top_index: 0,
            scrollback_top_index: 0,
            scroll_delta: 0,

            grid_start_position: grid_position,
            grid_current_position: grid_position,
            grid_destination: grid_position,
            position_t: 2.0, // 2.0 is out of the 0.0 to 1.0 range and stops animation.

            scroll_animation: CriticallyDampedSpringAnimation::new(),

            padding,
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

    pub fn animate(&mut self, settings: &RendererSettings, dt: f32) -> bool {
        let mut animating = false;

        if 1.0 - self.position_t < std::f32::EPSILON {
            // We are at destination, move t out of 0-1 range to stop the animation.
            self.position_t = 2.0;
        } else {
            animating = true;
            self.position_t = (self.position_t + dt / settings.position_animation_length).min(1.0);
        }

        self.grid_current_position = ease_point(
            ease_out_expo,
            self.grid_start_position,
            self.grid_destination,
            self.position_t,
        );

        animating |= self
            .scroll_animation
            .update(dt, settings.scroll_animation_length);

        animating
    }

    pub fn draw_surface(
        &mut self,
        canvas: &mut Canvas,
        pixel_region: &Rect,
        font_dimensions: Dimensions,
        default_background: Color,
    ) {
        let scroll_offset_lines = self.scroll_animation.position.floor();
        let scroll_offset = scroll_offset_lines - self.scroll_animation.position;
        let scroll_offset_pixels = (scroll_offset * font_dimensions.height as f32).round() as isize;
        let mut has_transparency = false;

        let lines: Vec<(Matrix, &Arc<Mutex<Line>>)> = (0..self.grid_size.height as isize + 1)
            .filter_map(|i| {
                let line_index = (self.scrollback_top_index + scroll_offset_lines as isize + i)
                    .rem_euclid(self.scrollback_lines.len() as isize)
                    as usize;
                if let Some(line) = &self.scrollback_lines[line_index] {
                    let mut m = Matrix::new_identity();
                    m.set_translate((
                        pixel_region.left(),
                        pixel_region.top()
                            + (scroll_offset_pixels + (i * font_dimensions.height as isize)) as f32,
                    ));
                    Some((m, line))
                } else {
                    None
                }
            })
            .collect();

        let mut background_paint = Paint::default();
        background_paint.set_blend_mode(BlendMode::Src);
        background_paint.set_alpha(default_background.a());

        let save_layer_rec = SaveLayerRec::default()
            .bounds(pixel_region)
            .paint(&background_paint);
        canvas.save_layer(&save_layer_rec);
        canvas.clear(default_background.with_a(255));
        for (matrix, line) in &lines {
            let line = line.lock().unwrap();
            if let Some(background_picture) = &line.background_picture {
                has_transparency |= line.has_transparency;
                canvas.draw_picture(background_picture, Some(matrix), None);
            }
        }
        canvas.restore();

        for (matrix, line) in &lines {
            let line = line.lock().unwrap();
            if let Some(foreground_picture) = &line.foreground_picture {
                canvas.draw_picture(foreground_picture, Some(matrix), None);
            }
        }
        self.has_transparency = has_transparency;
    }

    fn has_transparency(&self) -> bool {
        let scroll_offset_lines = self.scroll_animation.position.floor();
        (0..self.grid_size.height as isize + 1).any(|i| {
            let line_index = (self.scrollback_top_index + scroll_offset_lines as isize + i)
                .rem_euclid(self.scrollback_lines.len() as isize)
                as usize;
            if let Some(line) = &self.scrollback_lines[line_index] {
                let line = line.lock().unwrap();
                line.has_transparency
            } else {
                false
            }
        })
    }

    pub fn draw(
        &mut self,
        root_canvas: &mut Canvas,
        settings: &RendererSettings,
        default_background: Color,
        font_dimensions: Dimensions,
    ) -> WindowDrawDetails {
        let has_transparency = default_background.a() != 255 || self.has_transparency();

        let pixel_region = self.pixel_region(font_dimensions);
        let transparent_floating = self.floating_order.is_some() && has_transparency;

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
            .set_blend_mode(if self.floating_order.is_some() {
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
            floating_order: self.floating_order,
        }
    }

    pub fn handle_window_draw_command(
        &mut self,
        grid_renderer: &mut GridRenderer,
        draw_command: WindowDrawCommand,
    ) {
        match draw_command {
            WindowDrawCommand::Position {
                grid_position: (grid_left, grid_top),
                grid_size,
                floating_order,
            } => {
                tracy_zone!("position_cmd", 0);
                let Dimensions {
                    width: font_width,
                    height: font_height,
                } = grid_renderer.font_dimensions;

                let top_offset = self.padding.top as f32 / font_height as f32;
                let left_offset = self.padding.left as f32 / font_width as f32;

                let grid_left = grid_left.max(0.0);
                let grid_top = grid_top.max(0.0);
                let new_destination: Point =
                    (grid_left as f32 + left_offset, grid_top as f32 + top_offset).into();
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

                let mut new_actual_lines = vec![None; new_grid_size.height as usize];

                for row in 0..self.grid_size.height.min(new_grid_size.height) {
                    let line_index = (self.actual_top_index + row as isize)
                        .rem_euclid(self.actual_lines.len() as isize)
                        as usize;
                    new_actual_lines[row as usize] = self.actual_lines[line_index].clone();
                }
                self.actual_lines = new_actual_lines;
                self.grid_size = new_grid_size;

                self.scrollback_lines
                    .resize(2 * new_grid_size.height as usize, None);
                self.scrollback_lines[..new_grid_size.height as usize]
                    .clone_from_slice(&self.actual_lines);
                self.actual_top_index = 0;
                self.scrollback_top_index = 0;
                self.scroll_delta = 0;

                self.floating_order = floating_order;

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
                log::trace!("Handling DrawLine {}", self.id);
                tracy_zone!("draw_line_cmd", 0);
                let line_index = (self.actual_top_index + row as isize)
                    .rem_euclid(self.actual_lines.len() as isize)
                    as usize;

                self.actual_lines[line_index] = Some(Arc::new(Mutex::new(Line {
                    line_fragments,
                    background_picture: None,
                    foreground_picture: None,
                    has_transparency: false,
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
                    self.actual_top_index += rows as isize;
                }
            }
            WindowDrawCommand::Clear => {
                tracy_zone!("clear_cmd", 0);
                self.actual_top_index = 0;
                self.scrollback_top_index = 0;
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

    pub fn flush(&mut self, renderer_settings: &RendererSettings) {
        let scroll_delta = self.scroll_delta;
        self.scrollback_top_index += scroll_delta;

        for i in 0..self.actual_lines.len() {
            let scrollback_index = (self.scrollback_top_index + i as isize)
                .rem_euclid(self.scrollback_lines.len() as isize)
                as usize;
            let actual_index = (self.actual_top_index + i as isize)
                .rem_euclid(self.actual_lines.len() as isize)
                as usize;
            self.scrollback_lines[scrollback_index] = self.actual_lines[actual_index].clone();
        }

        if scroll_delta != 0 {
            let mut scroll_offset = self.scroll_animation.position;

            let minmax = self.scrollback_lines.len() - self.grid_size.height as usize;
            log::trace!("Scroll offset {scroll_offset}, delta {scroll_delta}, minmax {minmax}");
            // Do a limited scroll with empty lines when scrolling far
            if scroll_delta.unsigned_abs() > minmax {
                let far_lines = renderer_settings
                    .scroll_animation_far_lines
                    .min(self.actual_lines.len() as u32) as isize;

                scroll_offset = (far_lines * scroll_delta.signum()) as f32;
                let empty_lines = if scroll_delta > 0 {
                    self.actual_lines.len() as isize..self.actual_lines.len() as isize + far_lines
                } else {
                    -far_lines..0
                };
                for i in empty_lines {
                    let i = (self.scrollback_top_index + i)
                        .rem_euclid(self.scrollback_lines.len() as isize)
                        as usize;
                    self.scrollback_lines[i] = None;
                }
            // And even when scrolling in steps, we can't let it drift too far, since the
            // buffer size is limited
            } else {
                scroll_offset -= scroll_delta as f32;
                scroll_offset = scroll_offset.clamp(-(minmax as f32), minmax as f32);
            }
            self.scroll_animation.position = scroll_offset;
            log::trace!("Current scroll {scroll_offset}");
        }
        self.scroll_delta = 0;
    }

    pub fn prepare_lines(&mut self, grid_renderer: &mut GridRenderer) {
        let scroll_offset_lines = self.scroll_animation.position.floor();
        let top_index = self.scrollback_top_index;
        let height = self.grid_size.height as isize;
        let len = self.scrollback_lines.len() as isize;
        let dirty_lines: Vec<usize> = (0..height + 1)
            .flat_map(|i| {
                let line_index =
                    (top_index + scroll_offset_lines as isize + i).rem_euclid(len) as usize;

                let line = &self.scrollback_lines[line_index];
                line.as_ref().map(|line| {
                    let is_valid = line.lock().unwrap().is_valid;
                    (!is_valid).then_some(line_index)
                })
            })
            .flatten()
            .collect();

        for line in dirty_lines {
            let line = &mut self.scrollback_lines[line]
                .as_mut()
                .unwrap()
                .lock()
                .unwrap();
            let font_dimensions = grid_renderer.font_dimensions;
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
                let (custom, transparent) =
                    grid_renderer.draw_background(canvas, grid_position, *width, style);
                custom_background |= custom;
                has_transparency |= transparent;
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
        }
    }
}
