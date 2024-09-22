use std::collections::VecDeque;

use palette::Srgba;
use vide::parley::style::{FontStack, StyleProperty};
use vide::{Layer, Path, PathCommand, Scene, Shaper};

use crate::{
    profiling::tracy_zone,
    renderer::{animation_utils::lerp, RendererSettings},
    units::{PixelPos, PixelRect},
    SETTINGS,
};

const FRAMETIMES_COUNT: usize = 48;

// TODO: Remove
#[allow(unused)]
pub struct Profiler {
    font_size: f32,
    rect: PixelRect<f32>,
    frametimes: VecDeque<f32>,
}

impl Profiler {
    pub fn new(font_size: f32) -> Self {
        Self {
            font_size,
            rect: PixelRect::from_origin_and_size((32.0, 32.0).into(), (200.0, 120.0).into()),
            frametimes: VecDeque::with_capacity(FRAMETIMES_COUNT),
        }
    }

    pub fn draw(&mut self, dt: f32, scene: &mut Scene, shaper: &mut Shaper) {
        tracy_zone!("profiler_draw");
        if !SETTINGS.get::<RendererSettings>().profiler {
            return;
        }

        let background_color = Srgba::new(30, 30, 30, 200);

        let mut layer = Layer::new()
            .with_clip(self.rect.as_untyped().to_rect().try_cast().unwrap())
            .with_clear(background_color.into());

        // // Draw FPS
        let text = format!("{:.0}FPS", 1.0 / dt.max(f32::EPSILON));
        let text_pos = self.rect.min;
        self.draw_text(&text, &text_pos, &mut layer, scene, shaper);

        self.frametimes.push_back(dt * 1000.0); // to msecs
        while self.frametimes.len() > FRAMETIMES_COUNT {
            self.frametimes.pop_front();
        }
        self.draw_graph(&mut layer, scene, shaper);

        scene.add_layer(layer);
    }

    fn draw_text(
        &self,
        text: &str,
        text_pos: &PixelPos<f32>,
        layer: &mut Layer,
        scene: &mut Scene,
        shaper: &mut Shaper,
    ) {
        let text_color = Srgba::new(0, 255, 0, 255);
        let layout = shaper.layout_with(text, |builder| {
            builder.push_default(&StyleProperty::FontStack(FontStack::Source("monospace")));
            builder.push_default(&StyleProperty::Brush(text_color.into()));
            builder.push_default(&StyleProperty::FontSize(self.font_size));
        });
        layer.add_text_layout(&mut scene.resources, &layout, *text_pos.as_untyped());
    }

    fn draw_graph(&self, layer: &mut Layer, scene: &mut Scene, shaper: &mut Shaper) {
        let color = Srgba::new(0, 100, 200, 255);

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

        let mut rect = self.rect.to_rect();
        rect.size.height -= 8.0;

        let graph_height = 80.0;

        //paint.set_anti_alias(true);

        let start_point = (rect.max().x + 10.0, rect.min().y + rect.height() / 2.0);
        let mut path = Path::new_line(1.0, color.into(), start_point.into());
        for (i, dt) in self.frametimes.iter().enumerate() {
            let x = lerp(
                rect.min().x,
                rect.max().x,
                i as f32 / self.frametimes.len() as f32,
            );
            let y = self.rect.max.y - graph_height * (*dt - min_g) / diff;
            let point = (x, y);
            path.commands.push(PathCommand::LineTo { to: point.into() });
        }

        layer.add_path(path);

        // Show min, max, avg (average).
        self.draw_text(
            &format!("min: {min_ft:.1}ms"),
            &(rect.min().x, rect.max().y - self.font_size).into(),
            layer,
            scene,
            shaper,
        );
        self.draw_text(
            &format!("avg: {avg:.1}ms"),
            &(
                rect.min().x,
                rect.max().y - graph_height * 0.5 - self.font_size,
            )
                .into(),
            layer,
            scene,
            shaper,
        );
        self.draw_text(
            &format!("max: {max_ft:.1}ms"),
            &(rect.min().x, rect.max().y - graph_height - self.font_size).into(),
            layer,
            scene,
            shaper,
        );
    }
}
