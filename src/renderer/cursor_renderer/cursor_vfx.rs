use log::error;
use skulpin::skia_safe::{paint::Style, BlendMode, Canvas, Color, Paint, Point, Rect};

use super::animation_utils::*;
use crate::editor::{Colors, Cursor};
use crate::settings::*;

pub trait CursorVfx {
    fn update(&mut self, current_cursor_destination: Point, dt: f32) -> bool;
    fn restart(&mut self, position: Point);
    fn render(&self, canvas: &mut Canvas, cursor: &Cursor, colors: &Colors, font_size: (f32, f32));
}

#[derive(Clone, PartialEq)]
pub enum HighlightMode {
    SonicBoom,
    Ripple,
    Wireframe,
}

#[derive(Clone, PartialEq)]
pub enum TrailMode {
    Railgun,
    Torpedo,
    PixieDust,
}

#[derive(Clone, PartialEq)]
pub enum VfxMode {
    Highlight(HighlightMode),
    Trail(TrailMode),
    Disabled,
}

impl FromValue for VfxMode {
    fn from_value(&mut self, value: Value) {
        if value.is_str() {
            *self = match value.as_str().unwrap() {
                "sonicboom" => VfxMode::Highlight(HighlightMode::SonicBoom),
                "ripple" => VfxMode::Highlight(HighlightMode::Ripple),
                "wireframe" => VfxMode::Highlight(HighlightMode::Wireframe),
                "railgun" => VfxMode::Trail(TrailMode::Railgun),
                "torpedo" => VfxMode::Trail(TrailMode::Torpedo),
                "pixiedust" => VfxMode::Trail(TrailMode::PixieDust),
                "" => VfxMode::Disabled,
                value => {
                    error!("Expected a VfxMode name, but received {:?}", value);
                    return;
                }
            };
        } else {
            error!("Expected a VfxMode string, but received {:?}", value);
        }
    }
}

impl From<VfxMode> for Value {
    fn from(mode: VfxMode) -> Self {
        match mode {
            VfxMode::Highlight(HighlightMode::SonicBoom) => Value::from("sonicboom"),
            VfxMode::Highlight(HighlightMode::Ripple) => Value::from("ripple"),
            VfxMode::Highlight(HighlightMode::Wireframe) => Value::from("wireframe"),
            VfxMode::Trail(TrailMode::Railgun) => Value::from("railgun"),
            VfxMode::Trail(TrailMode::Torpedo) => Value::from("torpedo"),
            VfxMode::Trail(TrailMode::PixieDust) => Value::from("pixiedust"),
            VfxMode::Disabled => Value::from(""),
        }
    }
}

pub fn new_cursor_vfx(mode: &VfxMode) -> Option<Box<dyn CursorVfx>> {
    match mode {
        VfxMode::Highlight(mode) => Some(Box::new(PointHighlight::new(mode))),
        VfxMode::Trail(mode) => Some(Box::new(ParticleTrail::new(mode))),
        VfxMode::Disabled => None,
    }
}

pub struct PointHighlight {
    t: f32,
    center_position: Point,
    mode: HighlightMode,
}

impl PointHighlight {
    pub fn new(mode: &HighlightMode) -> PointHighlight {
        PointHighlight {
            t: 0.0,
            center_position: Point::new(0.0, 0.0),
            mode: mode.clone(),
        }
    }
}

impl CursorVfx for PointHighlight {
    fn update(&mut self, _current_cursor_destination: Point, dt: f32) -> bool {
        self.t = (self.t + dt * 5.0).min(1.0); // TODO - speed config
        self.t < 1.0
    }

    fn restart(&mut self, position: Point) {
        self.t = 0.0;
        self.center_position = position;
    }

    fn render(&self, canvas: &mut Canvas, cursor: &Cursor, colors: &Colors, font_size: (f32, f32)) {
        if self.t == 1.0 {
            return;
        }
        let mut paint = Paint::new(skulpin::skia_safe::colors::WHITE, None);
        paint.set_blend_mode(BlendMode::SrcOver);

        let base_color: Color = cursor.background(&colors).to_color();
        let alpha = ease(ease_in_quad, 255.0, 0.0, self.t) as u8;
        let color = Color::from_argb(alpha, base_color.r(), base_color.g(), base_color.b());
        paint.set_color(color);

        let size = 3.0 * font_size.1;
        let radius = self.t * size;
        let hr = radius * 0.5;
        let rect = Rect::from_xywh(
            self.center_position.x - hr,
            self.center_position.y - hr,
            radius,
            radius,
        );

        match self.mode {
            HighlightMode::SonicBoom => {
                canvas.draw_oval(&rect, &paint);
            }
            HighlightMode::Ripple => {
                paint.set_style(Style::Stroke);
                paint.set_stroke_width(font_size.1 * 0.2);
                canvas.draw_oval(&rect, &paint);
            }
            HighlightMode::Wireframe => {
                paint.set_style(Style::Stroke);
                paint.set_stroke_width(font_size.1 * 0.2);
                canvas.draw_rect(&rect, &paint);
            }
        }
    }
}

#[derive(Clone)]
struct ParticleData {
    pos: Point,
    speed: Point,
    lifetime: f32,
}

pub struct ParticleTrail {
    particles: Vec<ParticleData>,
    previous_cursor_dest: Point,
    trail_mode: TrailMode,
}

impl ParticleTrail {
    pub fn new(trail_mode: &TrailMode) -> ParticleTrail {
        ParticleTrail {
            particles: vec![],
            previous_cursor_dest: Point::new(0.0, 0.0),
            trail_mode: trail_mode.clone(),
        }
    }

    fn add_particle(&mut self, pos: Point, speed: Point, lifetime: f32) {
        self.particles.push(ParticleData {
            pos,
            speed,
            lifetime,
        });
    }

    // Note this method doesn't keep particles in order
    fn remove_particle(&mut self, idx: usize) {
        self.particles[idx] = self.particles[self.particles.len() - 1].clone();
        self.particles.pop();
    }
}

const PARTICLE_DENSITY: f32 = 0.008;
const PARTICLE_LIFETIME: f32 = 1.2;

impl CursorVfx for ParticleTrail {
    fn update(&mut self, current_cursor_dest: Point, dt: f32) -> bool {
        // Update lifetimes and remove dead particles
        let mut i = 0;
        while i < self.particles.len() {
            let particle: &mut ParticleData = &mut self.particles[i];
            particle.lifetime -= dt;
            if particle.lifetime <= 0.0 {
                self.remove_particle(i);
            } else {
                i += 1;
            }
        }

        // Update particle positions
        for i in 0..self.particles.len() {
            let particle = &mut self.particles[i];
            particle.pos += particle.speed * dt;
        }

        // Spawn new particles
        if current_cursor_dest != self.previous_cursor_dest {
            let travel = current_cursor_dest - self.previous_cursor_dest;
            let travel_distance = travel.length();
            // Increase amount of particles when cursor travels further
            // TODO -- particle count should not depend on font size
            let particle_count = (travel_distance.powf(1.5) * PARTICLE_DENSITY) as usize;

            let prev_p = self.previous_cursor_dest;

            let mut rng = RngState::new();

            for i in 0..particle_count {
                let t = i as f32 / (particle_count as f32 - 1.0);

                let phase = t * 60.0;

                let speed = match self.trail_mode {
                    TrailMode::Railgun => Point::new(phase.sin(), phase.cos()) * 20.0,
                    TrailMode::Torpedo => rng.rand_dir() * 10.0,
                    TrailMode::PixieDust => {
                        let base_dir = rng.rand_dir();
                        let dir = Point::new(base_dir.x * 0.5, base_dir.y.abs());
                        dir * 30.0
                    }
                };

                let pos = prev_p + travel * (t + 0.3 * rng.next_f32() / particle_count as f32);

                self.add_particle(pos, speed, t * PARTICLE_LIFETIME);
            }

            self.previous_cursor_dest = current_cursor_dest;
        }

        // Keep animating as long as there are particles alive
        !self.particles.is_empty()
    }

    fn restart(&mut self, _position: Point) {}

    fn render(&self, canvas: &mut Canvas, cursor: &Cursor, colors: &Colors, font_size: (f32, f32)) {
        let mut paint = Paint::new(skulpin::skia_safe::colors::WHITE, None);
        match self.trail_mode {
            TrailMode::Torpedo | TrailMode::Railgun => {
                paint.set_style(Style::Stroke);
            }
            _ => {}
        }

        paint.set_stroke_width(font_size.1 * 0.2);
        let base_color: Color = cursor.background(&colors).to_color();

        paint.set_blend_mode(BlendMode::SrcOver);

        self.particles.iter().for_each(|particle| {
            let l = particle.lifetime / PARTICLE_LIFETIME;
            let alpha = (l * 128.0) as u8;
            let color = Color::from_argb(alpha, base_color.r(), base_color.g(), base_color.b());
            paint.set_color(color);

            let radius = match self.trail_mode {
                TrailMode::Torpedo | TrailMode::Railgun => font_size.0 * 0.5 * l,
                TrailMode::PixieDust => font_size.0 * 0.2,
            };
            let hr = radius * 0.5;

            let rect = Rect::from_xywh(particle.pos.x - hr, particle.pos.y - hr, radius, radius);

            match self.trail_mode {
                TrailMode::Torpedo | TrailMode::Railgun => {
                    canvas.draw_oval(&rect, &paint);
                }
                TrailMode::PixieDust => {
                    canvas.draw_rect(&rect, &paint);
                }
            }
        });
    }
}

// Random number generator based on http://www.pcg-random.org/
struct RngState {
    state: u64,
    inc: u64,
}

impl RngState {
    fn new() -> RngState {
        RngState {
            state: 0x853C49E6748FEA9Bu64,
            inc: (0xDA3E39CB94B95BDBu64 << 1) | 1,
        }
    }
    fn next(&mut self) -> u64 {
        use std::num::Wrapping;

        let old_state = self.state;
        self.state =
            (Wrapping(old_state) * Wrapping(6_364_136_223_846_793_005u64) + Wrapping(self.inc)).0;
        let xorshifted = (old_state >> 18) ^ old_state >> 27;
        let rot = (old_state >> 59) as i64;
        (xorshifted >> rot as u64) | (xorshifted << ((-rot) & 31))
    }

    fn next_f32(&mut self) -> f32 {
        let v = self.next();

        let float_bits = (v as f64).to_bits();
        let exponent = (float_bits >> 53) & ((1 << 10) - 1);
        // Set exponent for 0-1 range
        let new_exponent = exponent.max(32) - 32;

        let new_bits = (new_exponent << 53) | (float_bits & 0x801F_FFFF_FFFF_FFFFu64);

        f64::from_bits(new_bits) as f32
    }

    fn rand_dir(&mut self) -> Point {
        let x = self.next_f32();
        let y = self.next_f32();

        Point::new(x * 2.0 - 1.0, y * 2.0 - 1.0)
    }
}
