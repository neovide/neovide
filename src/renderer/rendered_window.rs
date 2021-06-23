use std::collections::VecDeque;

use skia_safe::canvas::{SaveLayerRec, SrcRectConstraint};
use skia_safe::gpu::SurfaceOrigin;
use skia_safe::{
    image_filters::blur, BlendMode, Budgeted, Canvas, Color, Image, ImageInfo, Paint, Point, Rect,
    SamplingOptions, Surface, SurfaceProps, SurfacePropsFlags,
};

use super::animation_utils::*;
use super::{Renderer, RendererSettings};
use crate::editor::WindowDrawCommand;
use crate::redraw_scheduler::REDRAW_SCHEDULER;

fn build_window_surface(
    parent_canvas: &mut Canvas,
    pixel_width: u64,
    pixel_height: u64,
) -> Surface {
    let dimensions = (pixel_width as i32, pixel_height as i32);
    let mut context = parent_canvas.recording_context().unwrap();
    let budgeted = Budgeted::Yes;
    let parent_image_info = parent_canvas.image_info();
    let image_info = ImageInfo::new(
        dimensions,
        parent_image_info.color_type(),
        parent_image_info.alpha_type(),
        parent_image_info.color_space(),
    );
    let surface_origin = SurfaceOrigin::TopLeft;
    // subpixel layout (should be configurable/obtained from fontconfig)
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
    renderer: &Renderer,
    grid_width: u64,
    grid_height: u64,
    scaling: f32,
) -> Surface {
    let pixel_width = ((grid_width * renderer.font_width) as f32 / scaling) as u64;
    let pixel_height = ((grid_height * renderer.font_height) as f32 / scaling) as u64;
    let mut surface = build_window_surface(parent_canvas, pixel_width, pixel_height);

    let canvas = surface.canvas();
    canvas.clear(renderer.get_default_background());
    surface
}

pub struct LocatedSnapshot {
    image: Image,
    top_line: u64,
}

pub struct LocatedSurface {
    surface: Surface,
    pub top_line: u64,
}

impl LocatedSurface {
    fn new(
        parent_canvas: &mut Canvas,
        renderer: &Renderer,
        grid_width: u64,
        grid_height: u64,
        top_line: u64,
        scaling: f32,
    ) -> LocatedSurface {
        let surface = build_window_surface_with_grid_size(
            parent_canvas,
            renderer,
            grid_width,
            grid_height,
            scaling,
        );

        LocatedSurface { surface, top_line }
    }

    fn snapshot(&mut self) -> LocatedSnapshot {
        let image = self.surface.image_snapshot();
        LocatedSnapshot {
            image,
            top_line: self.top_line,
        }
    }
}

pub struct RenderedWindow {
    snapshots: VecDeque<LocatedSnapshot>,
    pub current_surface: LocatedSurface,

    pub id: u64,
    pub hidden: bool,
    pub floating_order: Option<u64>,

    pub grid_width: u64,
    pub grid_height: u64,

    grid_start_position: Point,
    pub grid_current_position: Point,
    grid_destination: Point,
    position_t: f32,

    start_scroll: f32,
    pub current_scroll: f32,
    scroll_destination: f32,
    scroll_t: f32,
}

#[derive(Clone, Debug)]
pub struct WindowDrawDetails {
    pub id: u64,
    pub region: Rect,
    pub floating_order: Option<u64>,
}

impl RenderedWindow {
    pub fn new(
        parent_canvas: &mut Canvas,
        renderer: &Renderer,
        id: u64,
        grid_position: Point,
        grid_width: u64,
        grid_height: u64,
        scaling: f32,
    ) -> RenderedWindow {
        let current_surface =
            LocatedSurface::new(parent_canvas, renderer, grid_width, grid_height, 0, scaling);

        RenderedWindow {
            snapshots: VecDeque::new(),
            current_surface,
            id,
            hidden: false,
            floating_order: None,

            grid_width,
            grid_height,

            grid_start_position: grid_position,
            grid_current_position: grid_position,
            grid_destination: grid_position,
            position_t: 2.0, // 2.0 is out of the 0.0 to 1.0 range and stops animation

            start_scroll: 0.0,
            current_scroll: 0.0,
            scroll_destination: 0.0,
            scroll_t: 2.0, // 2.0 is out of the 0.0 to 1.0 range and stops animation
        }
    }

    pub fn pixel_region(&self, font_width: u64, font_height: u64) -> Rect {
        let current_pixel_position = Point::new(
            self.grid_current_position.x * font_width as f32,
            self.grid_current_position.y * font_height as f32,
        );

        let image_width = (self.grid_width * font_width) as i32;
        let image_height = (self.grid_height * font_height) as i32;

        Rect::from_point_and_size(current_pixel_position, (image_width, image_height))
    }

    pub fn update(&mut self, settings: &RendererSettings, dt: f32) -> bool {
        let mut animating = false;

        {
            if 1.0 - self.position_t < std::f32::EPSILON {
                // We are at destination, move t out of 0-1 range to stop the animation
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
            if 1.0 - self.scroll_t < std::f32::EPSILON {
                // We are at destination, move t out of 0-1 range to stop the animation
                self.scroll_t = 2.0;
                self.snapshots.clear();
            } else {
                animating = true;
                self.scroll_t = (self.scroll_t + dt / settings.scroll_animation_length).min(1.0);
            }

            self.current_scroll = ease(
                ease_out_expo,
                self.start_scroll,
                self.scroll_destination,
                self.scroll_t,
            );
        }

        animating
    }

    pub fn draw(
        &mut self,
        root_canvas: &mut Canvas,
        settings: &RendererSettings,
        default_background: Color,
        font_width: u64,
        font_height: u64,
        dt: f32,
    ) -> WindowDrawDetails {
        if self.update(settings, dt) {
            REDRAW_SCHEDULER.queue_next_frame();
        }

        let pixel_region = self.pixel_region(font_width, font_height);

        root_canvas.save();
        root_canvas.clip_rect(&pixel_region, None, Some(false));

        if self.floating_order.is_some() && settings.floating_blur {
            let blur = blur((2.0, 2.0), None, None, None).unwrap();
            let save_layer_rec = SaveLayerRec::default()
                .backdrop(&blur)
                .bounds(&pixel_region);

            root_canvas.save_layer(&save_layer_rec);
        }

        let mut paint = Paint::default();
        // We want each surface to overwrite the one underneath and will use layers to ensure
        // only lower priority surfaces will get clobbered and not the underlying windows
        paint.set_blend_mode(BlendMode::Src);
        paint.set_anti_alias(false);

        // Save layer so that setting the blend mode doesn't effect the blur
        root_canvas.save_layer(&SaveLayerRec::default());
        let mut a = 255;
        if self.floating_order.is_some() {
            a = (settings.floating_opacity.min(1.0).max(0.0) * 255.0) as u8;
        }

        paint.set_color(default_background.with_a(a));
        root_canvas.draw_rect(pixel_region, &paint);

        paint.set_color(Color::from_argb(a, 255, 255, 255));

        // Draw scrolling snapshots
        for snapshot in self.snapshots.iter_mut().rev() {
            let scroll_offset = (snapshot.top_line * font_height) as f32
                - (self.current_scroll * font_height as f32);
            let image = &mut snapshot.image;
            root_canvas.draw_image_rect(
                image,
                None,
                pixel_region.with_offset((0.0, scroll_offset as f32)),
                &paint,
            );
        }
        // Draw current surface
        let scroll_offset = (self.current_surface.top_line * font_height) as f32
            - (self.current_scroll * font_height as f32);
        let snapshot = self.current_surface.surface.image_snapshot();
        root_canvas.draw_image_rect(
            snapshot,
            None,
            pixel_region.with_offset((0.0, scroll_offset as f32)),
            &paint,
        );

        root_canvas.restore();

        if self.floating_order.is_some() {
            root_canvas.restore();
        }

        root_canvas.restore();

        WindowDrawDetails {
            id: self.id,
            region: pixel_region,
            floating_order: self.floating_order,
        }
    }

    pub fn handle_window_draw_command(
        mut self,
        renderer: &mut Renderer,
        draw_command: WindowDrawCommand,
        scaling: f32,
    ) -> Self {
        match draw_command {
            WindowDrawCommand::Position {
                grid_left,
                grid_top,
                width: grid_width,
                height: grid_height,
                floating_order,
            } => {
                let new_destination: Point = (grid_left as f32, grid_top as f32).into();

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

                if grid_width != self.grid_width || grid_height != self.grid_height {
                    let mut old_surface = self.current_surface.surface;
                    self.current_surface.surface = build_window_surface_with_grid_size(
                        old_surface.canvas(),
                        renderer,
                        grid_width,
                        grid_height,
                        scaling,
                    );
                    old_surface.draw(
                        self.current_surface.surface.canvas(),
                        (0.0, 0.0),
                        SamplingOptions::default(),
                        None,
                    );

                    self.grid_width = grid_width;
                    self.grid_height = grid_height;
                }

                self.floating_order = floating_order;

                if self.hidden {
                    self.hidden = false;
                    self.position_t = 2.0; // We don't want to animate since the window is becoming visible, so we set t to 2.0 to stop animations.
                    self.grid_start_position = new_destination;
                    self.grid_destination = new_destination;
                }
            }
            WindowDrawCommand::Cells {
                cells,
                window_left,
                window_top,
                width,
                style,
            } => {
                let grid_position = (window_left, window_top);

                let canvas = self.current_surface.surface.canvas();
                canvas.save();
                canvas.scale((1.0 / scaling, 1.0 / scaling));
                renderer.draw_background(canvas, grid_position, width, &style);
                renderer.draw_foreground(canvas, &cells, grid_position, width, &style);
                canvas.restore();
            }
            WindowDrawCommand::Scroll {
                top,
                bot,
                left,
                right,
                rows,
                cols,
            } => {
                let scrolled_region = Rect::new(
                    (left * renderer.font_width) as f32 / scaling,
                    (top * renderer.font_height) as f32 / scaling,
                    (right * renderer.font_width) as f32 / scaling,
                    (bot * renderer.font_height) as f32 / scaling,
                );

                let mut translated_region = scrolled_region;
                translated_region.offset((
                    (-cols * renderer.font_width as i64) as f32 / scaling,
                    (-rows * renderer.font_height as i64) as f32 / scaling,
                ));

                let snapshot = self.current_surface.surface.image_snapshot();
                let canvas = self.current_surface.surface.canvas();

                canvas.save();
                canvas.clip_rect(scrolled_region, None, Some(false));

                canvas.draw_image_rect(
                    snapshot,
                    Some((&scrolled_region, SrcRectConstraint::Fast)),
                    translated_region,
                    &renderer.paint,
                );

                canvas.restore();
            }
            WindowDrawCommand::Clear => {
                self.current_surface.surface = build_window_surface_with_grid_size(
                    self.current_surface.surface.canvas(),
                    renderer,
                    self.grid_width,
                    self.grid_height,
                    scaling,
                );

                self.snapshots.clear();
            }
            WindowDrawCommand::Show => {
                if self.hidden {
                    self.hidden = false;
                    self.position_t = 2.0; // We don't want to animate since the window is becoming visible, so we set t to 2.0 to stop animations.
                    self.grid_start_position = self.grid_destination;
                }
            }
            WindowDrawCommand::Hide => self.hidden = true,
            WindowDrawCommand::Viewport { top_line, .. } => {
                if self.current_surface.top_line != top_line as u64 {
                    let new_snapshot = self.current_surface.snapshot();
                    self.snapshots.push_back(new_snapshot);

                    if self.snapshots.len() > 5 {
                        self.snapshots.pop_front();
                    }

                    self.current_surface.top_line = top_line as u64;

                    // Set new target viewport position and initialize animation timer
                    self.start_scroll = self.current_scroll;
                    self.scroll_destination = top_line as f32;
                    self.scroll_t = 0.0;
                }
            }
            _ => {}
        };

        self
    }
}
