use std::time::{Duration, Instant};

use skulpin::skia_safe::{Canvas, Paint, Path, Point};

use crate::renderer::CachingShaper;
use crate::editor::{EDITOR, Colors, Cursor, CursorShape};
use crate::redraw_scheduler::REDRAW_SCHEDULER;

const AVERAGE_MOTION_PERCENTAGE: f32 = 0.7;
const MOTION_PERCENTAGE_SPREAD: f32 = 0.5;
const COMMAND_LINE_DELAY_FRAMES: u64 = 5;
const DEFAULT_CELL_PERCENTAGE: f32 = 1.0 / 8.0;

const STANDARD_CORNERS: &[(f32, f32); 4] = &[(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)];

enum BlinkState {
    Waiting,
    On,
    Off
}

struct BlinkStatus {
    state: BlinkState,
    last_transition: Instant,
    previous_cursor: Option<Cursor>
}

impl BlinkStatus {
    pub fn new() -> BlinkStatus {
        BlinkStatus {
            state: BlinkState::Waiting,
            last_transition: Instant::now(),
            previous_cursor: None
        }
    }

    pub fn update_status(&mut self, new_cursor: &Cursor) -> bool {
        if self.previous_cursor.is_none() || new_cursor != self.previous_cursor.as_ref().unwrap() {
            self.previous_cursor = Some(new_cursor.clone());
            self.last_transition = Instant::now();
            if new_cursor.blinkwait.is_some() && new_cursor.blinkwait != Some(0) {
                self.state = BlinkState::Waiting;
            } else {
                self.state = BlinkState::On;
            }
        } 

        if new_cursor.blinkwait == Some(0) || 
            new_cursor.blinkoff == Some(0) ||
            new_cursor.blinkon == Some(0) {
            return true;
        }

        let delay = match self.state {
            BlinkState::Waiting => new_cursor.blinkwait,
            BlinkState::Off => new_cursor.blinkoff,
            BlinkState::On => new_cursor.blinkon
        }.filter(|millis| *millis > 0).map(Duration::from_millis);

        if delay.map(|delay| self.last_transition + delay < Instant::now()).unwrap_or(false) {
            self.state = match self.state {
                BlinkState::Waiting => BlinkState::On,
                BlinkState::On => BlinkState::Off,
                BlinkState::Off => BlinkState::On
            };
            self.last_transition = Instant::now();
        }

        let scheduled_frame = (match self.state {
            BlinkState::Waiting => new_cursor.blinkwait,
            BlinkState::Off => new_cursor.blinkoff,
            BlinkState::On => new_cursor.blinkon
        }).map(|delay| self.last_transition + Duration::from_millis(delay));

        if let Some(scheduled_frame) = scheduled_frame {
            REDRAW_SCHEDULER.schedule(scheduled_frame);
        }

        match self.state {
            BlinkState::Waiting | BlinkState::Off => false,
            BlinkState::On => true
        }
    }
}

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
            // Project relative_scaled_position (actual possition of the corner relative to the
            // center of the cursor) onto the remaining distance vector. This gives us the relative
            // distance to the destination along the delta vector which we can then use to scale the
            // motion_percentage.
            let motion_scale = delta.dot(relative_scaled_position) / delta.length() / font_dimensions.length();

            // The motion_percentage is then equal to the motion_scale factor times the
            // MOTION_PERCENTAGE_SPREAD and added to the AVERAGE_MOTION_PERCENTAGE. This way all of
            // the percentages are positive and spread out by the spread constant.
            let motion_percentage = motion_scale * MOTION_PERCENTAGE_SPREAD + AVERAGE_MOTION_PERCENTAGE;

            // Then the current_position is animated by taking the delta vector, multiplying it by
            // the motion_percentage and adding the resulting value to the current position causing
            // the cursor to "jump" toward the target destination. Since further away corners jump
            // slower, the cursor appears to smear toward the destination in a satisfying and
            // visually trackable way.
            let delta = corner_destination - self.current_position;
            self.current_position += delta * motion_percentage;
        }

        delta.length() > 0.001
    }
}

pub struct CursorRenderer {
    pub corners: Vec<Corner>,
    pub previous_position: (u64, u64),
    pub command_line_delay: u64,
    blink_status: BlinkStatus
}

impl CursorRenderer {
    pub fn new() -> CursorRenderer {
        let mut renderer = CursorRenderer {
            corners: vec![Corner::new((0.0, 0.0).into()); 4],
            previous_position: (0, 0),
            command_line_delay: 0,
            blink_status: BlinkStatus::new()
        };
        renderer.set_cursor_shape(&CursorShape::Block, DEFAULT_CELL_PERCENTAGE);
        renderer
    }

    fn set_cursor_shape(&mut self, cursor_shape: &CursorShape, cell_percentage: f32) {
        self.corners = self.corners
            .clone()
            .into_iter().enumerate()
            .map(|(i, corner)| {
                let (x, y) = STANDARD_CORNERS[i];
                Corner {
                    relative_position: match cursor_shape {
                        CursorShape::Block => (x, y).into(),
                        // Transform the x position so that the right side is translated over to
                        // the BAR_WIDTH position
                        CursorShape::Vertical => ((x + 0.5) * cell_percentage - 0.5, y).into(),
                        // Do the same as above, but flip the y coordinate and then flip the result
                        // so that the horizontal bar is at the bottom of the character space
                        // instead of the top.
                        CursorShape::Horizontal => (x, -((-y + 0.5) * cell_percentage - 0.5)).into()
                    },
                    .. corner
                }
            })
            .collect::<Vec<Corner>>();
    }

    pub fn draw(&mut self, 
            cursor: Cursor, default_colors: &Colors, 
            font_width: f32, font_height: f32,
            paint: &mut Paint, shaper: &mut CachingShaper, 
            canvas: &mut Canvas) {
        let render = self.blink_status.update_status(&cursor);

        self.previous_position = {
            let editor = EDITOR.lock();
            let (_, grid_y) = cursor.position;
            let (_, previous_y) = self.previous_position;
            if grid_y == editor.grid.height - 1 && previous_y != grid_y {
                self.command_line_delay += 1;
                if self.command_line_delay < COMMAND_LINE_DELAY_FRAMES {
                    self.previous_position
                } else {
                    self.command_line_delay = 0;
                    cursor.position
                }
            } else {
                self.command_line_delay = 0;
                cursor.position
            }
        };

        let (grid_x, grid_y) = self.previous_position;

        let (character, font_dimensions): (String, Point) = {
            let editor = EDITOR.lock();
            let character = match editor.grid.get_cell(grid_x, grid_y) {
                Some(Some((character, _))) => character.clone(),
                _ => ' '.to_string(),
            };
            
            let is_double = match editor.grid.get_cell(grid_x + 1, grid_y) {
                Some(Some((character, _))) => character.is_empty(),
                _ => false,
            };

            let font_width = match (is_double, &cursor.shape) {
                (true, CursorShape::Block) => font_width * 2.0,
                _ => font_width
            };
            (character, (font_width, font_height).into())
        };
        let destination: Point = (grid_x as f32 * font_width, grid_y as f32 * font_height).into();
        let center_destination = destination + font_dimensions * 0.5;

        self.set_cursor_shape(&cursor.shape, cursor.cell_percentage.unwrap_or(DEFAULT_CELL_PERCENTAGE));

        let mut animating = false;
        if !center_destination.is_zero() {
            for corner in self.corners.iter_mut() {
                let corner_animating = corner.update(font_dimensions, center_destination);
                animating = animating || corner_animating;
            }
        }

        if animating {
            REDRAW_SCHEDULER.queue_next_frame();
        }

        if cursor.enabled && render {
            // Draw Background
            paint.set_color(cursor.background(&default_colors).to_color());

            // The cursor is made up of four points, so I create a path with each of the four
            // corners.
            let mut path = Path::new();
            path.move_to(self.corners[0].current_position);
            path.line_to(self.corners[1].current_position);
            path.line_to(self.corners[2].current_position);
            path.line_to(self.corners[3].current_position);
            path.close();
            canvas.draw_path(&path, &paint);

            // Draw foreground
            paint.set_color(cursor.foreground(&default_colors).to_color());
            canvas.save();
            canvas.clip_path(&path, None, Some(false));
            
            let blobs = &shaper.shape_cached(&character, false, false);
            for blob in blobs.iter() {
                canvas.draw_text_blob(&blob, destination, &paint);
            }
            canvas.restore();
        }
    }
}
