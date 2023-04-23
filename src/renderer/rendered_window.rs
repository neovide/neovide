use std::ops::Range;
use std::sync::Arc;

/*
use skia_safe::{
    canvas::SaveLayerRec,
    gpu::{Budgeted, SurfaceOrigin},
    image_filters::blur,
    BlendMode, Canvas, Color, ImageInfo, Matrix, Paint, Picture, PictureRecorder, Point, Rect,
    Surface, SurfaceProps, SurfacePropsFlags,
};
*/
use csscolorparser::Color;
use euclid::default::{Point2D, Rect, Size2D, Vector2D};

use super::fonts::caching_shaper::CachingShaper;
use crate::{
    dimensions::Dimensions,
    editor::Style,
    profiling::tracy_zone,
    renderer::{
        animation_utils::*,
        pipeline::{BackgroundFragment, GlyphFragment},
        GlyphPlaceholder, GridRenderer, MainRenderPass, RendererSettings, WGpuRenderer,
    },
};

use wgpu::Buffer;

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

/*
fn build_window_surface(parent_canvas: &mut Canvas, pixel_size: (i32, i32)) -> Surface {
    let mut context = parent_canvas.recording_context().unwrap();
    let budgeted = Budgeted::Yes;
    let parent_image_info = parent_canvas.image_info();
    let image_info = ImageInfo::new(
        pixel_size,
        parent_image_info.color_type(),
        parent_image_info.alpha_type(),
        parent_image_info.color_space(),
    );
    let surface_origin = SurfaceOrigin::TopLeft;
    // Subpixel layout (should be configurable/obtained from fontconfig).
    let props = SurfaceProps::new(SurfacePropsFlags::default(), skia_safe::PixelGeometry::RGBH);
    Surface::new_render_target(
        &mut context,
        budgeted,
        &image_info,
        None,
        surface_origin,
        Some(&props),
        None,
    )
    .expect("Could not create surface")
}

fn build_window_surface_with_grid_size(
    parent_canvas: &mut Canvas,
    grid_renderer: &GridRenderer,
    grid_size: Dimensions,
) -> Surface {
    let mut surface = build_window_surface(
        parent_canvas,
        (grid_size * grid_renderer.font_dimensions).into(),
    );

    let canvas = surface.canvas();
    canvas.clear(grid_renderer.get_default_background());
    surface
}
*/

pub struct LocatedSurface {
    pub vertical_position: f32,
}

impl LocatedSurface {
    fn new(
        grid_renderer: &GridRenderer,
        grid_size: Dimensions,
        vertical_position: f32,
    ) -> LocatedSurface {
        LocatedSurface { vertical_position }
    }
}

#[derive(Clone)]
struct Line {
    background: Vec<BackgroundFragment>,
    glyphs: Vec<GlyphPlaceholder>,
    has_transparency: bool,
    line_fragments: Vec<LineFragment>,
}

pub struct RenderedWindow {
    pub current_surface: LocatedSurface,

    pub id: u64,
    pub hidden: bool,
    pub floating_order: Option<u64>,

    pub grid_size: Dimensions,

    scrollback_lines: Vec<Option<Line>>,
    actual_lines: Vec<Option<Line>>,
    actual_top_index: isize,
    scrollback_top_index: isize,

    grid_start_position: Point2D<f32>,
    pub grid_current_position: Point2D<f32>,
    grid_destination: Point2D<f32>,
    position_t: f32,

    pub current_scroll: f32,
    scroll_v: f32,
    scroll_delta: isize,

    pub padding: WindowPadding,

    background_range: Range<u64>,
    glyph_range: Range<u64>,
    has_transparency: bool,
}

#[derive(Clone, Debug)]
pub struct WindowDrawDetails {
    pub id: u64,
    pub region: Rect<f32>,
    pub floating_order: Option<u64>,
}

impl RenderedWindow {
    pub fn new(
        grid_renderer: &GridRenderer,
        id: u64,
        grid_position: Point2D<f32>,
        grid_size: Dimensions,
        padding: WindowPadding,
    ) -> RenderedWindow {
        let current_surface = LocatedSurface::new(grid_renderer, grid_size, 0.);

        RenderedWindow {
            current_surface,
            id,
            hidden: false,
            floating_order: None,

            grid_size,

            actual_lines: vec![None; grid_size.height as usize],
            scrollback_lines: vec![None; 2 * grid_size.height as usize],
            actual_top_index: 0,
            scrollback_top_index: 0,

            grid_start_position: grid_position,
            grid_current_position: grid_position,
            grid_destination: grid_position,
            position_t: 2.0, // 2.0 is out of the 0.0 to 1.0 range and stops animation.

            current_scroll: 0.0,
            scroll_v: 0.0,
            scroll_delta: 0,
            padding,
            has_transparency: false,
            background_range: 0..0,
            glyph_range: 0..0,
        }
    }

    pub fn pixel_region(&self, font_dimensions: &Dimensions) -> Rect<f32> {
        let current_pixel_position = Point2D::new(
            self.grid_current_position.x * font_dimensions.width as f32,
            self.grid_current_position.y * font_dimensions.height as f32,
        );

        let image_size = self.grid_size * *font_dimensions;

        Rect::new(current_pixel_position, image_size.into())
    }

    pub fn animate(&mut self, settings: &RendererSettings, dt: f32) -> bool {
        let mut animating = false;

        {
            if 1.0 - self.position_t < std::f32::EPSILON {
                // We are at destination, move t out of 0-1 range to stop the animation.
                self.position_t = 2.0;
            } else {
                animating = true;
                self.position_t =
                    (self.position_t + dt / settings.position_animation_length).min(1.0);
            }

            self.grid_current_position = ease_point(
                ease_out_expo,
                self.grid_start_position,
                self.grid_destination,
                self.position_t,
            );
        }

        {
            let scroll_destination = 0.0;
            let zeta = 1.0;
            let omega = 4.0 / (zeta * settings.scroll_animation_length);
            let k_p = omega * omega;
            let k_d = -2.0 * zeta * omega;
            let acc = k_p * (scroll_destination - self.current_scroll) + k_d * self.scroll_v;
            self.scroll_v += acc * dt;
            self.current_scroll += self.scroll_v * dt;

            if (self.current_scroll - scroll_destination).abs() < 0.01 {
                self.reset_scroll();
            } else {
                animating = true;
            }
        }

        animating
    }

    pub fn draw_surface(
        &mut self,
        font_dimensions: &Dimensions,
        default_background: &Color,
        background_fragments: &mut Vec<BackgroundFragment>,
        glyph_fragments: &mut Vec<GlyphFragment>,
        shaper: &CachingShaper,
    ) {
        let image_size: (i32, i32) = (self.grid_size * *font_dimensions).into();
        //let pixel_region = Rect::from_size(image_size);

        let scroll_offset_lines = self.current_scroll.floor();
        let scroll_offset = scroll_offset_lines - self.current_scroll;
        let scroll_offset_pixels = (scroll_offset * font_dimensions.height as f32).round() as isize;
        let mut has_transparency = false;

        //let mut background_paint = Paint::default();
        //background_paint.set_blend_mode(BlendMode::Src);
        //background_paint.set_alpha(default_background.a());

        // HACK the position
        let pixel_region = self.pixel_region(font_dimensions);

        let lines: Vec<(f32, &Line)> = (0..self.grid_size.height as isize + 1)
            .filter_map(|i| {
                let line_index = (self.scrollback_top_index + scroll_offset_lines as isize + i)
                    .rem_euclid(self.scrollback_lines.len() as isize)
                    as usize;
                if let Some(line) = &self.scrollback_lines[line_index] {
                    let y = (scroll_offset_pixels + (i * font_dimensions.height as isize)) as f32;
                    Some((y, line))
                } else {
                    None
                }
            })
            .collect();

        let new_fragments = lines.iter().flat_map(|(y, line)| {
            line.background.iter().map(|fragment| BackgroundFragment {
                position: [
                    fragment.position[0] + pixel_region.min_x(),
                    *y + pixel_region.min_y(),
                ],
                ..*fragment
            })
        });
        let start_index = background_fragments.len();
        background_fragments.extend(new_fragments);
        self.background_range = start_index as u64..background_fragments.len() as u64;

        let new_fragments = lines.iter().flat_map(|(y, line)| {
            line.glyphs.iter().filter_map(|placeholder| {
                if let Some(coord) = shaper.get_glyph_coordinate(placeholder.id) {
                    let translate = Vector2D::new(
                        pixel_region.min_x() + placeholder.position[0],
                        *y + pixel_region.min_y() + placeholder.position[1],
                    );
                    let rect = coord.dst_rect.translate(translate);

                    Some(GlyphFragment {
                        rect: [rect.min_x(), rect.min_y(), rect.width(), rect.height()],
                        color: placeholder.color,
                        texture: coord.texture_id,
                        uv: [
                            coord.rect.origin.x,
                            coord.rect.origin.y,
                            coord.rect.size.width,
                            coord.rect.size.height,
                        ],
                    })
                } else {
                    None
                }
            })
        });
        let start_index = glyph_fragments.len();
        glyph_fragments.extend(new_fragments);
        self.glyph_range = start_index as u64..glyph_fragments.len() as u64;

        /*
        let mut foreground_paint = Paint::default();
        foreground_paint.set_blend_mode(BlendMode::SrcOver);
        for (matrix, line) in &lines {
            if let Some(foreground_picture) = &line.foreground_picture {
                canvas.draw_picture(foreground_picture, Some(matrix), Some(&foreground_paint));
            }
        }
        */
        self.has_transparency = has_transparency;
    }

    pub fn draw(
        &mut self,
        render_pass: &mut MainRenderPass,
        settings: &RendererSettings,
        default_background: &Color,
        font_dimensions: &Dimensions,
    ) -> WindowDrawDetails {
        let has_transparency = self.has_transparency;

        let pixel_region = self.pixel_region(font_dimensions);
        let transparent_floating = self.floating_order.is_some() && has_transparency;
        render_pass.draw_window(&self.background_range, &self.glyph_range);
        /*

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
            .set_color(Color::from_argb(255, 255, 255, 255))
            .set_blend_mode(if self.floating_order.is_some() {
                BlendMode::SrcOver
            } else {
                BlendMode::Src
            })
            .to_owned();

        // Draw current surface.
        let snapshot = self.current_surface.surface.image_snapshot();
        root_canvas.draw_image_rect(snapshot, None, pixel_region, &paint);

        root_canvas.restore();
        */

        WindowDrawDetails {
            id: self.id,
            region: pixel_region,
            floating_order: self.floating_order,
        }
    }

    fn reset_scroll(&mut self) {
        self.current_scroll = 0.0;
        self.scroll_v = 0.0;
    }

    pub fn handle_window_draw_command(
        &mut self,
        grid_renderer: &mut GridRenderer,
        draw_command: WindowDrawCommand,
        renderer_settings: &RendererSettings,
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
                let new_destination: Point2D<f32> =
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

                if self.grid_size != new_grid_size {
                    /*
                    self.current_surface.surface = build_window_surface_with_grid_size(
                        self.current_surface.surface.canvas(),
                        grid_renderer,
                        new_grid_size,
                    );
                    */
                    self.grid_size = new_grid_size;
                }

                // This could perhaps be optimized, setting the position does not necessarily need
                // to rezize
                self.scrollback_lines = vec![None; 2 * new_grid_size.height as usize];
                self.actual_lines = vec![None; new_grid_size.height as usize];
                self.actual_top_index = 0;
                self.scrollback_top_index = 0;

                self.floating_order = floating_order;

                if self.hidden {
                    self.hidden = false;
                    self.position_t = 2.0; // We don't want to animate since the window is becoming visible,
                                           // so we set t to 2.0 to stop animations.
                    self.grid_start_position = new_destination;
                    self.grid_destination = new_destination;
                }
                self.reset_scroll();
            }
            WindowDrawCommand::DrawLine {
                row,
                line_fragments,
            } => {
                tracy_zone!("draw_line_cmd", 0);
                let font_dimensions = grid_renderer.font_dimensions;

                /*
                let grid_rect = Rect::from_wh(
                    (self.grid_size.width * font_dimensions.width) as f32,
                    font_dimensions.height as f32,
                );
                */

                let line_index = (self.actual_top_index + row as isize)
                    .rem_euclid(self.actual_lines.len() as isize)
                    as usize;

                let mut has_transparency = false;
                let mut custom_background = false;

                // Draw the foreground and glyphs first to give more time for the rasterizer to
                // finish

                let mut glyphs = Vec::new();
                for line_fragment in &line_fragments {
                    let LineFragment {
                        text,
                        window_left,
                        width,
                        style,
                        ..
                    } = line_fragment;
                    let grid_position = (*window_left, 0);
                    grid_renderer.draw_foreground(text, grid_position, *width, style, &mut glyphs);
                }

                let background = line_fragments
                    .iter()
                    .map(|line_fragment| {
                        let LineFragment {
                            window_left,
                            width,
                            style,
                            ..
                        } = line_fragment;
                        let grid_position = (*window_left, 0);
                        let background_fragment =
                            grid_renderer.draw_background(grid_position, *width, style);
                        let transparent = background_fragment.color[3] < 1.0;
                        has_transparency |= transparent;
                        background_fragment
                    })
                    .collect();

                self.actual_lines[line_index] = Some(Line {
                    glyphs,
                    background,
                    has_transparency,
                    line_fragments,
                });
                // Also update the scrollback buffer if there's no scroll in progress
                if self.scroll_delta == 0 {
                    let scrollback_index = (self.scrollback_top_index + row as isize)
                        .rem_euclid(self.scrollback_lines.len() as isize)
                        as usize;
                    self.scrollback_lines[scrollback_index] = self.actual_lines[line_index].clone();
                }
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
                self.scrollback_lines
                    .iter_mut()
                    .for_each(|line| *line = None);
                self.reset_scroll();
                /*
                self.current_surface.surface = build_window_surface_with_grid_size(
                    self.current_surface.surface.canvas(),
                    grid_renderer,
                    self.grid_size,
                );
                */
            }
            WindowDrawCommand::Show => {
                tracy_zone!("show_cmd", 0);
                if self.hidden {
                    self.hidden = false;
                    self.position_t = 2.0; // We don't want to animate since the window is becoming visible,
                                           // so we set t to 2.0 to stop animations.
                    self.grid_start_position = self.grid_destination;
                    self.reset_scroll();
                }
            }
            WindowDrawCommand::Hide => {
                tracy_zone!("hide_cmd", 0);
                self.hidden = true;
            }
            WindowDrawCommand::Viewport { scroll_delta } => {
                // The scroll delta is unfortunately buggy in the current version of Neovim. For more details see:
                // So just store the delta, and commit the actual scrolling when receiving a Viewport command without a delta
                // https://github.com/neovide/neovide/pull/1790
                let scroll_delta = scroll_delta.round() as isize;
                if scroll_delta.unsigned_abs() > 0 {
                    self.scroll_delta = scroll_delta;
                } else {
                    let scroll_delta = self.scroll_delta;
                    self.scroll_delta = 0;
                    self.scrollback_top_index += scroll_delta;

                    for i in 0..self.actual_lines.len() {
                        let scrollback_index = (self.scrollback_top_index + i as isize)
                            .rem_euclid(self.scrollback_lines.len() as isize)
                            as usize;
                        let actual_index = (self.actual_top_index + i as isize)
                            .rem_euclid(self.actual_lines.len() as isize)
                            as usize;
                        self.scrollback_lines[scrollback_index] =
                            self.actual_lines[actual_index].clone();
                    }

                    let mut scroll_offset = self.current_scroll;

                    let minmax = self.scrollback_lines.len() - self.grid_size.height as usize;
                    // Do a limited scroll with empty lines when scrolling far
                    if scroll_delta.unsigned_abs() > minmax {
                        let far_lines = renderer_settings
                            .scroll_animation_far_scroll_lines
                            .min(self.actual_lines.len() as u32)
                            as isize;

                        scroll_offset = (far_lines * scroll_delta.signum()) as f32;
                        let empty_lines = if scroll_delta > 0 {
                            self.actual_lines.len() as isize
                                ..self.actual_lines.len() as isize + far_lines
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
                    self.current_scroll = scroll_offset;
                }
            }
            _ => {}
        };
    }

    pub fn redraw_foreground(&mut self, grid_renderer: &mut GridRenderer) {
        let mut draw_line = |line: &mut Option<Line>| {
            if let Some(line) = line.as_mut() {
                let line_fragments = &line.line_fragments;
                let mut glyphs = &mut line.glyphs;
                glyphs.clear();
                for line_fragment in line_fragments.iter() {
                    let LineFragment {
                        text,
                        window_left,
                        width,
                        style,
                        ..
                    } = line_fragment;
                    let grid_position = (*window_left, 0);
                    grid_renderer.draw_foreground(text, grid_position, *width, style, &mut glyphs);
                }
            }
        };
        for line in &mut self.scrollback_lines {
            draw_line(line);
        }
        for line in &mut self.actual_lines {
            draw_line(line);
        }
    }
}
