mod blink;
mod cursor_vfx;

use skulpin::skia_safe::{Canvas, Paint, Path, Point};

use crate::editor::{Colors, Cursor, CursorShape};
use crate::redraw_scheduler::REDRAW_SCHEDULER;
use crate::renderer::animation_utils::*;
use crate::renderer::CachingShaper;
use crate::settings::*;

use blink::*;

const DEFAULT_CELL_PERCENTAGE: f32 = 1.0 / 8.0;

const STANDARD_CORNERS: &[(f32, f32); 4] = &[(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)];

// ----------------------------------------------------------------------------

#[derive(Clone)]
pub struct CursorSettings {
    antialiasing: bool,
    animation_length: f32,
    animate_in_insert_mode: bool,
    springloaded: bool,
    squash_and_stretch: f32,
    spring_constant: f32,
    damping: f32,
    trail_size: f32,
    vfx_mode: cursor_vfx::VfxMode,
    vfx_opacity: f32,
    vfx_particle_lifetime: f32,
    vfx_particle_density: f32,
    vfx_particle_speed: f32,
    vfx_particle_phase: f32,
    vfx_particle_curl: f32,
}

pub fn initialize_settings() {
    SETTINGS.set(&CursorSettings {
        antialiasing: true,
        animation_length: 0.13,
        animate_in_insert_mode: true,
        /// Whether to use the alternative spring-damper cursor movement
        springloaded: true,
        /// How much the cursor should deform in the direction of movement
        squash_and_stretch: 0.015,
        /// Higher spring constants yield faster cursor movement
        spring_constant: 1024.0,
        /// 1 is critically damped, <1 is underdamped, >1 is overdamped
        /// Less damping means snappier cursor movement that will overshoot its target
        damping: 0.666,
        trail_size: 0.7,
        vfx_mode: cursor_vfx::VfxMode::Disabled,
        vfx_opacity: 200.0,
        vfx_particle_lifetime: 1.2,
        vfx_particle_density: 7.0,
        vfx_particle_speed: 10.0,
        vfx_particle_phase: 1.5,
        vfx_particle_curl: 1.0,
    });

    register_nvim_setting!("cursor_antialiasing", CursorSettings::antialiasing);
    register_nvim_setting!(
        "cursor_animate_in_insert_mode",
        CursorSettings::animate_in_insert_mode
    );
    register_nvim_setting!("cursor_animation_length", CursorSettings::animation_length);
    register_nvim_setting!("cursor_trail_size", CursorSettings::trail_size);
    register_nvim_setting!("cursor_vfx_mode", CursorSettings::vfx_mode);
    register_nvim_setting!("cursor_vfx_opacity", CursorSettings::vfx_opacity);
    register_nvim_setting!(
        "cursor_vfx_particle_lifetime",
        CursorSettings::vfx_particle_lifetime
    );
    register_nvim_setting!(
        "cursor_vfx_particle_density",
        CursorSettings::vfx_particle_density
    );
    register_nvim_setting!(
        "cursor_vfx_particle_speed",
        CursorSettings::vfx_particle_speed
    );
    register_nvim_setting!(
        "cursor_vfx_particle_phase",
        CursorSettings::vfx_particle_phase
    );
    register_nvim_setting!(
        "cursor_vfx_particle_curl",
        CursorSettings::vfx_particle_curl
    );
    register_nvim_setting!("cursor_springloaded", CursorSettings::springloaded);
    register_nvim_setting!(
        "cursor_squash_and_stretch",
        CursorSettings::squash_and_stretch
    );
    register_nvim_setting!("cursor_spring_constant", CursorSettings::spring_constant);
    register_nvim_setting!("cursor_damping", CursorSettings::damping);
}

// ----------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Corner {
    start_position: Point,
    current_position: Point,
    relative_position: Point,
    previous_destination: Point,
    t: f32,
}

impl Corner {
    pub fn new() -> Corner {
        Corner {
            start_position: Point::new(0.0, 0.0),
            current_position: Point::new(0.0, 0.0),
            relative_position: Point::new(0.0, 0.0),
            previous_destination: Point::new(-1000.0, -1000.0),
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
            self.t = (self.t + corner_dt / settings.animation_length).min(1.0)
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

    current_center: Point,
    velocity: Point,

    blink_status: BlinkStatus,
    previous_cursor_shape: Option<CursorShape>,
    cursor_vfx: Option<Box<dyn cursor_vfx::CursorVfx>>,
    previous_vfx_mode: cursor_vfx::VfxMode,
}

impl CursorRenderer {
    pub fn new() -> CursorRenderer {
        let mut renderer = CursorRenderer {
            corners: vec![Corner::new(); 4],
            cursor: Cursor::new(),

            current_center: Point::new(0f32, 0f32),
            velocity: Point::new(0f32, 0f32),

            blink_status: BlinkStatus::new(),
            previous_cursor_shape: None,
            //cursor_vfx: Box::new(PointHighlight::new(Point{x:0.0, y:0.0}, HighlightMode::Ripple)),
            cursor_vfx: None,
            previous_vfx_mode: cursor_vfx::VfxMode::Disabled,
        };
        renderer.set_cursor_shape(&CursorShape::Block, DEFAULT_CELL_PERCENTAGE);
        renderer
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

    pub fn draw(
        &mut self,
        default_colors: &Colors,
        font_size: (f32, f32),
        shaper: &mut CachingShaper,
        canvas: &mut Canvas,
        dt: f32,
    ) {
        let (font_width, font_height) = font_size;
        let render = self.blink_status.update_status(&self.cursor);
        let settings = SETTINGS.get::<CursorSettings>();

        if settings.vfx_mode != self.previous_vfx_mode {
            self.cursor_vfx = cursor_vfx::new_cursor_vfx(&settings.vfx_mode);
            self.previous_vfx_mode = settings.vfx_mode.clone();
        }

        let mut paint = Paint::new(skulpin::skia_safe::colors::WHITE, None);
        paint.set_anti_alias(settings.antialiasing);

        let (grid_x, grid_y) = self.cursor.position;
        let character = self.cursor.character.clone();

        let font_width = match (self.cursor.double_width, &self.cursor.shape) {
            (true, CursorShape::Block) => font_width * 2.0,
            _ => font_width,
        };

        let font_dimensions: Point = (font_width, font_height).into();

        let in_insert_mode = false;
        // {
        // let editor = EDITOR.lock();
        // matches!(editor.current_mode, EditorMode::Insert)
        // };

        let destination: Point = (grid_x as f32 * font_width, grid_y as f32 * font_height).into();
        let center_destination = destination + font_dimensions * 0.5;
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
            if settings.springloaded {
                // Note: implicit euler integration becomes unstable for
                // spring constant values greater than about 1024.

                let toward = center_destination - self.current_center;
                let toward: Point = toward.into();
                let b = settings.spring_constant.sqrt() * 2.0 * settings.damping;
                let spring_force = toward * settings.spring_constant;
                let damping_force = self.velocity * -b;
                let acceleration = spring_force + damping_force;
                self.velocity += acceleration * dt;
                self.current_center += self.velocity * dt;

                let toward_unit = {
                    let mut toward_unit = toward;
                    toward_unit.normalize();
                    toward_unit
                };
                let scale = 1.0 + toward.length() * settings.squash_and_stretch;
                let theta = toward_unit.y.atan2(toward_unit.x);
                let unrot = Matrix2::rot(-theta);
                let scale = Matrix2::scale(scale, 1.0 / scale);
                let rot = Matrix2::rot(theta);
                let squash_transform = rot * scale * unrot;

                for corner in self.corners.iter_mut() {
                    let mut rel = corner.relative_position;
                    rel.x *= font_dimensions.x;
                    rel.y *= font_dimensions.y;
                    let rel = squash_transform.mul_vec(rel);
                    corner.current_position = self.current_center + rel;
                }

                let near_target = toward.length() < std::f32::EPSILON;
                let critically_slow = self.velocity.length() < std::f32::EPSILON;
                animating = !(near_target && critically_slow);
            } else {
                for corner in self.corners.iter_mut() {
                    let corner_animating = corner.update(
                        &settings,
                        font_dimensions,
                        center_destination,
                        dt,
                        !settings.animate_in_insert_mode && in_insert_mode,
                    );

                    animating |= corner_animating;
                }
            }

            let vfx_animating = if let Some(vfx) = self.cursor_vfx.as_mut() {
                vfx.update(&settings, center_destination, (font_width, font_height), dt)
            } else {
                false
            };

            animating |= vfx_animating;
        }

        if animating {
            REDRAW_SCHEDULER.queue_next_frame();
        }

        if self.cursor.enabled && render {
            // Draw Background
            paint.set_color(self.cursor.background(&default_colors).to_color());

            // The cursor is made up of four points, so I create a path with each of the four
            // corners.
            let mut path = Path::new();

            path.move_to(self.corners[0].current_position);
            path.line_to(self.corners[1].current_position);
            path.line_to(self.corners[2].current_position);
            path.line_to(self.corners[3].current_position);
            path.close();

            canvas.draw_path(&path, &paint);

            // Draw foreground
            paint.set_color(self.cursor.foreground(&default_colors).to_color());

            canvas.save();
            canvas.clip_path(&path, None, Some(false));

            let blobs = &shaper.shape_cached(&character, false, false);

            for blob in blobs.iter() {
                canvas.draw_text_blob(&blob, destination, &paint);
            }

            canvas.restore();

            if let Some(vfx) = self.cursor_vfx.as_ref() {
                vfx.render(
                    &settings,
                    canvas,
                    &self.cursor,
                    &default_colors,
                    (font_width, font_height),
                );
            }
        }
    }
}

struct Matrix2 {
    pub m: [[f32; 2]; 2],
}

impl Matrix2 {
    pub fn rot(theta: f32) -> Self {
        let cos = theta.cos();
        let sin = theta.sin();
        Self {
            m: [[cos, -sin], [sin, cos]],
        }
    }

    pub fn scale(x: f32, y: f32) -> Self {
        Self {
            m: [[x, 0.0], [0.0, y]],
        }
    }

    pub fn mul_vec(&self, v: Point) -> Point {
        Point {
            x: v.x * self[0][0] + v.y * self[0][1],
            y: v.x * self[1][0] + v.y * self[1][1],
        }
    }
}

impl std::ops::Mul for Matrix2 {
    type Output = Matrix2;

    fn mul(self, rhs: Self) -> Self {
        Self {
            m: [
                [
                    self[0][0] * rhs[0][0] + self[0][1] * rhs[1][0],
                    self[0][0] * rhs[0][1] + self[0][1] * rhs[1][1],
                ],
                [
                    self[1][0] * rhs[0][0] + self[1][1] * rhs[1][0],
                    self[1][0] * rhs[0][1] + self[1][1] * rhs[1][1],
                ],
            ],
        }
    }
}

impl std::ops::Index<usize> for Matrix2 {
    type Output = [f32; 2];

    fn index(&self, i: usize) -> &Self::Output {
        &self.m[i]
    }
}
