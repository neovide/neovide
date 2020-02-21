use skulpin::skia_safe::{BlendMode, Canvas, Color, Paint, Point, Rect};

use crate::editor::{Colors, Cursor};

pub trait CursorVFX {
    fn update(&mut self, current_cursor_destination: Point, dt: f32) -> bool;
    fn restart(&mut self, position: Point);
    fn render(&self, _paint: &mut Paint, canvas: &mut Canvas, cursor: &Cursor, colors: &Colors);
}

pub struct SonicBoom {
    pub t: f32,
    pub center_position: Point,
}

impl CursorVFX for SonicBoom {
    fn update(&mut self, _current_cursor_destination: Point, dt: f32) -> bool {
        self.t = (self.t + dt * 5.0).min(1.0); // TODO - speed config
        return self.t < 1.0;
    }

    fn restart(&mut self, position: Point) {
        self.t = 0.0;
        self.center_position = position;
    }

    fn render(&self, _paint: &mut Paint, canvas: &mut Canvas, cursor: &Cursor, colors: &Colors) {
        if self.t == 1.0 {
            return;
        }
        let mut paint = Paint::new(skulpin::skia_safe::colors::WHITE, None);
        paint.set_blend_mode(BlendMode::SrcOver);

        let base_color: Color = cursor.background(&colors).to_color();
        let alpha = ((1.0 - self.t) * 255.0) as u8;
        let color = Color::from_argb(alpha, base_color.r(), base_color.g(), base_color.b());
        paint.set_color(color);

        let size = 40.0; // TODO -- Compute from font size
        let radius = self.t * size;
        let hr = radius * 0.5;
        let rect = Rect::from_xywh(
            self.center_position.x - hr,
            self.center_position.y - hr,
            radius,
            radius,
        );

        canvas.draw_oval(&rect, &paint);
    }
}
