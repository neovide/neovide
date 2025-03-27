use log::error;
use nvim_rs::Value;
use skia_safe::{BlendMode, Canvas, Color, Paint, Rect, paint::Style};

use crate::{
    editor::Cursor,
    renderer::cursor_renderer::CursorSettings,
    renderer::{animation_utils::*, grid_renderer::GridRenderer},
    settings::*,
    units::{GridSize, PixelPos, PixelSize, PixelVec},
};

pub trait CursorVfx {
    fn update(
        &mut self,
        settings: &CursorSettings,
        current_cursor_destination: PixelPos<f32>,
        cursor_dimensions: PixelSize<f32>,
        immediate_movement: bool,
        dt: f32,
    ) -> bool;
    fn restart(&mut self, position: PixelPos<f32>);
    fn cursor_jumped(&mut self, position: PixelPos<f32>);
    fn render(
        &self,
        settings: &CursorSettings,
        canvas: &Canvas,
        grid_renderer: &mut GridRenderer,
        cursor: &Cursor,
    );
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HighlightMode {
    SonicBoom,
    Ripple,
    Wireframe,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TrailMode {
    Railgun,
    Torpedo,
    PixieDust,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VfxMode {
    Highlight(HighlightMode),
    Trail(TrailMode),
    Disabled,
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct VfxModeList(Vec<VfxMode>);

impl ParseFromValue for VfxMode {
    fn parse_from_value(&mut self, value: Value) {
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

impl ParseFromValue for VfxModeList {
    fn parse_from_value(&mut self, value: Value) {
        if value.is_str() {
            let mut vfx_mode = VfxMode::Disabled;
            vfx_mode.parse_from_value(value);
            self.0.push(vfx_mode);
        } else if value.is_array() {
            for item in value.as_array().unwrap() {
                if item.is_str() {
                    let mut vfx_mode = VfxMode::Disabled;
                    vfx_mode.parse_from_value(item.clone());
                    self.0.push(vfx_mode);
                } else {
                    error!(
                        "Expected a VfxMode string in the array, but received {:?}",
                        item
                    );
                }
            }
        } else {
            error!(
                "Expected an array of VfxMode strings, but received {:?}",
                value
            );
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

impl From<VfxModeList> for Value {
    fn from(modes: VfxModeList) -> Self {
        let mut values = Vec::new();

        for mode in modes.0 {
            values.push(Value::from(mode));
        }

        Value::from(values)
    }
}

pub fn new_cursor_vfxs(modes: &VfxModeList) -> Vec<Box<dyn CursorVfx>> {
    modes
        .0
        .iter()
        .filter_map(|mode| match mode {
            VfxMode::Highlight(mode) => {
                Some(Box::new(PointHighlight::new(mode)) as Box<dyn CursorVfx>)
            }
            VfxMode::Trail(mode) => Some(Box::new(ParticleTrail::new(mode)) as Box<dyn CursorVfx>),
            VfxMode::Disabled => None,
        })
        .collect()
}

pub struct PointHighlight {
    t: f32,
    center_position: PixelPos<f32>,
    mode: HighlightMode,
}

impl PointHighlight {
    pub fn new(mode: &HighlightMode) -> PointHighlight {
        PointHighlight {
            t: 0.0,
            center_position: PixelPos::new(0.0, 0.0),
            mode: mode.clone(),
        }
    }
}

impl CursorVfx for PointHighlight {
    fn update(
        &mut self,
        settings: &CursorSettings,
        current_cursor_destination: PixelPos<f32>,
        _cursor_dimensions: PixelSize<f32>,
        _immediate_movement: bool,
        dt: f32,
    ) -> bool {
        self.center_position = current_cursor_destination;
        if settings.vfx_particle_highlight_lifetime > 0.0 {
            self.t = (self.t + dt * (1.0 / settings.vfx_particle_highlight_lifetime)).min(1.0);
        } else if settings.vfx_particle_lifetime > 0.0 {
            self.t = (self.t + dt * (1.0 / settings.vfx_particle_lifetime)).min(1.0);
        } else {
            self.t = 1.0
        }
        self.t < 1.0
    }

    fn restart(&mut self, position: PixelPos<f32>) {
        self.t = 0.0;
        self.center_position = position;
    }

    fn cursor_jumped(&mut self, position: PixelPos<f32>) {
        self.restart(position);
    }

    fn render(
        &self,
        settings: &CursorSettings,
        canvas: &Canvas,
        grid_renderer: &mut GridRenderer,
        cursor: &Cursor,
    ) {
        if (self.t - 1.0).abs() < f32::EPSILON {
            return;
        }

        let mut paint = Paint::new(skia_safe::colors::WHITE, None);
        paint.set_blend_mode(BlendMode::SrcOver);

        let colors = &grid_renderer.default_style.colors;
        let base_color: Color = cursor.background(colors).to_color();
        let alpha = ease(ease_in_quad, settings.vfx_opacity, 0.0, self.t) as u8;
        let color = Color::from_argb(alpha, base_color.r(), base_color.g(), base_color.b());

        paint.set_color(color);

        let cursor_height = grid_renderer.grid_scale.height();
        let size = 3.0 * cursor_height;
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
                canvas.draw_oval(rect, &paint);
            }
            HighlightMode::Ripple => {
                paint.set_style(Style::Stroke);
                paint.set_stroke_width(cursor_height * 0.2);
                canvas.draw_oval(rect, &paint);
            }
            HighlightMode::Wireframe => {
                paint.set_style(Style::Stroke);
                paint.set_stroke_width(cursor_height * 0.2);
                canvas.draw_rect(rect, &paint);
            }
        }
    }
}

#[derive(Clone)]
struct ParticleData {
    pos: PixelPos<f32>,
    speed: PixelVec<f32>,
    rotation_speed: f32,
    lifetime: f32,
}

pub struct ParticleTrail {
    particles: Vec<ParticleData>,
    previous_cursor_dest: PixelPos<f32>,
    trail_mode: TrailMode,
    rng: RngState,
    count_reminder: f32,
}

impl ParticleTrail {
    pub fn new(trail_mode: &TrailMode) -> ParticleTrail {
        ParticleTrail {
            particles: vec![],
            previous_cursor_dest: PixelPos::new(0.0, 0.0),
            trail_mode: trail_mode.clone(),
            rng: RngState::new(),
            count_reminder: 0.0,
        }
    }

    fn add_particle(
        &mut self,
        pos: PixelPos<f32>,
        speed: PixelVec<f32>,
        rotation_speed: f32,
        lifetime: f32,
    ) {
        self.particles.push(ParticleData {
            pos,
            speed,
            rotation_speed,
            lifetime,
        });
    }

    // Note this method doesn't keep particles in order
    fn remove_particle(&mut self, idx: usize) {
        self.particles[idx] = self.particles[self.particles.len() - 1].clone();
        self.particles.pop();
    }
}

impl CursorVfx for ParticleTrail {
    fn update(
        &mut self,
        settings: &CursorSettings,
        current_cursor_dest: PixelPos<f32>,
        cursor_dimensions: PixelSize<f32>,
        immediate_movement: bool,
        dt: f32,
    ) -> bool {
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
            particle.speed = rotate_vec(particle.speed, dt * particle.rotation_speed);
        }

        // Spawn new particles
        if current_cursor_dest != self.previous_cursor_dest {
            if !immediate_movement {
                let travel = current_cursor_dest - self.previous_cursor_dest;
                let travel_distance = travel.length();

                // Increase amount of particles when cursor travels further
                let f_particle_count = ((travel_distance / cursor_dimensions.height)
                    * settings.vfx_particle_density
                    * 0.1)
                    + self.count_reminder;

                let particle_count = f_particle_count as usize;
                self.count_reminder = f_particle_count - particle_count as f32;

                let prev_p = self.previous_cursor_dest;

                for i in 0..particle_count {
                    let t = ((i + 1) as f32) / (particle_count as f32);

                    let speed = match self.trail_mode {
                        TrailMode::Railgun => {
                            let phase = t / std::f32::consts::PI
                                * settings.vfx_particle_phase
                                * (travel_distance / cursor_dimensions.height);
                            PixelVec::new(phase.sin(), phase.cos())
                                * 2.0
                                * settings.vfx_particle_speed
                        }
                        TrailMode::Torpedo => {
                            let travel_dir = travel.normalize();
                            let particle_dir = self.rng.rand_dir_normalized() - travel_dir * 1.5;
                            particle_dir.normalize() * settings.vfx_particle_speed
                        }
                        TrailMode::PixieDust => {
                            let base_dir = self.rng.rand_dir_normalized();
                            let dir = PixelVec::new(base_dir.x * 0.5, 0.4 + base_dir.y.abs());
                            dir * 3.0 * settings.vfx_particle_speed
                        }
                    };

                    // Distribute particles along the travel distance
                    let pos = match self.trail_mode {
                        TrailMode::Railgun => prev_p + travel * t,
                        TrailMode::PixieDust | TrailMode::Torpedo => {
                            prev_p
                                + travel * self.rng.next_f32()
                                + PixelVec::new(0.0, cursor_dimensions.height * 0.5)
                        }
                    };

                    let rotation_speed = match self.trail_mode {
                        TrailMode::Railgun => std::f32::consts::PI * settings.vfx_particle_curl,
                        TrailMode::PixieDust | TrailMode::Torpedo => {
                            (self.rng.next_f32() - 0.5)
                                * std::f32::consts::FRAC_PI_2
                                * settings.vfx_particle_curl
                        }
                    };

                    self.add_particle(
                        pos,
                        speed,
                        rotation_speed,
                        t * settings.vfx_particle_lifetime,
                    );
                }
            }

            self.previous_cursor_dest = current_cursor_dest;
        }

        // Keep animating as long as there are particles alive
        !self.particles.is_empty()
    }

    fn restart(&mut self, _position: PixelPos<f32>) {
        self.count_reminder = 0.0;
    }

    fn cursor_jumped(&mut self, _position: PixelPos<f32>) {}

    fn render(
        &self,
        settings: &CursorSettings,
        canvas: &Canvas,
        grid_renderer: &mut GridRenderer,
        cursor: &Cursor,
    ) {
        let mut paint = Paint::new(skia_safe::colors::WHITE, None);
        let font_dimensions = GridSize::new(1.0, 1.0) * grid_renderer.grid_scale;
        match self.trail_mode {
            TrailMode::Torpedo | TrailMode::Railgun => {
                paint.set_style(Style::Stroke);
                paint.set_stroke_width(font_dimensions.height * 0.2);
            }
            _ => {}
        }

        let colors = &grid_renderer.default_style.colors;
        let base_color: Color = cursor.background(colors).to_color();

        paint.set_blend_mode(BlendMode::SrcOver);

        self.particles.iter().for_each(|particle| {
            let lifetime = particle.lifetime / settings.vfx_particle_lifetime;
            let alpha = (lifetime * settings.vfx_opacity) as u8;
            let color = Color::from_argb(alpha, base_color.r(), base_color.g(), base_color.b());
            paint.set_color(color);

            let radius = match self.trail_mode {
                TrailMode::Torpedo | TrailMode::Railgun => font_dimensions.width * 0.5 * lifetime,
                TrailMode::PixieDust => font_dimensions.width * 0.2,
            };

            let hr = radius * 0.5;
            let rect = Rect::from_xywh(particle.pos.x - hr, particle.pos.y - hr, radius, radius);

            match self.trail_mode {
                TrailMode::Torpedo | TrailMode::Railgun => {
                    canvas.draw_oval(rect, &paint);
                }
                TrailMode::PixieDust => {
                    canvas.draw_rect(rect, &paint);
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
            state: 0x853C_49E6_748F_EA9Bu64,
            inc: (0xDA3E_39CB_94B9_5BDBu64 << 1) | 1,
        }
    }
    fn next(&mut self) -> u32 {
        let old_state = self.state;

        // Implementation copied from:
        // https://rust-random.github.io/rand/src/rand_pcg/pcg64.rs.html#103
        let new_state = old_state
            .wrapping_mul(6_364_136_223_846_793_005u64)
            .wrapping_add(self.inc);

        self.state = new_state;

        const ROTATE: u32 = 59; // 64 - 5
        const XSHIFT: u32 = 18; // (5 + 32) / 2
        const SPARE: u32 = 27; // 64 - 32 - 5

        let rot = (old_state >> ROTATE) as u32;
        let xsh = (((old_state >> XSHIFT) ^ old_state) >> SPARE) as u32;
        xsh.rotate_right(rot)
    }

    fn next_f32(&mut self) -> f32 {
        let v = self.next();

        // In C we'd do ldexp(v, -32) to bring a number in the range [0,2^32) down to [0,1) range.
        // But as we don't have ldexp in Rust, we're implementing the same idea (subtracting 32
        // from the floating point exponent) manually.

        // First, extract exponent bits
        let float_bits = (v as f64).to_bits();
        let exponent = (float_bits >> 52) & ((1 << 11) - 1);

        // Set exponent for [0-1) range
        let new_exponent = exponent.max(32) - 32;

        // Build the new f64 value from the old mantissa and sign, and the new exponent
        let new_bits = (new_exponent << 52) | (float_bits & 0x801F_FFFF_FFFF_FFFFu64);

        f64::from_bits(new_bits) as f32
    }

    // Produces a random vector with x and y in the [-1,1) range
    // Note: Vector is not normalized.
    fn rand_dir(&mut self) -> PixelVec<f32> {
        let x = self.next_f32();
        let y = self.next_f32();

        PixelVec::new(x * 2.0 - 1.0, y * 2.0 - 1.0)
    }

    fn rand_dir_normalized(&mut self) -> PixelVec<f32> {
        self.rand_dir().normalize()
    }
}

fn rotate_vec(v: PixelVec<f32>, rot: f32) -> PixelVec<f32> {
    let sin = rot.sin();
    let cos = rot.cos();

    PixelVec::new(v.x * cos - v.y * sin, v.x * sin + v.y * cos)
}
