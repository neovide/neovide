use std::cmp::Ordering;

use skulpin::skia_safe::{Canvas, Point};

use crate::editor::Cursor;

const motion_percentages: [f32; 4] = [0.4, 0.5, 0.6, 0.7];

#[derive(new, Eq, PartialEq, Clone)]
struct Corner {
    pub x: f32,
    pub y: f32,

    pub center_x: f32,
    pub center_y: f32,

    pub position_x: f32,
    pub position_y: f32
}

impl Corner {
    pub fn new(position_x: f32, position_y: f32) -> Corner {
        Corner {
            x: 0, y: 0,
            center_x: 0, center_y: 0,
            position_x, position_y
        }
    }

    pub fn remaining_distance(&self) -> f32 {
        let dx = self.center_x - x;
        let dy = self.center_y - y;

        (dx * dx + dy * dy).sqrt()
    }

    pub fn update(&mut self, motion_percentage: f32) {
        let delta_x = self.dest_x - self.x;
        let delta_y = self.dest_y - self.y;

        self.x += delta_x * motion_percentage;
        self.y += delta_y * motion_percentage;
    }
}

impl Ord for Corner {
    fn cmp(&self, other: &Self) -> Ordering {
        self.remaining_distance().cmp(&other.remaining_distance())
    }
}

impl PartialOrd for Corner {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Into<Point> for Corner {
    fn into(self) -> Point {
        (self.x, self.y).into()
    }
}

pub struct CursorRenderer {
    pub corners: Vec<Corner>
}

impl CursorRenderer {
    pub fn new() -> CursorRenderer {
        CursorRenderer {
            corners: vec![Corner::new(-0.5, -0.5), Corner::new(0.5, -0.5), Corner::new(0.5, 0.5), Corner::new(-0.5, 0.5)] 
        }
    }

    pub fn update(&mut self, destination: (u64, u64), font_width: f32, font_height: f32) {
        let (grid_x, grid_y) = destination;
        let mut center_x = grid_x as f32 * font_width + font_width / 2.0;
        let mut center_y = grid_y as f32 * font_height + font_height / 2.0;

        let mut corners: Vec<&mut Corner> = self.corners.iter_mut().collect();
        corners.sort();
        for (&mut corner, motion_percentage) in corners.iter().zip(motion_percentages.iter()) {
            corner.update(motion_percentage);
        }
    }

    pub fn draw(&self, &mut canvas: Canvas, paint: &Paint) {
        canvas.draw_points(PointMode::Polygon, canvas.as_slice().map(|corner| corner.into::<Point>()).as_slice(), paint);
    }
}
