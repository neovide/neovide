mod blink;
mod cursor_vfx;

use std::collections::HashMap;

use skia_safe::{op, Canvas, Paint, Path, Point};
use winit::event::{Event, WindowEvent};

use crate::{
    bridge::EditorMode,
    editor::{Cursor, CursorShape},
    redraw_scheduler::REDRAW_SCHEDULER,
    renderer::animation_utils::*,
    renderer::{GridRenderer, RenderedWindow},
    settings::{ParseFromValue, SETTINGS},
};

use blink::*;

const DEFAULT_CELL_PERCENTAGE: f32 = 1.0 / 8.0;

const STANDARD_CORNERS: &[(f32, f32); 4] = &[(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)];

#[derive(SettingGroup)]
#[setting_prefix = "cursor"]
#[derive(Clone)]
pub struct CursorSettings {
    antialiasing: bool,
    animation_length: f32,
    distance_length_adjust: bool,
    animate_in_insert_mode: bool,
    animate_command_line: bool,
    trail_size: f32,
    unfocused_outline_width: f32,

    vfx_mode: cursor_vfx::VfxMode,
    vfx_opacity: f32,
    vfx_particle_lifetime: f32,
    vfx_particle_density: f32,
    vfx_particle_speed: f32,
    vfx_particle_phase: f32,
    vfx_particle_curl: f32,
}

impl Default for CursorSettings {
    fn default() -> Self {
        CursorSettings {
            antialiasing: true,
            animation_length: 0.06,
            distance_length_adjust: true,
            animate_in_insert_mode: true,
            animate_command_line: true,
            trail_size: 0.7,
            unfocused_outline_width: 1.0 / 8.0,
            vfx_mode: cursor_vfx::VfxMode::Disabled,
            vfx_opacity: 200.0,
            vfx_particle_lifetime: 1.2,
            vfx_particle_density: 7.0,
            vfx_particle_speed: 10.0,
            vfx_particle_phase: 1.5,
            vfx_particle_curl: 1.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Corner {
    start_position: Point,
    current_position: Point,
    relative_position: Point,
    previous_destination: Point,
    length_multiplier: f32,
    t: f32,
}

impl Corner {
    pub fn new() -> Corner {
        Corner {
            start_position: Point::new(0.0, 0.0),
            current_position: Point::new(0.0, 0.0),
            relative_position: Point::new(0.0, 0.0),
            previous_destination: Point::new(-1000.0, -1000.0),
            length_multiplier: 1.0,
            t: 0.0,
        }
    }

    pub fn update(
        &mut self,
        settings: &CursorSettings,
        font_dimensions: Point,
        destination: Point,
        dt: f32,
        immediate_movement: bool,
    ) -> bool {
        if destination != self.previous_destination {
            self.t = 0.0;
            self.start_position = self.current_position;
            self.previous_destination = destination;
            self.length_multiplier = if settings.distance_length_adjust {
                (destination - self.current_position)
                    .length()
                    .log10()
                    .max(0.0)
            } else {
                1.0
            }
        }

        // Check first if animation's over
        if (self.t - 1.0).abs() < std::f32::EPSILON {
            return false;
        }

        // Calculate window-space destination for corner
        let relative_scaled_position: Point = (
            self.relative_position.x * font_dimensions.x,
            self.relative_position.y * font_dimensions.y,
        )
            .into();

        let corner_destination = destination + relative_scaled_position;

        if immediate_movement {
            self.t = 1.0;
            self.current_position = corner_destination;
            return true;
        }

        // Calculate how much a corner will be lagging behind based on how much it's aligned
        // with the direction of motion. Corners in front will move faster than corners in the
        // back
        let travel_direction = {
            let mut d = destination - self.current_position;
            d.normalize();
            d
        };

        let corner_direction = {
            let mut d = self.relative_position;
            d.normalize();
            d
        };

        let direction_alignment = travel_direction.dot(corner_direction);

        if (self.t - 1.0).abs() < std::f32::EPSILON {
            // We are at destination, move t out of 0-1 range to stop the animation
            self.t = 2.0;
        } else {
            let corner_dt = dt
                * lerp(
                    1.0,
                    (1.0 - settings.trail_size).max(0.0).min(1.0),
                    -direction_alignment,
                );
            self.t =
                (self.t + corner_dt / (settings.animation_length * self.length_multiplier)).min(1.0)
        }

        self.current_position = ease_point(
            ease_out_expo,
            self.start_position,
            corner_destination,
            self.t,
        );

        true
    }
}

pub struct CursorRenderer {
    pub corners: Vec<Corner>,
    cursor: Cursor,
    destination: Point,
    blink_status: BlinkStatus,
    previous_cursor_shape: Option<CursorShape>,
    previous_editor_mode: EditorMode,
    cursor_vfx: Option<Box<dyn cursor_vfx::CursorVfx>>,
    previous_vfx_mode: cursor_vfx::VfxMode,
    window_has_focus: bool,
}

impl CursorRenderer {
    pub fn new() -> CursorRenderer {
        let mut renderer = CursorRenderer {
            corners: vec![Corner::new(); 4],
            cursor: Cursor::new(),
            destination: (0.0, 0.0).into(),
            blink_status: BlinkStatus::new(),
            previous_cursor_shape: None,
            previous_editor_mode: EditorMode::Normal,
            cursor_vfx: None,
            previous_vfx_mode: cursor_vfx::VfxMode::Disabled,
            window_has_focus: true,
        };
        renderer.set_cursor_shape(&CursorShape::Block, DEFAULT_CELL_PERCENTAGE);
        renderer
    }

    pub fn handle_event(&mut self, event: &Event<()>) {
        if let Event::WindowEvent {
            event: WindowEvent::Focused(is_focused),
            ..
        } = event
        {
            self.window_has_focus = *is_focused
        }
    }

    pub fn update_cursor(&mut self, new_cursor: Cursor) {
        self.cursor = new_cursor;
    }

    fn set_cursor_shape(&mut self, cursor_shape: &CursorShape, cell_percentage: f32) {
        self.corners = self
            .corners
            .clone()
            .into_iter()
            .enumerate()
            .map(|(i, corner)| {
                let (x, y) = STANDARD_CORNERS[i];

                Corner {
                    relative_position: match cursor_shape {
                        CursorShape::Block => (x, y).into(),
                        // Transform the x position so that the right side is translated over to
                        // the BAR_WIDTH position
                        CursorShape::Vertical => ((x + 0.5) * cell_percentage - 0.5, y).into(),
                        // Do the same as above, but flip the y coordinate and then flip the result
                        // so that the horizontal bar is at the bottom of the character space
                        // instead of the top.
                        CursorShape::Horizontal => {
                            (x, -((-y + 0.5) * cell_percentage - 0.5)).into()
                        }
                    },
                    t: 0.0,
                    start_position: corner.current_position,
                    ..corner
                }
            })
            .collect::<Vec<Corner>>();
    }

    pub fn update_cursor_destination(
        &mut self,
        (font_width, font_height): (u64, u64),
        windows: &HashMap<u64, RenderedWindow>,
    ) {
        let (cursor_grid_x, cursor_grid_y) = self.cursor.grid_position;

        if let Some(window) = windows.get(&self.cursor.parent_window_id) {
            let grid_x = cursor_grid_x as f32 + window.grid_current_position.x;
            let mut grid_y = cursor_grid_y as f32 + window.grid_current_position.y
                - (window.current_scroll - window.current_surface.vertical_position);

            // Prevent the cursor from targeting a position outside its current window. Since only
            // the vertical direction is effected by scrolling, we only have to clamp the vertical
            // grid position.
            grid_y = grid_y
                .max(window.grid_current_position.y)
                .min(window.grid_current_position.y + window.grid_size.height as f32 - 1.0);

            self.destination = (grid_x * font_width as f32, grid_y * font_height as f32).into();
        } else {
            self.destination = (
                (cursor_grid_x * font_width) as f32,
                (cursor_grid_y * font_height) as f32,
            )
                .into();
        }
    }

    pub fn draw(
        &mut self,
        grid_renderer: &mut GridRenderer,
        current_mode: &EditorMode,
        canvas: &mut Canvas,
        dt: f32,
    ) {
        let render = self.blink_status.update_status(&self.cursor);
        let settings = SETTINGS.get::<CursorSettings>();

        if settings.vfx_mode != self.previous_vfx_mode {
            self.cursor_vfx = cursor_vfx::new_cursor_vfx(&settings.vfx_mode);
            self.previous_vfx_mode = settings.vfx_mode.clone();
        }

        let mut paint = Paint::new(skia_safe::colors::WHITE, None);
        paint.set_anti_alias(settings.antialiasing);

        let character = self.cursor.grid_cell.0.clone();

        let mut cursor_width = grid_renderer.font_dimensions.width;
        if self.cursor.double_width && self.cursor.shape == CursorShape::Block {
            cursor_width *= 2;
        }

        let cursor_dimensions: Point = (
            cursor_width as f32,
            grid_renderer.font_dimensions.height as f32,
        )
            .into();

        let in_insert_mode = matches!(current_mode, EditorMode::Insert);

        let changed_to_from_cmdline = !matches!(self.previous_editor_mode, EditorMode::CmdLine)
            ^ matches!(current_mode, EditorMode::CmdLine);

        let center_destination = self.destination + cursor_dimensions * 0.5;
        let new_cursor = Some(self.cursor.shape.clone());

        if self.previous_cursor_shape != new_cursor {
            self.previous_cursor_shape = new_cursor.clone();
            self.set_cursor_shape(
                &new_cursor.unwrap(),
                self.cursor
                    .cell_percentage
                    .unwrap_or(DEFAULT_CELL_PERCENTAGE),
            );

            if let Some(vfx) = self.cursor_vfx.as_mut() {
                vfx.restart(center_destination);
            }
        }

        let mut animating = false;

        if !center_destination.is_zero() {
            for corner in self.corners.iter_mut() {
                let immediate_movement = !settings.animate_in_insert_mode && in_insert_mode
                    || !settings.animate_command_line && !changed_to_from_cmdline;

                let corner_animating = corner.update(
                    &settings,
                    cursor_dimensions,
                    center_destination,
                    dt,
                    immediate_movement,
                );

                animating |= corner_animating;
            }

            let vfx_animating = if let Some(vfx) = self.cursor_vfx.as_mut() {
                vfx.update(&settings, center_destination, cursor_dimensions, dt)
            } else {
                false
            };

            animating |= vfx_animating;
        }

        if animating {
            REDRAW_SCHEDULER.queue_next_frame();
        } else {
            self.previous_editor_mode = current_mode.clone();
        }
        if !(self.cursor.enabled && render) {
            return;
        }
        // Draw Background
        let background_color = self
            .cursor
            .background(&grid_renderer.default_style.colors)
            .to_color()
            .with_a(self.cursor.alpha());
        paint.set_color(background_color);

        let path = if self.window_has_focus || self.cursor.shape != CursorShape::Block {
            self.draw_rectangle(canvas, &paint)
        } else {
            let outline_width = settings.unfocused_outline_width * grid_renderer.em_size;
            self.draw_rectangular_outline(canvas, &paint, outline_width)
        };

        // Draw foreground
        let foreground_color = self
            .cursor
            .foreground(&grid_renderer.default_style.colors)
            .to_color()
            .with_a(self.cursor.alpha());
        paint.set_color(foreground_color);

        canvas.save();
        canvas.clip_path(&path, None, Some(false));

        let y_adjustment = grid_renderer.shaper.y_adjustment();
        let style = &self.cursor.grid_cell.1;

        let bold = style.as_ref().map(|x| x.bold).unwrap_or(false);
        let italic = style.as_ref().map(|x| x.italic).unwrap_or(false);

        let blobs = &grid_renderer.shaper.shape_cached(character, bold, italic);

        for blob in blobs.iter() {
            canvas.draw_text_blob(
                blob,
                (self.destination.x, self.destination.y + y_adjustment as f32),
                &paint,
            );
        }

        canvas.restore();

        if let Some(vfx) = self.cursor_vfx.as_ref() {
            vfx.render(&settings, canvas, grid_renderer, &self.cursor);
        }
    }

    fn draw_rectangle(&self, canvas: &mut Canvas, paint: &Paint) -> Path {
        // The cursor is made up of four points, so I create a path with each of the four
        // corners.
        let mut path = Path::new();

        path.move_to(self.corners[0].current_position);
        path.line_to(self.corners[1].current_position);
        path.line_to(self.corners[2].current_position);
        path.line_to(self.corners[3].current_position);
        path.close();

        canvas.draw_path(&path, paint);
        path
    }

    fn draw_rectangular_outline(
        &self,
        canvas: &mut Canvas,
        paint: &Paint,
        outline_width: f32,
    ) -> Path {
        let mut rectangle = Path::new();
        rectangle.move_to(self.corners[0].current_position);
        rectangle.line_to(self.corners[1].current_position);
        rectangle.line_to(self.corners[2].current_position);
        rectangle.line_to(self.corners[3].current_position);
        rectangle.close();

        let offsets: [Point; 4] = [
            (outline_width, outline_width).into(),
            (-outline_width, outline_width).into(),
            (-outline_width, -outline_width).into(),
            (outline_width, -outline_width).into(),
        ];

        let mut subtract = Path::new();
        subtract.move_to(self.corners[0].current_position + offsets[0]);
        subtract.line_to(self.corners[1].current_position + offsets[1]);
        subtract.line_to(self.corners[2].current_position + offsets[2]);
        subtract.line_to(self.corners[3].current_position + offsets[3]);
        subtract.close();

        // We have two "rectangles"; create an outline path by subtracting the smaller rectangle
        // from the larger one. This can fail in which case we return a full "rectangle".
        let path = op(&rectangle, &subtract, skia_safe::PathOp::Difference).unwrap_or(rectangle);

        canvas.draw_path(&path, paint);
        path
    }
}
