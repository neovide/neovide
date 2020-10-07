use skulpin::skia_safe::gpu::SurfaceOrigin;
use skulpin::skia_safe::{
    Budgeted, Canvas, ImageInfo, Rect, Surface, Point, Paint, Color, BlendMode, image_filters::blur
};
use skulpin::skia_safe::canvas::{
    SaveLayerRec,
    SrcRectConstraint
};

use super::{Renderer, RendererSettings};
use super::animation_utils::*;
use crate::editor::WindowDrawCommand;
use crate::redraw_scheduler::REDRAW_SCHEDULER;

fn build_window_surface(
    parent_canvas: &mut Canvas,
    renderer: &Renderer,
    grid_width: u64,
    grid_height: u64,
) -> Surface {
    let dimensions = ((grid_width as f32 * renderer.font_width) as i32, (grid_height as f32 * renderer.font_height) as i32);
    let mut context = parent_canvas.gpu_context().unwrap();
    let budgeted = Budgeted::Yes;
    let parent_image_info = parent_canvas.image_info();
    let image_info = ImageInfo::new(
        dimensions,
        parent_image_info.color_type(),
        parent_image_info.alpha_type(),
        parent_image_info.color_space(),
    );
    let surface_origin = SurfaceOrigin::TopLeft;
    let mut surface = Surface::new_render_target(
        &mut context,
        budgeted,
        &image_info,
        None,
        surface_origin,
        None,
        None,
    )
    .expect("Could not create surface");
    let canvas = surface.canvas();

    canvas.clear(renderer.default_style.colors.background.clone().unwrap().to_color());
    surface
}

pub struct RenderedWindow {
    background_surface: Surface,
    foreground_surface: Surface,
    pub id: u64,
    pub hidden: bool,
    pub floating: bool,

    grid_width: u64, grid_height: u64,

    grid_start_position: Point,
    grid_current_position: Point,
    grid_destination: Point,
    t: f32
}

impl RenderedWindow {
    pub fn new(parent_canvas: &mut Canvas, renderer: &Renderer, id: u64, grid_position: Point, grid_width: u64, grid_height: u64) -> RenderedWindow {
        let background_surface = build_window_surface(parent_canvas, renderer, grid_width, grid_height);
        let foreground_surface = build_window_surface(parent_canvas, renderer, grid_width, grid_height);

        RenderedWindow {
            background_surface,
            foreground_surface,
            id,
            hidden: false,
            floating: false,

            grid_width,
            grid_height,

            grid_start_position: grid_position,
            grid_current_position: grid_position,
            grid_destination: grid_position,
            t: 2.0 // 2.0 is out of the 0.0 to 1.0 range and stops animation
        }
    }

    pub fn update(
        &mut self,
        settings: &RendererSettings,
        dt: f32
    ) -> bool {
        if (self.t - 1.0).abs() < std::f32::EPSILON {
            return false;
        }

        if (self.t - 1.0).abs() < std::f32::EPSILON {
            // We are at destination, move t out of 0-1 range to stop the animation
            self.t = 2.0;
        } else {
            self.t = (self.t + dt / settings.animation_length).min(1.0);
        }

        self.grid_current_position = ease_point(
            ease_out_expo,
            self.grid_start_position,
            self.grid_destination,
            self.t,
        );

        true
    }

    pub fn draw(
        &mut self,
        root_canvas: &mut Canvas,
        settings: &RendererSettings,
        font_width: f32,
        font_height: f32,
        dt: f32
        ) -> (u64, Rect) {
        let current_pixel_position = Point::new(self.grid_current_position.x * font_width, self.grid_current_position.y * font_height);

        let image_width = (self.grid_width as f32 * font_width) as i32;
        let image_height = (self.grid_height as f32 * font_height) as i32;

        if self.update(settings, dt) {
            REDRAW_SCHEDULER.queue_next_frame();
        }

        root_canvas.save();

        let region = Rect::from_point_and_size(current_pixel_position, (image_width, image_height));
        root_canvas.clip_rect(&region, None, Some(false));

        if self.floating && settings.floating_blur {
            let blur = blur((2.0, 2.0), None, None, None).unwrap();
            let save_layer_rec = SaveLayerRec::default()
                .backdrop(&blur)
                .bounds(&region);

            root_canvas.save_layer(&save_layer_rec);
        }

        let mut paint = Paint::default();

        if self.floating {
            let a = (settings.floating_opacity.min(1.0).max(0.0) * 255.0) as u8;
            paint.set_color(Color::from_argb(a, 255, 255, 255));
        }

        self.background_surface.draw(root_canvas.as_mut(), (current_pixel_position.x, current_pixel_position.y), Some(&paint));

        let mut paint = Paint::default();
        paint.set_blend_mode(BlendMode::SrcOver);

        self.foreground_surface.draw(root_canvas.as_mut(), (current_pixel_position.x, current_pixel_position.y), Some(&paint));

        if self.floating {
            root_canvas.restore();
        }

        root_canvas.restore();

        let window_position = current_pixel_position.clone();

        (self.id, Rect::from_point_and_size(window_position, (image_width as f32, image_height as f32)))
    }

    pub fn handle_window_draw_command(mut self, renderer: &mut Renderer, draw_command: WindowDrawCommand) -> Self {
        match draw_command {
            WindowDrawCommand::Position {
                grid_left, grid_top,
                width: grid_width, 
                height: grid_height,
                floating
            } => {
                let new_destination: Point = (grid_left as f32, grid_top as f32).into();

                if self.grid_destination != new_destination {
                    if self.grid_start_position.x.abs() > f32::EPSILON || self.grid_start_position.y.abs() > f32::EPSILON {
                        self.t = 0.0; // Reset animation as we have a new destination.
                        self.grid_start_position = self.grid_current_position;
                        self.grid_destination = new_destination;
                    } else {
                        // We don't want to animate since the window is animating out of the start location, 
                        // so we set t to 2.0 to stop animations.
                        self.t = 2.0; 
                        self.grid_start_position = new_destination;
                        self.grid_destination = new_destination;
                    }
                }

                if grid_width != self.grid_width || grid_height != self.grid_height {
                    {
                        let mut old_background = self.background_surface;
                        self.background_surface = build_window_surface(old_background.canvas(), &renderer, grid_width, grid_height);
                        old_background.draw(self.background_surface.canvas(), (0.0, 0.0), None);
                    }

                    {
                        let mut old_foreground = self.foreground_surface;
                        self.foreground_surface = build_window_surface(old_foreground.canvas(), &renderer, grid_width, grid_height);
                        old_foreground.draw(self.foreground_surface.canvas(), (0.0, 0.0), None);
                    }

                    self.grid_width = grid_width;
                    self.grid_height = grid_height;
                }

                self.floating = floating;

                if self.hidden {
                    self.hidden = false;
                    self.t = 2.0; // We don't want to animate since the window is becoming visible, so we set t to 2.0 to stop animations.
                    self.grid_start_position = new_destination;
                    self.grid_destination = new_destination;
                }
            },
            WindowDrawCommand::Cell {
                text, cell_width, window_left, window_top, style
            } => {
                let grid_position = (window_left, window_top);

                // println!("{} left: {} top: {}", text, window_left, window_top);

                {
                    let mut background_canvas = self.background_surface.canvas();
                    renderer.draw_background(
                        &mut background_canvas,
                        grid_position,
                        cell_width,
                        &style
                    );
                }

                {
                    let mut foreground_canvas = self.foreground_surface.canvas();
                    renderer.draw_foreground(
                        &mut foreground_canvas,
                        &text,
                        grid_position,
                        cell_width,
                        &style,
                    );
                }
            },
            WindowDrawCommand::Scroll {
                top, bot, left, right, rows, cols
            } => {
                let scrolled_region = Rect::new(
                    left as f32 * renderer.font_width,
                    top as f32 * renderer.font_height,
                    right as f32 * renderer.font_width,
                    bot as f32 * renderer.font_height);

                {
                    let background_snapshot = self.background_surface.image_snapshot();
                    let background_canvas = self.background_surface.canvas();

                    background_canvas.save();
                    background_canvas.clip_rect(scrolled_region, None, Some(false));

                    let mut translated_region = scrolled_region.clone();
                    translated_region.offset((-cols as f32 * renderer.font_width, -rows as f32 * renderer.font_height));

                    background_canvas.draw_image_rect(background_snapshot, Some((&scrolled_region, SrcRectConstraint::Fast)), translated_region, &renderer.paint);

                    background_canvas.restore();
                }

                {
                    let foreground_snapshot = self.foreground_surface.image_snapshot();
                    let foreground_canvas = self.foreground_surface.canvas();

                    foreground_canvas.save();
                    foreground_canvas.clip_rect(scrolled_region, None, Some(false));

                    let mut translated_region = scrolled_region.clone();
                    translated_region.offset((-cols as f32 * renderer.font_width, -rows as f32 * renderer.font_height));

                    foreground_canvas.draw_image_rect(foreground_snapshot, Some((&scrolled_region, SrcRectConstraint::Fast)), translated_region, &renderer.paint);

                    foreground_canvas.restore();
                }
            },
            WindowDrawCommand::Clear => {
                let background_canvas = self.background_surface.canvas();
                self.background_surface = build_window_surface(background_canvas, &renderer, self.grid_width, self.grid_height);

                let foreground_canvas = self.foreground_surface.canvas();
                self.foreground_surface = build_window_surface(foreground_canvas, &renderer, self.grid_width, self.grid_height);
            },
            WindowDrawCommand::Show => {
                if self.hidden {
                    self.hidden = false;
                    self.t = 2.0; // We don't want to animate since the window is becoming visible, so we set t to 2.0 to stop animations.
                    self.grid_start_position = self.grid_destination;
                    self.grid_destination = self.grid_destination;
                }
            },
            WindowDrawCommand::Hide => self.hidden = true,
            _ => {}
        };

        self
    }
}
