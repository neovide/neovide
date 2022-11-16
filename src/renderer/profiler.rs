use crate::renderer::animation_utils::lerp;
use crate::settings::SETTINGS;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

use crate::{
    profiling::tracy_zone,
    renderer::{fonts::font_loader::*, RendererSettings},
};
use skia_safe::{Canvas, Color, Paint, Point, Rect, Size};

const FRAMETIMES_COUNT: usize = 48;

pub struct Profiler {
    pub font: Arc<FontPair>,
    pub position: Point,
    pub size: Size,
    pub last_draw: Instant,
    pub frametimes: VecDeque<f32>,
}

impl Profiler {
    pub fn new(font_size: f32) -> Self {
        let font_key = FontKey::default();
        let mut font_loader = FontLoader::new(font_size);
        let font = font_loader.get_or_load(&font_key).unwrap();
        Self {
            font,
            position: Point::new(32.0, 32.0),
            size: Size::new(200.0, 120.0),
            last_draw: Instant::now(),
            frametimes: VecDeque::with_capacity(FRAMETIMES_COUNT),
        }
    }

    pub fn draw(&mut self, root_canvas: &mut Canvas, dt: f32) {
        tracy_zone!("profiler_draw");
        if !SETTINGS.get::<RendererSettings>().profiler {
            return;
        }

        root_canvas.save();
        let rect = self.get_rect();
        root_canvas.clip_rect(rect, None, Some(false));

        let mut paint = Paint::default();

        // Draw background
        let color = Color::from_argb(200, 30, 30, 30);
        paint.set_color(color);
        root_canvas.draw_paint(&paint);

        // Draw FPS
        let color = Color::from_argb(255, 0, 255, 0);
        paint.set_color(color);
        let mut text_position = self.position;
        text_position.y += self.font.skia_font.size();
        root_canvas.draw_str(
            format!("{:.0}FPS", 1.0 / dt.max(f32::EPSILON)),
            text_position,
            &self.font.skia_font,
            &paint,
        );

        self.frametimes.push_back(dt * 1000.0); // to msecs
        while self.frametimes.len() > FRAMETIMES_COUNT {
            self.frametimes.pop_front();
        }

        self.draw_graph(root_canvas);

        root_canvas.restore();
    }

    fn draw_graph(&self, root_canvas: &mut Canvas) {
        let mut paint = Paint::default();
        let color = Color::from_argb(255, 0, 100, 200);
        paint.set_color(color);

        // Get min and max and avg.
        let mut min_ft = f32::MAX;
        let mut max_ft = f32::MIN;
        let mut sum = 0.0;
        for dt in self.frametimes.iter() {
            min_ft = dt.min(min_ft);
            max_ft = dt.max(max_ft);
            sum += dt;
        }
        let avg = sum / self.frametimes.len() as f32;
        let min_g = min_ft * 0.8;
        let max_g = max_ft * 1.1;
        let diff = max_g - min_g;

        let mut rect = self.get_rect();
        rect.bottom -= 8.0; // bottom margin

        let graph_height = 80.0;

        paint.set_anti_alias(true);

        let mut prev_point = (rect.left - 10.0, self.position.y + rect.bottom);
        for (i, dt) in self.frametimes.iter().enumerate() {
            let x = lerp(
                rect.left,
                rect.right,
                i as f32 / self.frametimes.len() as f32,
            );
            let y = rect.bottom - graph_height * (*dt - min_g) / diff;
            let point = (x, y);
            root_canvas.draw_line(prev_point, point, &paint);
            prev_point = point;
        }

        let color = Color::from_argb(255, 0, 255, 0);
        paint.set_color(color);
        paint.set_anti_alias(false);

        // Show min, max, avg (average).
        root_canvas.draw_str(
            format!("min: {min_ft:.1}ms"),
            (rect.left, rect.bottom),
            &self.font.skia_font,
            &paint,
        );
        root_canvas.draw_str(
            format!("avg: {avg:.1}ms"),
            (rect.left, rect.bottom - graph_height * 0.5),
            &self.font.skia_font,
            &paint,
        );
        root_canvas.draw_str(
            format!("max: {max_ft:.1}ms"),
            (rect.left, rect.bottom - graph_height),
            &self.font.skia_font,
            &paint,
        );
    }

    fn get_rect(&self) -> Rect {
        Rect::new(
            self.position.x,
            self.position.y,
            self.position.x + self.size.width,
            self.position.y + self.size.height,
        )
    }
}
