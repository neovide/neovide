use skulpin::skia_safe::{BlendMode, Canvas, Color, Paint, Point, Rect, paint::Style};

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
        return self.t < 1.0;
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
        let alpha = ((1.0 - self.t) * 255.0) as u8;
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
            },
            HighlightMode::Ripple => {
                paint.set_style(Style::Stroke);
                paint.set_stroke_width(font_size.1 * 0.2);
                canvas.draw_oval(&rect, &paint);
            },
            HighlightMode::Wireframe => {
                paint.set_style(Style::Stroke);
                paint.set_stroke_width(font_size.1 * 0.2);
                canvas.draw_rect(&rect, &paint);
            },
        }

    }
}
