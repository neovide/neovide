use std::collections::BTreeMap;
use std::f32::consts::PI;
use std::sync::LazyLock;

use super::settings::{BoxDrawingMode, BoxDrawingSettings, ThicknessMultipliers};
use glamour::{Box2, Size2, Vector2};
use skia_safe::{
    BlendMode, Canvas, ClipOp, Color, Paint, PaintStyle, Path, PathEffect, PathFillType, Point,
    Rect, Size,
};

use crate::renderer::fonts::font_options::points_to_pixels;
use crate::units::Pixel;
use crate::units::{to_skia_point, to_skia_rect, PixelRect};

pub struct Context<'a> {
    canvas: &'a Canvas,
    settings: &'a BoxDrawingSettings,
    bounding_box: PixelRect<f32>,
    color_fg: Color,
}

impl<'a> Context<'a> {
    pub fn new(
        canvas: &'a Canvas,
        settings: &'a BoxDrawingSettings,
        bounding_box: PixelRect<f32>,
        color_fg: Color,
    ) -> Self {
        Context {
            canvas,
            settings,
            bounding_box,
            color_fg,
        }
    }

    fn get_stroke_width_pixels(&self, t: Thickness) -> f32 {
        let base_stroke_size =
            self.bounding_box.size().width * self.settings.stroke_width_ratio.unwrap_or(0.15);
        points_to_pixels(t.scale_factor(self.settings.thickness_multipliers) * base_stroke_size)
            .round()
            .max(1.0)
    }

    fn fg_paint(&self) -> Paint {
        let mut fg = Paint::default();
        fg.set_style(PaintStyle::Fill);
        fg.set_color(self.color_fg);
        fg.set_blend_mode(BlendMode::Plus);
        fg.set_anti_alias(false);
        fg
    }

    fn draw_fg_line1(&self, o: Orientation, which_half: HalfSelector) {
        self.draw_line(
            o,
            which_half,
            self.get_stroke_width_pixels(Thickness::Level1),
            self.color_fg,
            None,
        );
    }

    fn draw_fg_line3(&self, o: Orientation, which_half: HalfSelector) {
        self.draw_line(
            o,
            which_half,
            self.get_stroke_width_pixels(Thickness::Level3),
            self.color_fg,
            None,
        );
    }

    fn get_dash_effect(&self, o: Orientation, num_gaps: u8) -> PathEffect {
        let Size2 {
            width: cell_width,
            height: cell_height,
        } = self.bounding_box.size();
        let total = f32::round(match o {
            Orientation::Horizontal => cell_width,
            Orientation::Vertical => cell_height,
        }) as i32;

        let gap_sz = 2;
        let all_gaps_use = (num_gaps as i32) * gap_sz;
        let num_dashes = num_gaps as i32 + 1;
        let dash_sz = (total - all_gaps_use) / num_dashes;
        PathEffect::dash(&[dash_sz as f32, gap_sz as f32], 0.)
            .expect("new path effect ptr to be not null")
    }

    fn draw_arrow(&self, side: Side) {
        let mut path = Path::default();
        let min = self.bounding_box.min;
        let max = self.bounding_box.max;
        let mid = self.bounding_box.center();
        path.set_fill_type(PathFillType::Winding);
        match side {
            Side::Left => {
                path.move_to((max.x, min.y));
                path.line_to((min.x, mid.y));
                path.line_to((max.x, max.y));
            }
            Side::Right => {
                path.move_to((min.x, min.y));
                path.line_to((max.x, mid.y));
                path.line_to((min.x, max.y));
            }
        }
        path.close();
        let mut fg = self.fg_paint();
        fg.set_style(PaintStyle::Fill);
        fg.set_anti_alias(true);
        self.canvas.draw_path(&path, &fg);
    }

    fn draw_quarter_triangle(&self, corner: Corner, height: Height) {
        let mut path = Path::default();
        let min = self.bounding_box.min;
        let max = self.bounding_box.max;
        let mid = self.bounding_box.center();
        path.set_fill_type(PathFillType::Winding);
        match corner {
            Corner::TopLeft => {
                path.move_to((min.x, min.y));
                path.line_to((max.x, min.y));
                path.line_to((
                    min.x,
                    match height {
                        Height::Tall => max.y,
                        Height::Short => mid.y,
                    },
                ));
            }
            Corner::TopRight => {
                path.move_to((max.x, min.y));
                path.line_to((
                    max.x,
                    match height {
                        Height::Tall => max.y,
                        Height::Short => mid.y,
                    },
                ));
                path.line_to((min.x, min.y));
            }
            Corner::BottomRight => {
                path.move_to((max.x, max.y));
                path.line_to((min.x, max.y));
                path.line_to((
                    max.x,
                    match height {
                        Height::Tall => min.y,
                        Height::Short => mid.y,
                    },
                ));
            }
            Corner::BottomLeft => {
                path.move_to((min.x, max.y));
                path.line_to((max.x, max.y));
                path.line_to((
                    min.x,
                    match height {
                        Height::Tall => min.y,
                        Height::Short => mid.y,
                    },
                ));
            }
        }
        path.close();
        let mut fg = self.fg_paint();
        fg.set_style(PaintStyle::Fill);
        fg.set_anti_alias(true);
        self.canvas.draw_path(&path, &fg);
    }

    fn draw_half_cross_line(&self, start_corner: Corner) {
        let mut path = Path::default();
        let min = self.bounding_box.min;
        let max = self.bounding_box.max;
        let mid = self.bounding_box.center();
        match start_corner {
            Corner::TopLeft => {
                path.move_to((min.x, min.y));
                path.line_to((max.x, mid.y));
            }
            Corner::TopRight => {
                path.move_to((max.x, min.y));
                path.line_to((min.x, mid.y));
            }
            Corner::BottomRight => {
                path.move_to((max.x, max.y));
                path.line_to((min.x, mid.y));
            }
            Corner::BottomLeft => {
                path.move_to((min.x, max.y));
                path.line_to((max.x, mid.y));
            }
        }
        let mut fg = self.fg_paint();
        fg.set_style(PaintStyle::Stroke);
        fg.set_stroke_width(self.get_stroke_width_pixels(Thickness::Level2));
        fg.set_anti_alias(true);
        self.canvas.draw_path(&path, &fg);
    }

    fn draw_d(&self, side: Side, fill: PaintStyle, close_path: bool) {
        let mut path = Path::default();
        let mut oval = to_skia_rect(&self.bounding_box);
        let stroke_width = self.get_stroke_width_pixels(Thickness::Level2);
        if fill == PaintStyle::Stroke {
            oval.inset((stroke_width * 0.5, stroke_width * 0.5));
        }
        match side {
            Side::Left => {
                let start_angle = 270.0;
                let sweep_angle = 180.0;
                oval.left -= oval.width();
                path.arc_to(oval, start_angle, sweep_angle, false);
            }
            Side::Right => {
                let start_angle = 90.0;
                let sweep_angle = 180.0;
                oval.right += oval.width();
                path.arc_to(oval, start_angle, sweep_angle, false);
            }
        }
        if close_path {
            path.close();
        }
        let mut fg = self.fg_paint();
        fg.set_stroke_width(stroke_width);
        fg.set_style(fill);
        fg.set_anti_alias(true);
        self.canvas.draw_path(&path, &fg);
    }

    fn draw_cross_line(&self, side: Side) {
        let min = self.bounding_box.min;
        let max = self.bounding_box.max;
        let mut fg = self.fg_paint();
        fg.set_stroke_width(self.get_stroke_width_pixels(Thickness::Level2));
        fg.set_style(PaintStyle::Stroke);
        fg.set_anti_alias(true);
        match side {
            Side::Left => {
                self.canvas.draw_line((min.x, min.y), (max.x, max.y), &fg);
            }
            Side::Right => {
                self.canvas.draw_line((max.x, min.y), (min.x, max.y), &fg);
            }
        }
    }

    fn draw_progress(&self, section: Section, fill: PaintStyle) {
        let bounds = to_skia_rect(&self.bounding_box);
        let t: f32 = self.get_stroke_width_pixels(Thickness::Level1);
        let clip_rect = match section {
            Section::Left => bounds.with_inset((0., t)).with_offset((t, 0.)),
            Section::Middle => bounds.with_inset((0., t)),
            Section::Right => bounds.with_inset((0., t)).with_offset((-t, 0.)),
        };
        let mut fg = self.fg_paint();
        fg.set_style(PaintStyle::Fill);
        self.canvas.save();
        {
            self.canvas
                .clip_rect(clip_rect, ClipOp::Difference, Some(false));
            self.canvas.draw_rect(bounds, &fg);
        }
        self.canvas.restore();
        if fill == PaintStyle::Fill {
            let gap_factor: f32 = 3.0;
            let gap = gap_factor * t;
            let inner_rect = clip_rect.with_inset((0., gap)).with_offset(match section {
                Section::Left => (gap, 0.),
                Section::Middle => (0., 0.),
                Section::Right => (-gap, 0.),
            });
            self.canvas.draw_rect(inner_rect, &fg);
        }
    }

    fn draw_double_line(&self, o: Orientation, which_half: HalfSelector) {
        let stroke_width = self.get_stroke_width_pixels(Thickness::Level2);
        let gap_width = self.get_stroke_width_pixels(Thickness::Level1);
        let fat_stroke_width = stroke_width + gap_width + stroke_width;
        self.draw_line(o, which_half, fat_stroke_width, self.color_fg, None);
    }

    // (min.x, min.y)                      (max.x, min.y)
    //      o------------------------------o
    //      |                              |
    //      |                              |
    //      |              o               |
    //      |         (mid.x, mid.y)       |
    //      |                              |
    //      o------------------------------o
    // (min.x, max.y)                      (max.x, max.y)
    fn draw_line(
        &self,
        o: Orientation,
        which_half: HalfSelector,
        stroke_width: f32,
        color: Color,
        effect: impl Into<Option<PathEffect>>,
    ) {
        let min = self.bounding_box.min;
        let max = self.bounding_box.max;
        let mid = self.bounding_box.center();
        let (p1, p2) = match (o, which_half) {
            (Orientation::Horizontal, HalfSelector::First) => ((min.x, mid.y), (mid.x, mid.y)),
            (Orientation::Horizontal, HalfSelector::Last) => ((mid.x, mid.y), (max.x, mid.y)),
            (Orientation::Horizontal, HalfSelector::Both) => ((min.x, mid.y), (max.x, mid.y)),
            (Orientation::Vertical, HalfSelector::First) => ((mid.x, min.y), (mid.x, mid.y)),
            (Orientation::Vertical, HalfSelector::Last) => ((mid.x, mid.y), (mid.x, max.y)),
            (Orientation::Vertical, HalfSelector::Both) => ((mid.x, min.y), (mid.x, max.y)),
        };
        let mut paint = self.fg_paint();
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(stroke_width);
        paint.set_color(color);
        if let Some(effect) = effect.into() {
            paint.set_path_effect(effect);
            let mut path = Path::default();
            path.move_to(p1);
            path.line_to(p2);
            self.canvas.draw_path(&path, &paint);
        } else {
            self.canvas.draw_line(p1, p2, &paint);
        }
    }

    fn draw_eighth(&self, o: Orientation, which: impl std::ops::RangeBounds<u8>) {
        let min = self.bounding_box.min;
        let Size2 { width, height } = self.bounding_box.size();
        let (start, num_steps) = {
            let start_idx = match which.start_bound() {
                std::ops::Bound::Included(&s) => s,
                std::ops::Bound::Excluded(&s) => s.saturating_add(1).min(7),
                std::ops::Bound::Unbounded => 0,
            };
            let end_idx = match which.end_bound() {
                std::ops::Bound::Included(&s) => s.saturating_add(1).min(8),
                std::ops::Bound::Excluded(&s) => s,
                std::ops::Bound::Unbounded => 8,
            };

            (start_idx as f32, end_idx.saturating_sub(start_idx) as f32)
        };
        let rect = match o {
            Orientation::Horizontal => {
                let step = height / 8.0;
                let y1 = min.y + start * step;
                Rect::from_point_and_size((min.x, y1), Size::new(width, num_steps * step))
            }
            Orientation::Vertical => {
                let step = width / 8.0;
                let x1 = min.x + start * step;
                Rect::from_point_and_size((x1, min.y), Size::new(num_steps * step, height))
            }
        };
        let mut paint = self.fg_paint();
        paint.set_style(PaintStyle::Fill);
        self.canvas.draw_rect(rect, &paint);
    }

    // Test 1:
    // ░
    // ░░░░░░░░░░
    // ░░░░░░░░░░
    // ░░░░░░░░░░
    // Test 2:
    // ▒▒▒▒▒▒▒▒▒▒
    // ▒▒▒▒▒▒▒▒▒▒
    // Test 3:
    // ▓▓▓▓▓▓▓▓▓▓
    // ▓▓▓▓▓▓▓▓▓▓
    // Test 4:
    // 🮌
    // 🮌
    // Test 5:
    // 🮍
    // 🮍
    // Test 6:
    // 🮎🮎🮎🮎🮎🮎🮎🮎🮎🮎
    // Test 7:
    // 🮏🮏🮏🮏🮏🮏🮏🮏🮏🮏
    // Test 8:
    // 🮐🮐🮐🮐🮐🮐🮐🮐🮐🮐
    // 🮐🮐🮐🮐🮐🮐🮐🮐🮐🮐
    // Test 9:
    // 🮑🮑🮑🮑🮑🮑🮑🮑🮑🮑
    // 🮑🮑🮑🮑🮑🮑🮑🮑🮑🮑
    // Test 10:
    // 🮒🮒🮒🮒🮒🮒🮒🮒🮒🮒
    // 🮒🮒🮒🮒🮒🮒🮒🮒🮒🮒
    // Test 11:
    // 🮓🮓🮓🮓🮓🮓🮓🮓🮓🮓
    // 🮓🮓🮓🮓🮓🮓🮓🮓🮓🮓
    // Test 12:
    // 🮔🮔🮔🮔🮔🮔🮔🮔🮔🮔
    // 🮔🮔🮔🮔🮔🮔🮔🮔🮔🮔
    fn draw_shade(
        &self,
        o: Orientation,
        which_half: HalfSelector,
        shade: Shade,
        mirror: MirrorMode,
        color_mode: ColorMode,
    ) {
        self.canvas.save();
        self.canvas.clip_rect(
            {
                let mut rect = to_skia_rect(&self.bounding_box);
                match which_half {
                    HalfSelector::First => match o {
                        Orientation::Horizontal => rect.right = rect.center_x(),
                        Orientation::Vertical => rect.bottom = rect.center_y(),
                    },
                    HalfSelector::Last => match o {
                        Orientation::Horizontal => rect.left = rect.center_x(),
                        Orientation::Vertical => rect.top = rect.center_y(),
                    },
                    HalfSelector::Both => {}
                }
                rect
            },
            ClipOp::Intersect,
            Some(false),
        );

        const NUM_STRIPES: i32 = 4;
        let tile_sz = self.bounding_box.size();
        let stripe_gap = tile_sz.height / NUM_STRIPES as f32;
        let mut rotation_degrees = f32::atan(stripe_gap / tile_sz.width) * 180.0 / PI;
        let stripe_height = match shade {
            Shade::Light => 1.0,
            Shade::Medium => 2.0,
            Shade::Dark => 3.0,
        };
        let mut fg = self.fg_paint();
        fg.set_style(PaintStyle::Fill);
        fg.set_anti_alias(true);
        match color_mode {
            ColorMode::Normal => (),
            ColorMode::Inverted => {
                // TODO: fix this
                // self.canvas.draw_paint(&fg);
                // fg.set_color(self.color_bg);
            }
        }

        {
            let stripe_sz = (3.0 * tile_sz.width.max(tile_sz.height), stripe_height);
            match mirror {
                MirrorMode::Normal => (),
                MirrorMode::Mirror => {
                    rotation_degrees = 180.0 - rotation_degrees;
                    self.canvas.translate((tile_sz.width, 0.0));
                }
            };
            let top_left = self.bounding_box.min;
            for i in -1..NUM_STRIPES + 1 {
                let (dx, dy) = (0., i as f32 * stripe_gap);
                let stripe_top_left = top_left.translate(Vector2::new(dx, dy));
                self.canvas.save();
                self.canvas
                    .rotate(rotation_degrees, Some(stripe_top_left.to_tuple().into()));
                self.canvas.draw_rect(
                    Rect::from_point_and_size(stripe_top_left.to_tuple(), stripe_sz),
                    &fg,
                );
                self.canvas.restore();
            }
        }
        self.canvas.restore();
    }

    fn triangle_path(&self, corner: Corner) -> Path {
        let mut path = Path::default();
        let bb = to_skia_rect(&self.bounding_box);
        let top_left = (bb.left, bb.top);
        let top_right = (bb.right, bb.top);
        let bottom_left = (bb.left, bb.bottom);
        let bottom_right = (bb.right, bb.bottom);
        match corner {
            Corner::TopLeft => {
                path.move_to(top_left);
                path.line_to(top_right);
                path.line_to(bottom_left);
            }
            Corner::TopRight => {
                path.move_to(top_right);
                path.line_to(top_left);
                path.line_to(bottom_right);
            }
            Corner::BottomRight => {
                path.move_to(bottom_right);
                path.line_to(top_right);
                path.line_to(bottom_left);
            }
            Corner::BottomLeft => {
                path.move_to(bottom_left);
                path.line_to(top_left);
                path.line_to(bottom_right);
            }
        }
        path.close();
        path
    }

    // draws a "rectircle" using the formula
    //
    // (|x| / a) ^ (2a / r) + (|y| / b) ^ (2b / r) = 1
    // where 2a = width, 2b = height and r is radius
    //
    // See: https://math.stackexchange.com/questions/1649714/whats-the-equation-for-a-rectircle-perfect-rounded-corner-rectangle-without-s
    fn draw_rounded_corner(&self, corner: Corner) {
        let Size2 { width, height } = self.bounding_box.size();
        let stroke_width = self.get_stroke_width_pixels(Thickness::Level1);

        const STEP: f32 = 1.0 / 2.0;
        let a = width / 2.0;
        let b = height / 2.0;
        let exp = 1.0 / (2.0 * height / width);
        let rectircle = |px: f32| {
            //let term_x = (px.abs() / a).powf(2.0 * a / r);
            //let intermediate = 1.0 - term_x;
            //b * intermediate.powf(1.0 / (2.0 * b / r))

            // if r == width/2, then above simplifies to
            let term_x = px.abs() / a;
            b * (1.0 - term_x * term_x).powf(exp)
        };

        let draw_rectircle = || {
            let mut path = Path::default();
            let num_steps = f32::ceil(width / STEP) as i32;
            let points: Vec<_> = (1..num_steps)
                .map(|i| -a + i as f32 * STEP)
                .map(|px| (px, rectircle(px)))
                .collect();
            let start = Point::from((a, 0.0));
            let end = Point::from((-a, 0.0));
            path.move_to(start);
            points.iter().rev().for_each(|p| {
                path.line_to(*p);
            });
            path.line_to(end);

            path.move_to(start);
            points.iter().rev().for_each(|(x, y)| {
                path.line_to(Point::from((*x, -*y)));
            });
            path.line_to(end);

            let mut paint = self.fg_paint();
            paint.set_style(PaintStyle::Stroke);
            paint.set_stroke_width(stroke_width);
            paint.set_anti_alias(true);
            self.canvas.draw_path(&path, &paint);
        };

        let center = to_skia_point(self.bounding_box.center());
        self.canvas.save();
        {
            self.canvas.translate(center);
            self.canvas.translate(match corner {
                Corner::TopLeft => (width / 2.0, height / 2.0),
                Corner::TopRight => (-width / 2.0, height / 2.0),
                Corner::BottomLeft => (width / 2.0, -height / 2.0),
                Corner::BottomRight => (-width / 2.0, -height / 2.0),
            });
            draw_rectircle();
        }
        self.canvas.restore();
    }

    fn draw_t_joint(
        &self,
        north: impl Into<Option<Thickness>>,
        east: impl Into<Option<Thickness>>,
        south: impl Into<Option<Thickness>>,
        west: impl Into<Option<Thickness>>,
    ) {
        let fg = self.color_fg;
        for (t, o, h) in [
            (north.into(), Orientation::Vertical, HalfSelector::First),
            (east.into(), Orientation::Horizontal, HalfSelector::Last),
            (south.into(), Orientation::Vertical, HalfSelector::Last),
            (west.into(), Orientation::Horizontal, HalfSelector::First),
        ] {
            if let Some(t) = t {
                self.draw_line(o, h, self.get_stroke_width_pixels(t), fg, None);
            }
        }
    }

    fn draw_corner(&self, corner: Corner, horiz_t: Thickness, vert_t: Thickness) {
        let horiz_t = self.get_stroke_width_pixels(horiz_t);
        let vert_t = self.get_stroke_width_pixels(vert_t);
        let color = self.color_fg;
        let min = self.bounding_box.min;
        let max = self.bounding_box.max;
        let mid = self.bounding_box.center();
        let mut fg = self.fg_paint();
        fg.set_style(PaintStyle::Stroke);
        fg.set_color(color);

        let aligned_mid = match corner {
            Corner::TopLeft | Corner::TopRight => {
                mid.translate(Vector2::from((0.0, horiz_t * -0.5)))
            }
            Corner::BottomLeft | Corner::BottomRight => {
                mid.translate(Vector2::from((0.0, horiz_t * 0.5)))
            }
        };
        match corner {
            Corner::TopLeft => {
                fg.set_stroke_width(horiz_t);
                self.canvas.draw_line(mid.to_tuple(), (max.x, mid.y), &fg);
                fg.set_stroke_width(vert_t);
                self.canvas
                    .draw_line(aligned_mid.to_tuple(), (mid.x, max.y), &fg);
            }
            Corner::TopRight => {
                fg.set_stroke_width(horiz_t);
                self.canvas.draw_line((min.x, mid.y), mid.to_tuple(), &fg);
                fg.set_stroke_width(vert_t);
                self.canvas
                    .draw_line(aligned_mid.to_tuple(), (mid.x, max.y), &fg);
            }
            Corner::BottomRight => {
                fg.set_stroke_width(horiz_t);
                self.canvas.draw_line((min.x, mid.y), mid.to_tuple(), &fg);
                fg.set_stroke_width(vert_t);
                self.canvas
                    .draw_line((mid.x, min.y), aligned_mid.to_tuple(), &fg);
            }
            Corner::BottomLeft => {
                fg.set_stroke_width(horiz_t);
                self.canvas.draw_line(mid.to_tuple(), (max.x, mid.y), &fg);
                fg.set_stroke_width(vert_t);
                self.canvas
                    .draw_line((mid.x, min.y), aligned_mid.to_tuple(), &fg);
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Height {
    Tall,
    Short,
}

#[derive(Clone, Copy)]
enum Corner {
    TopLeft,
    TopRight,
    BottomRight,
    BottomLeft,
}

#[derive(Clone, Copy)]
enum Side {
    Left,
    Right,
}

#[derive(Clone, Copy)]
enum Section {
    Left,
    Middle,
    Right,
}

#[derive(Clone, Copy)]
enum Thickness {
    Level1,
    Level2,
    Level3,
}

impl Thickness {
    fn scale_factor(self, mult: Option<ThicknessMultipliers>) -> f32 {
        let ThicknessMultipliers(mult) = mult.unwrap_or_default();
        match self {
            Thickness::Level1 => mult[0],
            Thickness::Level2 => mult[1],
            Thickness::Level3 => mult[2],
        }
    }
}

#[derive(Clone, Copy)]
enum Orientation {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy)]
enum HalfSelector {
    First,
    Last,
    Both,
}

#[derive(Clone, Copy)]
enum Shade {
    Light,
    Medium,
    Dark,
}

#[derive(Clone, Copy)]
enum MirrorMode {
    Normal,
    Mirror,
}

#[derive(Clone, Copy)]
enum ColorMode {
    Normal,
    Inverted,
}

type BoxDrawFn = Box<dyn Fn(&Context) + Send + Sync>;

static BOX_CHARS: LazyLock<BTreeMap<char, BoxDrawFn>> = LazyLock::new(|| {
    use Orientation::*;
    let mut m: BTreeMap<char, BoxDrawFn> = BTreeMap::new();

    macro_rules! box_char {
        ($($chars:literal),* -> $func:expr) => {
            for ch in &[$($chars),*] {
                m.insert(*ch, Box::new($func));
            }
        };
    }

    box_char!['─' -> |ctx: &Context| {
        ctx.draw_fg_line1(Horizontal, HalfSelector::Both);
    }];
    box_char!['━' -> |ctx: &Context| {
        ctx.draw_fg_line3(Horizontal, HalfSelector::Both);
    }];
    box_char!['│' -> |ctx: &Context| {
        ctx.draw_fg_line1(Vertical, HalfSelector::Both);
    }];
    box_char!['┃' -> |ctx: &Context| {
        ctx.draw_fg_line3(Vertical, HalfSelector::Both);
    }];
    box_char!['╌' -> |ctx: &Context| {
        ctx.draw_line(
            Horizontal,
            HalfSelector::Both,
            ctx.get_stroke_width_pixels(Thickness::Level1),
            ctx.color_fg,
            None,
        );
    }];
    box_char!['╍' -> |ctx: &Context| {
        ctx.draw_line(
            Horizontal,
            HalfSelector::Both,
            ctx.get_stroke_width_pixels(Thickness::Level3),
            ctx.color_fg,
            None,
        );
    }];
    box_char!['┅' -> |ctx: &Context| {
        ctx.draw_line(
            Horizontal,
            HalfSelector::Both,
            ctx.get_stroke_width_pixels(Thickness::Level3),
            ctx.color_fg,
            ctx.get_dash_effect(Horizontal, 2),
        );
    }];
    box_char!['┈' -> |ctx: &Context| {
        ctx.draw_line(
            Horizontal,
            HalfSelector::Both,
            ctx.get_stroke_width_pixels(Thickness::Level1),
            ctx.color_fg,
            ctx.get_dash_effect(Horizontal, 3),
        );
    }];
    box_char!['┉' -> |ctx: &Context| {
        ctx.draw_line(
            Horizontal,
            HalfSelector::Both,
            ctx.get_stroke_width_pixels(Thickness::Level3),
            ctx.color_fg,
            ctx.get_dash_effect(Horizontal, 3),
        );
    }];

    box_char!['╎' -> |ctx: &Context| {
        ctx.draw_line(
            Vertical,
            HalfSelector::Both,
            ctx.get_stroke_width_pixels(Thickness::Level1),
            ctx.color_fg,
            ctx.get_dash_effect(Vertical, 1),
        );
    }];
    box_char!['╏' -> |ctx: &Context| {
        ctx.draw_line(
            Vertical,
            HalfSelector::Both,
            ctx.get_stroke_width_pixels(Thickness::Level3),
            ctx.color_fg,
            ctx.get_dash_effect(Vertical, 1),
        );
    }];
    box_char!['┆' -> |ctx: &Context| {
        ctx.draw_line(
            Vertical,
            HalfSelector::Both,
            ctx.get_stroke_width_pixels(Thickness::Level1),
            ctx.color_fg,
            ctx.get_dash_effect(Vertical, 2),
        );
    }];
    box_char!['┇' -> |ctx: &Context| {
        ctx.draw_line(
            Vertical,
            HalfSelector::Both,
            ctx.get_stroke_width_pixels(Thickness::Level3),
            ctx.color_fg,
            ctx.get_dash_effect(Vertical, 2),
        );
    }];
    box_char!['┊' -> |ctx: &Context| {
        ctx.draw_line(
            Vertical,
            HalfSelector::Both,
            ctx.get_stroke_width_pixels(Thickness::Level1),
            Color::WHITE,
            ctx.get_dash_effect(Vertical, 3),
        );
    }];
    box_char!['┋' -> |ctx: &Context| {
        ctx.draw_line(
            Vertical,
            HalfSelector::Both,
            ctx.get_stroke_width_pixels(Thickness::Level3),
            ctx.color_fg,
            ctx.get_dash_effect(Vertical, 3),
        );
    }];

    // Half lines
    box_char!['╴' -> |ctx: &Context| {
        ctx.draw_fg_line1(Horizontal, HalfSelector::First);
    }];
    box_char!['╵' -> |ctx: &Context| {
        ctx.draw_fg_line1(Vertical, HalfSelector::First);
    }];
    box_char!['╶' -> |ctx: &Context| {
        ctx.draw_fg_line1(Horizontal, HalfSelector::Last);
    }];
    box_char!['╷' -> |ctx: &Context| {
        ctx.draw_fg_line1(Vertical, HalfSelector::Last);
    }];
    box_char!['╸' -> |ctx: &Context| {
        ctx.draw_fg_line3(Horizontal, HalfSelector::First);
    }];
    box_char!['╹' -> |ctx: &Context| {
        ctx.draw_fg_line3(Vertical, HalfSelector::First);
    }];
    box_char!['╺' -> |ctx: &Context| {
        ctx.draw_fg_line3(Horizontal, HalfSelector::Last);
    }];
    box_char!['╻' -> |ctx: &Context| {
        ctx.draw_fg_line3(Vertical, HalfSelector::Last);
    }];
    box_char!['╼' -> |ctx: &Context| {
        ctx.draw_fg_line1(Horizontal, HalfSelector::First);
        ctx.draw_fg_line3(Horizontal, HalfSelector::Last);
    }];
    box_char!['╽' -> |ctx: &Context| {
        ctx.draw_fg_line1(Vertical, HalfSelector::First);
        ctx.draw_fg_line3(Vertical, HalfSelector::Last);
    }];
    box_char!['╾' -> |ctx: &Context| {
        ctx.draw_fg_line3(Horizontal, HalfSelector::First);
        ctx.draw_fg_line1(Horizontal, HalfSelector::Last);
    }];
    box_char!['╿' -> |ctx: &Context| {
        ctx.draw_fg_line3(Vertical, HalfSelector::First);
        ctx.draw_fg_line1(Vertical, HalfSelector::Last);
    }];

    box_char!['' -> |ctx: &Context| {
        ctx.draw_arrow(Side::Right);
    }];
    box_char!['' -> |ctx: &Context| {
        ctx.draw_quarter_triangle(Corner::TopRight, Height::Short);
        ctx.draw_quarter_triangle(Corner::BottomRight, Height::Short);
    }];
    box_char!['' -> |ctx: &Context| {
        ctx.draw_arrow(Side::Left);
    }];
    box_char!['' -> |ctx: &Context| {
        ctx.draw_quarter_triangle(Corner::TopLeft, Height::Short);
        ctx.draw_quarter_triangle(Corner::BottomLeft, Height::Short);
    }];
    box_char!['' -> |ctx: &Context| {
        ctx.draw_half_cross_line(Corner::TopLeft);
        ctx.draw_half_cross_line(Corner::BottomLeft);
    }];
    box_char!['' -> |ctx: &Context| {
        ctx.draw_half_cross_line(Corner::TopRight);
        ctx.draw_half_cross_line(Corner::BottomRight);
    }];
    box_char!['', '◗' -> |ctx: &Context| {
        ctx.draw_d(Side::Left, PaintStyle::Fill, true);
    }];
    box_char!['', '◖' -> |ctx: &Context| {
        ctx.draw_d(Side::Right, PaintStyle::Fill, true);
    }];
    box_char!['' -> |ctx: &Context| {
        ctx.draw_d(Side::Left, PaintStyle::Stroke, false);
    }];
    box_char!['' -> |ctx: &Context| {
        ctx.draw_d(Side::Right, PaintStyle::Stroke, false);
    }];

    box_char!['', '', '╲' -> |ctx: &Context| {
        ctx.draw_cross_line(Side::Left);
    }];
    box_char!['', '', '╱' -> |ctx: &Context| {
        ctx.draw_cross_line(Side::Right);
    }];
    box_char!['╳' -> |ctx: &Context| {
        ctx.draw_cross_line(Side::Left);
        ctx.draw_cross_line(Side::Right);
    }];

    box_char!['', '◣' -> |ctx: &Context| {
        ctx.draw_quarter_triangle(Corner::BottomLeft, Height::Tall);
    }];
    box_char!['', '◢' -> |ctx: &Context| {
        ctx.draw_quarter_triangle(Corner::BottomRight, Height::Tall);
    }];
    box_char!['', '◤' -> |ctx: &Context| {
        ctx.draw_quarter_triangle(Corner::TopLeft, Height::Tall);
    }];
    box_char!['', '◥' -> |ctx: &Context| {
        ctx.draw_quarter_triangle(Corner::TopRight, Height::Tall);
    }];

    // 
    box_char!['' -> |ctx: &Context| {
        ctx.draw_progress(Section::Left, PaintStyle::Stroke);
    }];
    box_char!['' -> |ctx: &Context| {
        ctx.draw_progress(Section::Middle, PaintStyle::Stroke);
    }];
    box_char!['' -> |ctx: &Context| {
        ctx.draw_progress(Section::Right, PaintStyle::Stroke);
    }];
    // 
    box_char!['' -> |ctx: &Context| {
        ctx.draw_progress(Section::Left, PaintStyle::Fill);
    }];
    box_char!['' -> |ctx: &Context| {
        ctx.draw_progress(Section::Middle, PaintStyle::Fill);
    }];
    box_char!['' -> |ctx: &Context| {
        ctx.draw_progress(Section::Right, PaintStyle::Fill);
    }];

    // double lines
    box_char!('═' -> |ctx: &Context|{
        ctx.draw_double_line(Horizontal, HalfSelector::Both);
    });
    box_char!('║' -> |ctx: &Context|{
        ctx.draw_double_line(Vertical, HalfSelector::Both);
    });
    box_char!['╞' -> |ctx: &Context| {
        ctx.draw_fg_line1(Vertical, HalfSelector::Both);
        ctx.draw_double_line(Horizontal, HalfSelector::Last);
    }];
    box_char!['╡' -> |ctx: &Context| {
        ctx.draw_fg_line1(Vertical, HalfSelector::Both);
        ctx.draw_double_line(Horizontal, HalfSelector::First);
    }];
    box_char!['╥' -> |ctx: &Context| {
        ctx.draw_fg_line1(Horizontal, HalfSelector::Both);
        ctx.draw_double_line(Vertical, HalfSelector::Last);
    }];
    box_char!['╨' -> |ctx: &Context| {
        ctx.draw_fg_line1(Horizontal, HalfSelector::Both);
        ctx.draw_double_line(Vertical, HalfSelector::First);
    }];
    box_char!['╪' -> |ctx: &Context| {
        ctx.draw_fg_line1(Vertical, HalfSelector::Both);
        ctx.draw_double_line(Horizontal, HalfSelector::Both);
    }];
    box_char!['╫' -> |ctx: &Context| {
        ctx.draw_fg_line1(Horizontal, HalfSelector::Both);
        ctx.draw_double_line(Vertical, HalfSelector::Both);
    }];
    box_char!['╬' -> |ctx: &Context| {
        ctx.draw_double_line(Vertical, HalfSelector::Both);
        ctx.draw_double_line(Horizontal, HalfSelector::Both);
    }];
    box_char!['╠' -> |ctx: &Context| {
        ctx.draw_double_line(Vertical, HalfSelector::Both);
        ctx.draw_double_line(Horizontal, HalfSelector::Last);
    }];
    box_char!['╣' -> |ctx: &Context| {
        ctx.draw_double_line(Vertical, HalfSelector::Both);
        ctx.draw_double_line(Horizontal, HalfSelector::First);
    }];
    box_char!['╦' -> |ctx: &Context| {
        ctx.draw_double_line(Horizontal, HalfSelector::Both);
        ctx.draw_double_line(Vertical, HalfSelector::Last);
    }];
    box_char!['╩' -> |ctx: &Context| {
        ctx.draw_double_line(Horizontal, HalfSelector::Both);
        ctx.draw_double_line(Vertical, HalfSelector::First);
    }];

    // eighth blocks
    box_char!['▀' -> |ctx: &Context| {
        ctx.draw_eighth(Horizontal, 0..=3);
    }];
    box_char!['▁' -> |ctx: &Context| {
        ctx.draw_eighth(Horizontal, 7..=7);
    }];
    box_char!['▂' -> |ctx: &Context| {
        ctx.draw_eighth(Horizontal, 6..=7);
    }];
    box_char!['▃' -> |ctx: &Context| {
        ctx.draw_eighth(Horizontal, 5..=7);
    }];
    box_char!['▄' -> |ctx: &Context| {
        ctx.draw_eighth(Horizontal, 4..=7);
    }];
    box_char!['▅' -> |ctx: &Context| {
        ctx.draw_eighth(Horizontal, 3..=7);
    }];
    box_char!['▆' -> |ctx: &Context| {
        ctx.draw_eighth(Horizontal, 2..=7);
    }];
    box_char!['▇' -> |ctx: &Context| {
        ctx.draw_eighth(Horizontal, 1..=7);
    }];
    box_char!['█' -> |ctx: &Context| {
        let mut paint = ctx.fg_paint();
        paint.set_style(PaintStyle::Fill);
        ctx.canvas.draw_paint(&paint);
    }];
    box_char!['▉' -> |ctx: &Context| {
        ctx.draw_eighth(Vertical, 0..=6);
    }];
    box_char!['▊' -> |ctx: &Context| {
        ctx.draw_eighth(Vertical, 0..=5);
    }];
    box_char!['▋' -> |ctx: &Context| {
        ctx.draw_eighth(Vertical, 0..=4);
    }];
    box_char!['▌' -> |ctx: &Context| {
        ctx.draw_eighth(Vertical, 0..=3);
    }];
    box_char!['▍' -> |ctx: &Context| {
        ctx.draw_eighth(Vertical, 0..=2);
    }];
    box_char!['▎' -> |ctx: &Context| {
        ctx.draw_eighth(Vertical, 0..=1);
    }];
    box_char!['▏' -> |ctx: &Context| {
        ctx.draw_eighth(Vertical, 0..=0);
    }];
    box_char!['▐' -> |ctx: &Context| {
        ctx.draw_eighth(Vertical, 4..=7);
    }];
    box_char!['▔' -> |ctx: &Context| {
        ctx.draw_eighth(Horizontal, 0..=0);
    }];
    box_char!['▕' -> |ctx: &Context| {
        ctx.draw_eighth(Vertical, 7..=7);
    }];
    box_char!['🭼' -> |ctx: &Context| {
        ctx.draw_eighth(Vertical, 0..=0);
        ctx.draw_eighth(Horizontal, 7..=7);
    }];
    box_char!['🭼' -> |ctx: &Context| {
        ctx.draw_eighth(Vertical, 0..=0);
        ctx.draw_eighth(Horizontal, 0..=0);
    }];
    box_char!['🭾' -> |ctx: &Context| {
        ctx.draw_eighth(Vertical, 7..=7);
        ctx.draw_eighth(Horizontal, 0..=0);
    }];
    box_char!['🭿' -> |ctx: &Context| {
        ctx.draw_eighth(Vertical, 7..=7);
        ctx.draw_eighth(Horizontal, 7..=7);
    }];
    box_char!['🮀' -> |ctx: &Context| {
        ctx.draw_eighth(Horizontal, 0..=0);
        ctx.draw_eighth(Horizontal, 7..=7);
    }];
    box_char!['🮁' -> |ctx: &Context| {
        ctx.draw_eighth(Horizontal, 0..=0);
        ctx.draw_eighth(Horizontal, 2..=2);
        ctx.draw_eighth(Horizontal, 4..=4);
        ctx.draw_eighth(Horizontal, 7..=7);
    }];
    box_char!['🮂' -> |ctx: &Context| {
        ctx.draw_eighth(Horizontal, 0..=1);
    }];
    box_char!['🮃' -> |ctx: &Context| {
        ctx.draw_eighth(Horizontal, 0..=2);
    }];
    box_char!['🮄' -> |ctx: &Context| {
        ctx.draw_eighth(Horizontal, 0..=4);
    }];
    box_char!['🮅' -> |ctx: &Context| {
        ctx.draw_eighth(Horizontal, 0..=5);
    }];
    box_char!['🮆' -> |ctx: &Context| {
        ctx.draw_eighth(Horizontal, 0..=6);
    }];
    box_char!['🮇' -> |ctx: &Context| {
        ctx.draw_eighth(Vertical, 6..=7);
    }];
    box_char!['🮈' -> |ctx: &Context| {
        ctx.draw_eighth(Vertical, 5..=7);
    }];
    box_char!['🮉' -> |ctx: &Context| {
        ctx.draw_eighth(Vertical, 3..=7);
    }];
    box_char!['🮊' -> |ctx: &Context| {
        ctx.draw_eighth(Vertical, 2..=7);
    }];
    box_char!['🮋' -> |ctx: &Context| {
        ctx.draw_eighth(Vertical, 1..=7);
    }];
    box_char!['🮂' -> |ctx: &Context| {
        ctx.draw_eighth(Horizontal, 0..=1);
    }];
    box_char!['🮃' -> |ctx: &Context| {
        ctx.draw_eighth(Horizontal, 0..=2);
    }];
    box_char!['🮄' -> |ctx: &Context| {
        ctx.draw_eighth(Horizontal, 0..=4);
    }];
    box_char!['🮅' -> |ctx: &Context| {
        ctx.draw_eighth(Horizontal, 0..=5);
    }];
    box_char!['🮆' -> |ctx: &Context| {
        ctx.draw_eighth(Horizontal, 0..=6);
    }];
    box_char!['🮇' -> |ctx: &Context| {
        ctx.draw_eighth(Horizontal, 6..=7);
    }];
    box_char!['🮈' -> |ctx: &Context| {
        ctx.draw_eighth(Horizontal, 5..=7);
    }];
    box_char!['🮉' -> |ctx: &Context| {
        ctx.draw_eighth(Horizontal, 3..=7);
    }];
    box_char!['🮊' -> |ctx: &Context| {
        ctx.draw_eighth(Horizontal, 2..=7);
    }];
    box_char!['🮋' -> |ctx: &Context| {
        ctx.draw_eighth(Horizontal, 1..=7);
    }];

    // Shade
    box_char!['░' -> |ctx: &Context| {
        ctx.draw_shade(Orientation::Horizontal, HalfSelector::Both, Shade::Light, MirrorMode::Normal, ColorMode::Normal);
    }];
    box_char!['▒' -> |ctx: &Context| {
        ctx.draw_shade(Orientation::Horizontal, HalfSelector::Both, Shade::Medium, MirrorMode::Normal, ColorMode::Normal);
    }];
    box_char!['▓' -> |ctx: &Context| {
        ctx.draw_shade(Orientation::Horizontal, HalfSelector::Both, Shade::Dark, MirrorMode::Normal, ColorMode::Normal);
    }];
    box_char!['🮌' -> |ctx: &Context| {
        ctx.draw_shade(Orientation::Horizontal, HalfSelector::First, Shade::Medium, MirrorMode::Normal, ColorMode::Normal);
    }];
    box_char!['🮍' -> |ctx: &Context| {
        ctx.draw_shade(Orientation::Horizontal, HalfSelector::Last, Shade::Medium, MirrorMode::Normal, ColorMode::Normal);
    }];
    box_char!['🮎' -> |ctx: &Context| {
        ctx.draw_shade(Orientation::Vertical, HalfSelector::First, Shade::Medium, MirrorMode::Normal, ColorMode::Normal);
    }];
    box_char!['🮏' -> |ctx: &Context| {
        ctx.draw_shade(Orientation::Vertical, HalfSelector::Last, Shade::Medium, MirrorMode::Normal, ColorMode::Normal);
    }];
    box_char!['🮐' -> |ctx: &Context| {
        ctx.draw_shade(Orientation::Horizontal, HalfSelector::Both, Shade::Medium, MirrorMode::Normal, ColorMode::Inverted);
    }];
    box_char!['🮑' -> |ctx: &Context| {
        ctx.draw_shade(Orientation::Vertical, HalfSelector::Last, Shade::Medium, MirrorMode::Normal, ColorMode::Inverted);
    }];
    box_char!['🮒' -> |ctx: &Context| {
        ctx.draw_shade(Orientation::Vertical, HalfSelector::First, Shade::Medium, MirrorMode::Normal, ColorMode::Inverted);
    }];
    box_char!['🮓' -> |ctx: &Context| {
        ctx.draw_shade(Orientation::Horizontal, HalfSelector::Last, Shade::Medium, MirrorMode::Normal, ColorMode::Inverted);
    }];
    box_char!['🮔' -> |ctx: &Context| {
        ctx.draw_shade(Orientation::Horizontal, HalfSelector::First, Shade::Medium, MirrorMode::Normal, ColorMode::Inverted);
    }];
    box_char!['🮜' -> |ctx: &Context| {
        ctx.canvas.clip_path(&ctx.triangle_path(Corner::TopLeft), None, Some(false));
        ctx.draw_shade(Orientation::Horizontal, HalfSelector::Both, Shade::Medium,  MirrorMode::Normal, ColorMode::Normal);
    }];
    box_char!['🮝' -> |ctx: &Context| {
        ctx.canvas.clip_path(&ctx.triangle_path(Corner::TopRight), None, Some(false));
        ctx.draw_shade(Orientation::Horizontal, HalfSelector::Both, Shade::Medium,  MirrorMode::Normal, ColorMode::Normal);
    }];
    box_char!['🮞' -> |ctx: &Context| {
        ctx.canvas.clip_path(&ctx.triangle_path(Corner::BottomRight), None, Some(false));
        ctx.draw_shade(Orientation::Horizontal, HalfSelector::Both, Shade::Medium,  MirrorMode::Normal, ColorMode::Normal);
    }];
    // 🮜🮝
    // 🮞🮟
    // 🮝🮜
    // 🮟🮞
    box_char!['🮟' -> |ctx: &Context| {
        ctx.canvas.clip_path(&ctx.triangle_path(Corner::BottomLeft), None, Some(false));
        ctx.draw_shade(Orientation::Horizontal, HalfSelector::Both, Shade::Medium,  MirrorMode::Normal, ColorMode::Normal);
    }];
    // 🮙🮙🮙🮙🮙🮙🮙🮙🮙🮙
    // 🮙🮙🮙🮙🮙🮙🮙🮙🮙🮙
    box_char!['🮙' -> |ctx: &Context| {
        ctx.draw_shade(Orientation::Horizontal, HalfSelector::Both, Shade::Light,  MirrorMode::Normal, ColorMode::Normal);
    }];
    // 🮘🮘🮘🮘🮘🮘🮘🮘🮘🮘
    // 🮘🮘🮘🮘🮘🮘🮘🮘🮘🮘
    box_char!['🮘' -> |ctx: &Context| {
       ctx.draw_shade(Orientation::Horizontal, HalfSelector::Both, Shade::Light,  MirrorMode::Mirror, ColorMode::Normal);
    }];

    // ╭╮╰╯
    // ╭──────────╮
    // │          │
    // ╰──────────╯
    box_char!['╭' -> |ctx: &Context| {
        ctx.draw_rounded_corner(Corner::TopLeft);
    }];
    box_char!['╮' -> |ctx: &Context| {
        ctx.draw_rounded_corner(Corner::TopRight);
    }];
    box_char!['╰' -> |ctx: &Context| {
        ctx.draw_rounded_corner(Corner::BottomLeft);
    }];
    box_char!['╯' -> |ctx: &Context| {
        ctx.draw_rounded_corner(Corner::BottomRight);
    }];

    // T joints
    {
        use Thickness::{Level1 as t1, Level3 as t3};
        macro_rules! t_joint {
            ($($ch:literal -> $north:ident, $east:ident, $south:ident, $west:ident)+) => {
                $(m.insert(
                    $ch,
                    Box::new(move |ctx: &Context| {
                        ctx.draw_t_joint($north, $east, $south, $west);
                    }),
                ));+
            };
        }

        t_joint![
        // ┬ ┭ ┮ ┯ ┰ ┱ ┲ ┳
        '┬' -> None, t1, t1, t1
        '┭' -> None, t1, t1, t3
        '┮' -> None, t1, t3, t1
        '┯' -> None, t1, t3, t3
        '┰' -> None, t3, t1, t1
        '┱' -> None, t3, t1, t3
        '┲' -> None, t3, t3, t1
        '┳' -> None, t3, t3, t3

        // ┤ ┥ ┦ ┧ ┨ ┩ ┪ ┫
        '┤' -> t1, None, t1, t1
        '┥' -> t1, None, t1, t3
        '┦' -> t3, None, t1, t1
        '┧' -> t1, None, t3, t1
        '┨' -> t3, None, t3, t1
        '┩' -> t3, None, t1, t3
        '┪' -> t1, None, t3, t3
        '┫' -> t3, None, t3, t3

        // ┴ ┵ ┶ ┷ ┸ ┹ ┺ ┻
        '┴' -> t1, t1, None, t1
        '┵' -> t1, t1, None, t3
        '┶' -> t3, t1, None, t1
        '┷' -> t3, t1, None, t3
        '┸' -> t1, t3, None, t1
        '┹' -> t1, t3, None, t3
        '┺' -> t3, t3, None, t1
        '┻' -> t3, t3, None, t3

        // ├ ┝ ┞ ┟ ┠ ┡ ┢ ┣
        '├' -> t1, t1, t1, None
        '┝' -> t1, t3, t1, None
        '┞' -> t3, t1, t1, None
        '┟' -> t1, t1, t3, None
        '┠' -> t3, t1, t3, None
        '┡' -> t3, t3, t1, None
        '┢' -> t1, t3, t3, None
        '┣' -> t3, t3, t3, None
        ];
    }

    // Corners
    // ┌ ┍ ┎ ┏
    // ┐ ┑ ┒ ┓
    // └ ┕ ┖ ┗
    // ┘ ┙ ┚ ┛
    //
    // Test 1:
    // ┌─┐
    // │ │
    // └─┘
    //
    // Test 2:
    // ┍━┑
    // │ │
    // ┕━┙
    //
    // Test 3:
    // ┎─┒
    // ┃ ┃
    // ┖─┚
    //
    // Test 4:
    // ┏━┓
    // ┃ ┃
    // ┗━┛
    {
        use Corner::*;
        use Thickness::{Level1 as t1, Level3 as t3};
        macro_rules! corner {
            ($($ch:literal -> $corner:ident, $horiz:ident, $vert:ident)+) => {
                $(m.insert(
                    $ch,
                    Box::new(move |ctx: &Context| {
                        ctx.draw_corner($corner, $horiz, $vert);
                    }),
                ));+
            };
        }
        corner![
            '┌' -> TopLeft, t1, t1
            '┍' -> TopLeft, t3, t1
            '┎' -> TopLeft, t1, t3
            '┏' -> TopLeft, t3, t3

            '┐' -> TopRight, t1, t1
            '┑' -> TopRight, t3, t1
            '┒' -> TopRight, t1, t3
            '┓' -> TopRight, t3, t3

            '└' -> BottomLeft, t1, t1
            '┕' -> BottomLeft, t3, t1
            '┖' -> BottomLeft, t1, t3
            '┗' -> BottomLeft, t3, t3

            '┘' -> BottomRight, t1, t1
            '┙' -> BottomRight, t3, t1
            '┚' -> BottomRight, t1, t3
            '┛' -> BottomRight, t3, t3
        ];
    }

    m
});

pub fn is_box_char(text: &str) -> bool {
    text.chars()
        .next()
        .is_some_and(|ch| BOX_CHARS.contains_key(&ch))
}

pub struct Renderer {
    settings: BoxDrawingSettings,
    cell_size: Size2<Pixel<f32>>,
}

impl Renderer {
    pub fn new(cell_size: Size2<Pixel<f32>>, settings: BoxDrawingSettings) -> Self {
        Self {
            settings,
            cell_size,
        }
    }

    pub fn update_dimensions(&mut self, new_cell_size: Size2<Pixel<f32>>) {
        if self.cell_size != new_cell_size {
            self.cell_size = new_cell_size;
        }
    }

    pub fn update_settings(&mut self, settings: BoxDrawingSettings) {
        if self.settings != settings {
            self.settings = settings;
        }
    }

    pub fn draw_glyph(
        &self,
        box_char_text: &str,
        canvas: &Canvas,
        dst: PixelRect<f32>,
        color_fg: Color,
    ) -> bool {
        match self
            .settings
            .mode
            .as_ref()
            .unwrap_or(&BoxDrawingMode::default())
        {
            BoxDrawingMode::FontGlyph => false,
            BoxDrawingMode::Native => self.draw_box_glyph(box_char_text, canvas, dst, color_fg),
            BoxDrawingMode::SelectedNative => {
                let selected = self.settings.selected.as_deref().unwrap_or("");
                let is_selected = box_char_text
                    .chars()
                    .next()
                    .is_some_and(|first| selected.contains(first));
                if is_selected {
                    self.draw_box_glyph(box_char_text, canvas, dst, color_fg)
                } else {
                    false
                }
            }
        }
    }

    fn draw_box_glyph(
        &self,
        box_char_text: &str,
        canvas: &Canvas,
        dst: PixelRect<f32>,
        color_fg: Color,
    ) -> bool {
        let Some(ch) = box_char_text.chars().next() else {
            return false;
        };
        let Some(draw_fn) = BOX_CHARS.get(&ch) else {
            return false;
        };
        for (i, _) in box_char_text.chars().enumerate() {
            canvas.save();
            let rect = Box2::from_rect(glamour::Rect::new(
                dst.min + Vector2::new(self.cell_size.width * i as f32, 0.0),
                self.cell_size,
            ));
            canvas.clip_rect(to_skia_rect(&rect), None, Some(false));
            let ctx = Context::new(canvas, &self.settings, rect, color_fg);
            (draw_fn)(&ctx);
            canvas.restore();
        }
        true
    }
}
