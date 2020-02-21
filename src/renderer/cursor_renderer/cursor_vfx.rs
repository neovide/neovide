use skulpin::skia_safe::{paint::Style, BlendMode, Canvas, Color, Paint, Point, Rect};

use super::animation_utils::*;
use crate::editor::{Colors, Cursor};

pub trait CursorVFX {
    fn update(&mut self, current_cursor_destination: Point, dt: f32) -> bool;
    fn restart(&mut self, position: Point);
    fn render(&self, canvas: &mut Canvas, cursor: &Cursor, colors: &Colors, font_size: (f32, f32));
}

#[allow(dead_code)]
pub enum HighlightMode {
    SonicBoom,
    Ripple,
    Wireframe,
}

pub struct PointHighlight {
    t: f32,
    center_position: Point,
    mode: HighlightMode,
}

impl PointHighlight {
    pub fn new(center: Point, mode: HighlightMode) -> PointHighlight {
        PointHighlight {
            t: 0.0,
            center_position: center,
            mode,
        }
    }
}

impl CursorVFX for PointHighlight {
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
}

impl ParticleTrail {
    pub fn new() -> ParticleTrail {
        ParticleTrail {
            particles: vec![],
            previous_cursor_dest: Point::new(0.0, 0.0),
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

impl CursorVFX for ParticleTrail {
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
            for i in 0..particle_count {
                let t = i as f32 / (particle_count as f32 - 1.0);

                let phase = t * 60.0;
                let rand = Point::new(phase.sin(), phase.cos());

                let pos = prev_p + travel * (t + 0.3 * rand.x / particle_count as f32);

                self.add_particle(pos, rand * 20.0, t * PARTICLE_LIFETIME);
            }

            self.previous_cursor_dest = current_cursor_dest;
        }

        // Keep animating as long as there are particles alive
        !self.particles.is_empty()
    }

    fn restart(&mut self, _position: Point) {}

    fn render(&self, canvas: &mut Canvas, cursor: &Cursor, colors: &Colors, font_size: (f32, f32)) {
        let mut paint = Paint::new(skulpin::skia_safe::colors::WHITE, None);
        paint.set_style(Style::Stroke);
        paint.set_stroke_width(font_size.1 * 0.2);
        let base_color: Color = cursor.background(&colors).to_color();

        paint.set_blend_mode(BlendMode::SrcOver);

        self.particles.iter().for_each(|particle| {
            let l = particle.lifetime / PARTICLE_LIFETIME;
            let alpha = (l * 255.0) as u8;
            let color = Color::from_argb(alpha, base_color.r(), base_color.g(), base_color.b());
            paint.set_color(color);

            let radius = font_size.0 * 0.5 * l;
            let hr = radius * 0.5;

            let rect = Rect::from_xywh(particle.pos.x - hr, particle.pos.y - hr, radius, radius);
            canvas.draw_oval(&rect, &paint);
            //canvas.draw_rect(&rect, &paint);
        });
    }
}
