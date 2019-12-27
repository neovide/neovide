use std::sync::{Arc, Mutex};

use skulpin::skia_safe::{Canvas, Paint, Path, Point};

use crate::renderer::{CachingShaper, FontLookup};
use crate::editor::{Colors, Cursor, CursorShape, Editor};

const AVERAGE_MOTION_PERCENTAGE: f32 = 0.8;
const MOTION_PERCENTAGE_SPREAD: f32 = 0.5;

const BAR_WIDTH: f32 = 1.0 / 8.0;

const STANDARD_CORNERS: &[(f32, f32); 4] = &[(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)];

#[derive(Debug, Clone)]
pub struct Corner {
    pub current_position: Point,
    pub relative_position: Point,
}

impl Corner {
    pub fn new(relative_position: Point) -> Corner {
        Corner {
            current_position: Point::new(0.0, 0.0),
            relative_position
        }
    }

    pub fn update(&mut self, font_dimensions: Point, destination: Point) -> bool {
        let relative_scaled_position: Point = 
            (self.relative_position.x * font_dimensions.x, self.relative_position.y * font_dimensions.y).into();
        let corner_destination = destination + relative_scaled_position;

        let delta = corner_destination - self.current_position;

        if delta.length() > 0.0 {
            let motion_scale = delta.dot(relative_scaled_position) / delta.length() / font_dimensions.length();
            let motion_percentage = motion_scale * MOTION_PERCENTAGE_SPREAD + AVERAGE_MOTION_PERCENTAGE;

            let delta = corner_destination - self.current_position;
            self.current_position += delta * motion_percentage;
        }

        delta.length() > 0.001
    }
}

pub struct CursorRenderer {
    pub corners: Vec<Corner>
}

impl CursorRenderer {
    pub fn new() -> CursorRenderer {
        let mut renderer = CursorRenderer {
            corners: vec![Corner::new((0.0, 0.0).into()); 4]
        };
        renderer.set_cursor_shape(&CursorShape::Block);
        renderer
    }

    fn set_cursor_shape(&mut self, cursor_shape: &CursorShape) {
        self.corners = self.corners
            .clone()
            .into_iter().enumerate()
            .map(|(i, corner)| {
                let (x, y) = STANDARD_CORNERS[i];
                Corner {
                    relative_position: match cursor_shape {
                        CursorShape::Block => (x, y).into(),
                        CursorShape::Vertical => ((x + 0.5) * BAR_WIDTH - 0.5, y).into(),
                        CursorShape::Horizontal => (x, (y + 0.5) * BAR_WIDTH - 0.5).into()
                    },
                    .. corner
                }
            })
            .collect::<Vec<Corner>>();
    }

    pub fn draw(&mut self, 
            cursor: Cursor, default_colors: &Colors, 
            font_width: f32, font_height: f32,
            paint: &mut Paint, editor: Arc<Mutex<Editor>>,
            shaper: &mut CachingShaper, fonts_lookup: &mut FontLookup,
            canvas: &mut Canvas) -> bool {
        let (grid_x, grid_y) = cursor.position;
        let font_dimensions: Point = (font_width, font_height).into();
        let center_destination: Point = (
            grid_x as f32 * font_width + font_width / 2.0, 
            grid_y as f32 * font_height + font_height / 2.0
        ).into();

        self.set_cursor_shape(&cursor.shape);

        let mut animating = false;
        if !center_destination.is_zero() {
            for corner in self.corners.iter_mut() {
                let corner_animating = corner.update(font_dimensions, center_destination);
                animating = animating || corner_animating;
            }
        }


        if cursor.enabled {
            // Draw Background
            paint.set_color(cursor.background(&default_colors).to_color());

            let mut path = Path::new();
            path.move_to(self.corners[0].current_position);
            path.line_to(self.corners[1].current_position);
            path.line_to(self.corners[2].current_position);
            path.line_to(self.corners[3].current_position);
            path.close();

            canvas.draw_path(&path, &paint);

            let mut position_sum: Point = (0.0, 0.0).into();
            for i in 0..4 {
                position_sum += self.corners[i].current_position;
            }
            let Point { x: cursor_x, y: cursor_y } = position_sum * (1.0 / 4.0) - font_dimensions * 0.5;

            // Draw foreground
            if let CursorShape::Block = cursor.shape {
                let (cursor_grid_y, cursor_grid_x) = cursor.position;
                paint.set_color(cursor.foreground(&default_colors).to_color());
                let editor = editor.lock().unwrap();
                let character = editor.grid[cursor_grid_x as usize][cursor_grid_y as usize].clone()
                    .map(|(character, _)| character)
                    .unwrap_or(' ');
                canvas.draw_text_blob(
                    shaper.shape_cached(character.to_string(), &fonts_lookup.size(1).normal), 
                    (cursor_x, cursor_y), &paint);
            }
        }

        animating
    }
}
