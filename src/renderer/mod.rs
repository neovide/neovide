use std::collections::HashMap;
use std::sync::Arc;

use log::trace;
use skulpin::skia_safe::gpu::SurfaceOrigin;
use skulpin::skia_safe::{
    colors, dash_path_effect, Budgeted, Canvas, ImageInfo, Paint, Rect, Surface, Point, BlendMode, image_filters::blur
};
use skulpin::skia_safe::canvas::{
    SrcRectConstraint, SaveLayerRec
};
use skulpin::CoordinateSystemHelper;

mod caching_shaper;
pub mod cursor_renderer;
pub mod font_options;
pub mod animation_utils;

pub use caching_shaper::CachingShaper;
pub use font_options::*;
use animation_utils::*;

use crate::editor::{Style, WindowRenderInfo, EDITOR, DrawCommand};
use crate::redraw_scheduler::REDRAW_SCHEDULER;
use crate::settings::*;
use cursor_renderer::CursorRenderer;


// ----------------------------------------------------------------------------

#[derive(Clone)]
pub struct RendererSettings {
    animation_length: f32,
}

pub fn initialize_settings() {
    SETTINGS.set(&RendererSettings {
        animation_length: 0.15,
    });

    register_nvim_setting!("window_animation_length", RendererSettings::animation_length);
}

// ----------------------------------------------------------------------------

pub struct RenderedWindow {
    surface: Surface,
    start_position: Point,
    current_position: Point,
    previous_destination: Point,
    t: f32
}

impl RenderedWindow {
    pub fn new(surface: Surface, position: Point) -> RenderedWindow {
        RenderedWindow {
            surface,
            start_position: position.clone(),
            current_position: position.clone(),
            previous_destination: position.clone(),
            t: 2.0 // 2.0 is out of the 0.0 to 1.0 range and stops animation
        }
    }

    pub fn update(
        &mut self,
        settings: &RendererSettings,
        destination: Point,
        dt: f32
    ) -> bool {
        if destination != self.previous_destination {
            self.t = 0.0;
            self.start_position = self.current_position;
            self.previous_destination = destination;
        }

        if (self.t - 1.0).abs() < std::f32::EPSILON {
            return false;
        }

        if (self.t - 1.0).abs() < std::f32::EPSILON {
            // We are at destination, move t out of 0-1 range to stop the animation
            self.t = 2.0;
        } else {
            self.t = (self.t + dt / settings.animation_length).min(1.0);
        }

        self.current_position = ease_point(
            ease_out_expo,
            self.start_position,
            destination,
            self.t,
        );

        true
    }
}

pub struct Renderer {
    rendered_windows: HashMap<u64, RenderedWindow>,
    paint: Paint,
    shaper: CachingShaper,

    pub font_width: f32,
    pub font_height: f32,
    pub window_regions: Vec<(u64, Rect)>,
    cursor_renderer: CursorRenderer,
}

impl Renderer {
    pub fn new() -> Renderer {
        let rendered_windows = HashMap::new();

        let mut paint = Paint::new(colors::WHITE, None);
        paint.set_anti_alias(false);

        let mut shaper = CachingShaper::new();

        let (font_width, font_height) = shaper.font_base_dimensions();
        let window_regions = Vec::new();
        let cursor_renderer = CursorRenderer::new();

        Renderer {
            rendered_windows,
            paint,
            shaper,
            font_width,
            font_height,
            window_regions,
            cursor_renderer,
        }
    }

    fn update_font(&mut self, guifont_setting: &str) -> bool {
        let updated = self.shaper.update_font(guifont_setting);
        if updated {
            let (font_width, font_height) = self.shaper.font_base_dimensions();
            self.font_width = font_width;
            self.font_height = font_height.ceil();
        }
        updated
    }

    fn compute_text_region(&self, grid_pos: (u64, u64), cell_width: u64) -> Rect {
        let (grid_x, grid_y) = grid_pos;
        let x = grid_x as f32 * self.font_width;
        let y = grid_y as f32 * self.font_height;
        let width = cell_width as f32 * self.font_width as f32;
        let height = self.font_height as f32;
        Rect::new(x, y, x + width, y + height)
    }

    fn draw_background(
        &mut self,
        canvas: &mut Canvas,
        grid_pos: (u64, u64),
        cell_width: u64,
        style: &Option<Arc<Style>>,
        default_style: &Arc<Style>,
        floating: bool
    ) {
        self.paint.set_blend_mode(BlendMode::Src);

        let region = self.compute_text_region(grid_pos, cell_width);
        let style = style.as_ref().unwrap_or(default_style);

        let mut color = style.background(&default_style.colors);

        if floating {
            color.a = 0.8;
        }

        self.paint.set_color(color.to_color());
        canvas.draw_rect(region, &self.paint);
    }

    fn draw_foreground(
        &mut self,
        canvas: &mut Canvas,
        text: &str,
        grid_pos: (u64, u64),
        cell_width: u64,
        style: &Option<Arc<Style>>,
        default_style: &Arc<Style>,
    ) {
        let (grid_x, grid_y) = grid_pos;
        let x = grid_x as f32 * self.font_width;
        let y = grid_y as f32 * self.font_height;
        let width = cell_width as f32 * self.font_width;

        let style = style.as_ref().unwrap_or(default_style);

        canvas.save();

        let region = self.compute_text_region(grid_pos, cell_width);

        canvas.clip_rect(region, None, Some(false));

        if style.underline || style.undercurl {
            let line_position = self.shaper.underline_position();
            let stroke_width = self.shaper.options.size / 10.0;
            self.paint
                .set_color(style.special(&default_style.colors).to_color());
            self.paint.set_stroke_width(stroke_width);

            if style.undercurl {
                self.paint.set_path_effect(dash_path_effect::new(
                    &[stroke_width * 2.0, stroke_width * 2.0],
                    0.0,
                ));
            } else {
                self.paint.set_path_effect(None);
            }

            canvas.draw_line(
                (x, y - line_position + self.font_height),
                (x + width, y - line_position + self.font_height),
                &self.paint,
            );
        }

        self.paint
            .set_color(style.foreground(&default_style.colors).to_color());
        let text = text.trim_end();
        if !text.is_empty() {
            for blob in self
                .shaper
                .shape_cached(text, style.bold, style.italic)
                .iter()
            {
                canvas.draw_text_blob(blob, (x, y), &self.paint);
            }
        }

        if style.strikethrough {
            let line_position = region.center_y();
            self.paint
                .set_color(style.special(&default_style.colors).to_color());
            canvas.draw_line((x, line_position), (x + width, line_position), &self.paint);
        }

        canvas.restore();
    }

    pub fn build_window_surface(
        &self,
        gpu_canvas: &mut Canvas,
        default_style: &Arc<Style>,
        dimensions: (i32, i32),
    ) -> Surface {
        let mut context = gpu_canvas.gpu_context().unwrap();
        let budgeted = Budgeted::Yes;
        let parent_image_info = gpu_canvas.image_info();
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
        canvas.clear(default_style.colors.background.clone().unwrap().to_color());
        surface
    }

    pub fn draw_window(
        &mut self,
        settings: &RendererSettings,
        root_canvas: &mut Canvas,
        window_render_info: WindowRenderInfo,
        default_style: &Arc<Style>,
        dt: f32
    ) -> (u64, Rect) {
        let (grid_left, grid_top) = window_render_info.grid_position;
        let window_destination = Point::new(grid_left as f32 * self.font_width, grid_top as f32 * self.font_height);

        let image_width = (window_render_info.width as f32 * self.font_width) as i32;
        let image_height = (window_render_info.height as f32 * self.font_height) as i32;

        let mut rendered_window = self.rendered_windows
            .remove(&window_render_info.grid_id)
            .unwrap_or_else(|| {
                let surface = self.build_window_surface(root_canvas, &default_style, (image_width, image_height));
                RenderedWindow::new(surface, window_destination)
            });

        for command in window_render_info.draw_commands.into_iter() {
            match command {
                DrawCommand::Cell {
                    text, cell_width, grid_position, style
                } => {
                    let mut canvas = rendered_window.surface.canvas();

                    self.draw_background(
                        &mut canvas,
                        grid_position,
                        cell_width,
                        &style,
                        &default_style,
                        window_render_info.floating
                    );
                    self.draw_foreground(
                        &mut canvas,
                        &text,
                        grid_position,
                        cell_width,
                        &style,
                        &default_style,
                    );
                },
                DrawCommand::Scroll {
                    top, bot, left, right, rows, cols
                } => {
                    let scrolled_region = Rect::new(
                        left as f32 * self.font_width,
                        top as f32 * self.font_height,
                        right as f32 * self.font_width,
                        bot as f32 * self.font_height);

                    let snapshot = rendered_window.surface.image_snapshot();
                    let canvas = rendered_window.surface.canvas();

                    canvas.save();
                    canvas.clip_rect(scrolled_region, None, Some(false));

                    let mut translated_region = scrolled_region.clone();
                    translated_region.offset((-cols as f32 * self.font_width, -rows as f32 * self.font_height));

                    canvas.draw_image_rect(snapshot, Some((&scrolled_region, SrcRectConstraint::Fast)), translated_region, &self.paint);

                    canvas.restore();
                },
                DrawCommand::Resize => {
                    let mut old_surface = rendered_window.surface;
                    rendered_window.surface = self.build_window_surface(root_canvas, &default_style, (image_width, image_height));
                    old_surface.draw(rendered_window.surface.canvas(), (0.0, 0.0), None);
                },
                DrawCommand::Clear => {
                    rendered_window.surface = self.build_window_surface(root_canvas, &default_style, (image_width, image_height));
                }
            }
        }

        if rendered_window.update(settings, window_destination, dt) {
            REDRAW_SCHEDULER.queue_next_frame();
        }

        root_canvas.save();
        // let region = Rect::new(5.0, 5.0, 500.0, 500.0);
        // root_canvas.clip_rect(&region, None, Some(false));

        // let blur = blur((5.0, 5.0), None, None, None).unwrap();
        // let save_layer_rec = SaveLayerRec::default()
        //     .backdrop(&blur)
        //     .bounds(&region);

        // root_canvas.save_layer(&save_layer_rec);

        rendered_window.surface.draw(
            root_canvas.as_mut(),
            (rendered_window.current_position.x, rendered_window.current_position.y), 
            None);

        // root_canvas.restore();
        root_canvas.restore();

        let window_position = rendered_window.current_position.clone();
        self.rendered_windows.insert(window_render_info.grid_id, rendered_window);

        (window_render_info.grid_id, Rect::from_point_and_size(window_position, (image_width as f32, image_height as f32)))
    }

    pub fn draw(
        &mut self,
        gpu_canvas: &mut Canvas,
        coordinate_system_helper: &CoordinateSystemHelper,
        dt: f32,
    ) -> bool {
        trace!("Rendering");

        let settings = SETTINGS.get::<RendererSettings>();

        let (render_info, default_style, cursor, guifont_setting) = {
            let mut editor = EDITOR.lock();
            (
                editor.build_render_info(),
                editor.default_style.clone(),
                editor.cursor.clone(),
                editor.guifont.clone(),
            )
        };

        gpu_canvas.clear(default_style.colors.background.clone().unwrap().to_color());

        let font_changed = guifont_setting
            .map(|guifont| self.update_font(&guifont))
            .unwrap_or(false);

        for closed_window_id in render_info.closed_window_ids.iter() {
            self.rendered_windows.remove(&closed_window_id);
        }

        coordinate_system_helper.use_logical_coordinates(gpu_canvas);

        self.window_regions = render_info.windows
            .into_iter()
            .map(|window_render_info| self.draw_window(&settings, gpu_canvas, window_render_info, &default_style, dt))
            .collect();

        self.cursor_renderer.draw(
            cursor,
            &default_style.colors,
            (self.font_width, self.font_height),
            &mut self.shaper,
            gpu_canvas,
            dt,
        );

        font_changed
    }
}
