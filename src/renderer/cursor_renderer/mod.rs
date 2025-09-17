mod blink;
mod cursor_vfx;

use std::{collections::HashMap, sync::Arc};

use approx::AbsDiffEq;
use itertools::Itertools;
use skia_safe::{op, Canvas, Paint, Path};
use winit::event::WindowEvent;

use crate::{
    bridge::EditorMode,
    editor::{Cursor, CursorShape},
    profiling::{tracy_plot, tracy_zone},
    renderer::{animation_utils::*, GridRenderer, RenderedWindow},
    settings::{ParseFromValue, Settings},
    units::{
        to_skia_point, GridPos, GridScale, GridSize, PixelPos, PixelRect, PixelSize, PixelVec,
    },
    window::ShouldRender,
};

use blink::*;

const DEFAULT_CELL_PERCENTAGE: f32 = 1.0 / 8.0;

const STANDARD_CORNERS: &[(f32, f32); 4] = &[(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)];

#[cfg(feature = "profiling")]
use std::ffi::{c_char, CStr};
#[cfg(feature = "profiling")]
static PLOT_NAMES_X: [&CStr; 4] = [
    unsafe { CStr::from_ptr(b"Cursor top left x\0".as_ptr() as *const c_char) },
    unsafe { CStr::from_ptr(b"Cursor top right x\0".as_ptr() as *const c_char) },
    unsafe { CStr::from_ptr(b"Cursor bottom right x\0".as_ptr() as *const c_char) },
    unsafe { CStr::from_ptr(b"Cursor bottom left x\0".as_ptr() as *const c_char) },
];

#[cfg(feature = "profiling")]
static PLOT_NAMES_Y: [&CStr; 4] = [
    unsafe { CStr::from_ptr(b"Cursor top left y\0".as_ptr() as *const c_char) },
    unsafe { CStr::from_ptr(b"Cursor top right y\0".as_ptr() as *const c_char) },
    unsafe { CStr::from_ptr(b"Cursor bottom right y\0".as_ptr() as *const c_char) },
    unsafe { CStr::from_ptr(b"Cursor bottom left y\0".as_ptr() as *const c_char) },
];

#[derive(SettingGroup)]
#[setting_prefix = "cursor"]
#[derive(Clone)]
pub struct CursorSettings {
    antialiasing: bool,
    animation_length: f32,
    distance_length_adjust: bool,
    animate_in_insert_mode: bool,
    animate_command_line: bool,
    trail_size: f32,
    unfocused_outline_width: f32,
    smooth_blink: bool,

    vfx_mode: cursor_vfx::VfxModeList,
    vfx_opacity: f32,
    vfx_particle_lifetime: f32,
    vfx_particle_highlight_lifetime: f32,
    vfx_particle_density: f32,
    vfx_particle_speed: f32,
    vfx_particle_phase: f32,
    vfx_particle_curl: f32,
}

impl Default for CursorSettings {
    fn default() -> Self {
        CursorSettings {
            antialiasing: true,
            animation_length: 0.150,
            distance_length_adjust: true,
            animate_in_insert_mode: true,
            animate_command_line: true,
            trail_size: 1.0,
            unfocused_outline_width: 1.0 / 8.0,
            smooth_blink: false,
            vfx_mode: cursor_vfx::VfxModeList::default(),
            vfx_opacity: 200.0,
            vfx_particle_lifetime: 0.5,
            vfx_particle_highlight_lifetime: 0.2,
            vfx_particle_density: 0.7,
            vfx_particle_speed: 10.0,
            vfx_particle_phase: 1.5,
            vfx_particle_curl: 1.0,
        }
    }
}

#[derive(Clone)]
pub struct Corner {
    current_position: PixelPos<f32>,
    relative_position: GridPos<f32>,
    previous_destination: PixelPos<f32>,
    animation_x: CriticallyDampedSpringAnimation,
    animation_y: CriticallyDampedSpringAnimation,
    animation_length: f32,
    #[cfg(feature = "profiling")]
    id: usize,
}

impl Corner {
    pub fn new() -> Corner {
        Corner {
            current_position: PixelPos::default(),
            relative_position: GridPos::<f32>::default(),
            previous_destination: PixelPos::new(-1000.0, -1000.0),
            animation_x: CriticallyDampedSpringAnimation::new(),
            animation_y: CriticallyDampedSpringAnimation::new(),
            animation_length: 0.0,
            #[cfg(feature = "profiling")]
            id: 0,
        }
    }

    pub fn update(
        &mut self,
        cursor_dimensions: GridScale,
        destination: PixelPos<f32>,
        dt: f32,
        immediate_movement: bool,
    ) -> bool {
        let corner_destination = self.get_destination(destination, cursor_dimensions);
        if corner_destination != self.previous_destination {
            let delta = corner_destination - self.current_position;

            self.animation_x.position = delta.x;
            self.animation_y.position = delta.y;
            self.previous_destination = corner_destination;
        }

        if immediate_movement {
            self.current_position = corner_destination;
            return false;
        }

        let mut animating = self.animation_x.update(dt, self.animation_length);
        animating |= self.animation_y.update(dt, self.animation_length);
        self.current_position.x = corner_destination.x - self.animation_x.position;
        self.current_position.y = corner_destination.y - self.animation_y.position;

        #[cfg(feature = "profiling")]
        {
            tracy_plot!(PLOT_NAMES_X[self.id], self.current_position.x.into());
            tracy_plot!(PLOT_NAMES_Y[self.id], self.current_position.y.into());
        }

        animating
    }

    fn jump(
        &mut self,
        settings: &CursorSettings,
        destination: PixelPos<f32>,
        cursor_dimensions: GridScale,
        rank: usize,
    ) {
        let corner_destination = self.get_destination(destination, cursor_dimensions);
        let jump_vec = (corner_destination - self.previous_destination) / cursor_dimensions;

        self.animation_length = if jump_vec.x.abs() <= 2.001 && jump_vec.y.abs_diff_eq(&0.0, 0.001)
        {
            // Use a fast animation time for short jumps less than two characters, typically when
            // typing or holding a key in insert mode
            settings.animation_length.min(0.04)
        } else {
            let leading = settings.animation_length * (1.0 - settings.trail_size).clamp(0.0, 1.0);
            let trailing = settings.animation_length;
            match rank {
                // The leading edge runs faster than the trailing edge, with a trail size of one
                // it jumps to the destination
                2..=3 => leading,
                // One of the corner runs between the trailing corner and the leading edge, creating a triangular effect
                1 => (leading + trailing) / 2.0,
                0 => trailing,
                _ => panic!("Invalid rank"),
            }
        }
    }

    fn get_destination(
        &self,
        destination: PixelPos<f32>,
        cursor_dimensions: GridScale,
    ) -> PixelPos<f32> {
        // Calculate window-space destination for corner
        let relative_scaled_position = self.relative_position * cursor_dimensions;

        destination + relative_scaled_position.to_vector()
    }

    fn calculate_direction_alignment(
        &self,
        cursor_dimensions: GridScale,
        destination: PixelPos<f32>,
    ) -> f32 {
        // Calculate how much a corner will be lagging behind based on how much it's aligned
        // with the direction of motion. Corners in front will move faster than corners in the
        // back
        let relative_scaled_position = self.relative_position * cursor_dimensions;
        let corner_destination = destination + relative_scaled_position.to_vector();
        //(center_destination- corner.current_position).length()
        let corner_direction = self.relative_position.as_vector().normalize().cast();
        let travel_direction = {
            let d = corner_destination - self.current_position;
            d.normalize()
        };
        travel_direction.dot(corner_direction)
    }
}

pub struct CursorRenderer {
    pub corners: Vec<Corner>,
    cursor: Cursor,
    destination: PixelPos<f32>,
    blink_status: BlinkStatus,
    previous_cursor_position: Option<(u64, GridPos<u64>)>,
    previous_cursor_shape: Option<CursorShape>,
    previous_editor_mode: EditorMode,
    cursor_vfxs: Vec<Box<dyn cursor_vfx::CursorVfx>>,
    previous_vfx_mode: cursor_vfx::VfxModeList,
    window_has_focus: bool,
    jumped: bool,

    settings: Arc<Settings>,
}

impl CursorRenderer {
    pub fn new(settings: Arc<Settings>) -> CursorRenderer {
        let mut renderer = CursorRenderer {
            corners: vec![Corner::new(); 4],
            cursor: Cursor::new(),
            destination: (0.0, 0.0).into(),
            blink_status: BlinkStatus::new(),
            previous_cursor_position: None,
            previous_cursor_shape: None,
            previous_editor_mode: EditorMode::Normal,
            cursor_vfxs: vec![],
            previous_vfx_mode: cursor_vfx::VfxModeList::default(),
            window_has_focus: true,
            jumped: false,

            settings,
        };
        renderer.set_cursor_shape(&CursorShape::Block, DEFAULT_CELL_PERCENTAGE);
        renderer
    }

    pub fn handle_event(&mut self, event: &WindowEvent) {
        if let WindowEvent::Focused(is_focused) = event {
            self.window_has_focus = *is_focused;
        }
    }

    pub fn update_cursor(&mut self, new_cursor: Cursor) {
        self.cursor = new_cursor;
    }

    fn set_cursor_shape(&mut self, cursor_shape: &CursorShape, cell_percentage: f32) {
        self.corners = self
            .corners
            .clone()
            .into_iter()
            .enumerate()
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
                        CursorShape::Horizontal => {
                            (x, -((-y + 0.5) * cell_percentage - 0.5)).into()
                        }
                    },
                    #[cfg(feature = "profiling")]
                    id: i,
                    ..corner
                }
            })
            .collect::<Vec<Corner>>();
    }

    pub fn update_cursor_destination(
        &mut self,
        grid_scale: GridScale,
        windows: &HashMap<u64, RenderedWindow>,
    ) {
        let cursor_grid_position = GridPos::<u64>::from(self.cursor.grid_position);
        let cursor_grid_position_f = cursor_grid_position.try_cast().unwrap();
        let new_cursor_pos = if let Some(window) = windows.get(&self.cursor.parent_window_id) {
            let mut grid = cursor_grid_position_f + window.grid_current_position.to_vector();
            grid.y -= window.scroll_animation.position;

            let top_border = window.viewport_margins.top as f32;
            let bottom_border = window.viewport_margins.bottom as f32;

            // Prevent the cursor from targeting a position outside its current window. Since only
            // the vertical direction is effected by scrolling, we only have to clamp the vertical
            // grid position.
            grid.y = grid.y.max(window.grid_current_position.y + top_border).min(
                window.grid_current_position.y + window.grid_size.height as f32
                    - 1.0
                    - bottom_border,
            );

            self.destination = grid * grid_scale;
            Some((window.id, cursor_grid_position))
        } else {
            self.destination = cursor_grid_position_f * grid_scale;
            Some((0, cursor_grid_position))
        };
        if new_cursor_pos != self.previous_cursor_position {
            self.previous_cursor_position = new_cursor_pos;
            self.jumped = true;
            for vfx in self.cursor_vfxs.iter_mut() {
                vfx.cursor_jumped(self.destination);
            }
        }
    }

    pub fn prepare_frame(&mut self) -> ShouldRender {
        self.blink_status.update_status(&self.cursor)
    }

    pub fn draw(&mut self, grid_renderer: &mut GridRenderer, canvas: &Canvas) {
        tracy_zone!("cursor_draw");
        let settings = self.settings.get::<CursorSettings>();
        let render = self.blink_status.should_render() || settings.smooth_blink;
        let opacity = match settings.smooth_blink {
            true => self.blink_status.opacity(),
            false => 1.0,
        };
        let alpha = self.cursor.alpha() as f32;

        let mut paint = Paint::new(skia_safe::colors::WHITE, None);
        paint.set_anti_alias(settings.antialiasing);

        let character = self.cursor.grid_cell.0.clone();

        if !(self.cursor.enabled && render) {
            return;
        }
        // Draw Background
        let background_color = self
            .cursor
            .background(&grid_renderer.default_style.colors)
            .to_color()
            .with_a((opacity * alpha) as u8);
        paint.set_color(background_color);

        let path = if self.window_has_focus || self.cursor.shape != CursorShape::Block {
            self.draw_rectangle(canvas, &paint)
        } else {
            let outline_width = settings.unfocused_outline_width * grid_renderer.em_size;
            self.draw_rectangular_outline(canvas, &paint, outline_width)
        };

        // Draw foreground
        let foreground_color = self
            .cursor
            .foreground(&grid_renderer.default_style.colors)
            .to_color()
            .with_a((opacity * alpha) as u8);
        paint.set_color(foreground_color);

        canvas.save();
        canvas.clip_path(&path, None, Some(false));

        let baseline_offset = grid_renderer.shaper.baseline_offset();
        let style = &self.cursor.grid_cell.1;
        let coarse_style = style.as_ref().map(|style| style.into()).unwrap_or_default();

        let box_char_drawn = grid_renderer.box_char_renderer.draw_glyph(
            &character,
            canvas,
            PixelRect::from_origin_and_size(
                self.destination,
                GridSize::new(1, 1) * grid_renderer.grid_scale,
            ),
            foreground_color,
            PixelPos::default(),
        );
        if !box_char_drawn {
            let pos = (self.destination.x, self.destination.y + baseline_offset);
            let blobs = &grid_renderer.shaper.shape_cached(character, coarse_style);
            for blob in blobs.iter() {
                canvas.draw_text_blob(blob, pos, &paint);
            }
        }

        canvas.restore();

        for vfx in self.cursor_vfxs.iter() {
            vfx.render(&settings, canvas, grid_renderer, &self.cursor);
        }
    }

    pub fn animate(
        &mut self,
        current_mode: &EditorMode,
        grid_renderer: &GridRenderer,
        dt: f32,
    ) -> bool {
        tracy_zone!("cursor_animate");
        if !self.cursor.enabled {
            return false;
        }
        let settings = self.settings.get::<CursorSettings>();

        if settings.vfx_mode != self.previous_vfx_mode {
            self.cursor_vfxs = cursor_vfx::new_cursor_vfxs(&settings.vfx_mode);
            self.previous_vfx_mode = settings.vfx_mode.clone();
        }

        let mut cursor_width = grid_renderer.grid_scale.width();
        if self.cursor.double_width && self.cursor.shape == CursorShape::Block {
            cursor_width *= 2.0;
        }

        let cursor_dimensions = PixelSize::new(cursor_width, grid_renderer.grid_scale.height());

        let in_insert_mode = matches!(current_mode, EditorMode::Insert);

        let changed_to_from_cmdline = !matches!(self.previous_editor_mode, EditorMode::CmdLine)
            ^ matches!(current_mode, EditorMode::CmdLine);

        let center_destination = self.destination + cursor_dimensions.to_vector() * 0.5;

        if self.previous_cursor_shape.as_ref() != Some(&self.cursor.shape) {
            self.previous_cursor_shape = Some(self.cursor.shape.clone());
            self.set_cursor_shape(
                &self.cursor.shape.clone(),
                self.cursor
                    .cell_percentage
                    .unwrap_or(DEFAULT_CELL_PERCENTAGE),
            );

            for vfx in self.cursor_vfxs.iter_mut() {
                vfx.restart(center_destination);
            }
        }

        let mut animating = false;

        if center_destination != PixelPos::ZERO {
            let immediate_movement = !settings.animate_in_insert_mode && in_insert_mode
                || !settings.animate_command_line && !changed_to_from_cmdline;
            if self.jumped {
                // Caclculate the direction alignment for each corner and generate a sorted list
                // This way we know which corner is the front and which is the back
                let corner_ranks = self
                    .corners
                    .iter()
                    .map(|corner| {
                        corner.calculate_direction_alignment(
                            cursor_dimensions.into(),
                            center_destination,
                        )
                    })
                    .enumerate()
                    .sorted_by(|a, b| {
                        a.1.partial_cmp(&b.1)
                            .unwrap_or(std::cmp::Ordering::Equal)
                            .then(a.0.cmp(&b.0))
                    })
                    .enumerate()
                    .sorted_by_key(|(_, (id, _))| *id)
                    .map(|(rank, (_, _))| (rank))
                    .collect_array::<4>()
                    .unwrap();
                for (id, corner) in self.corners.iter_mut().enumerate() {
                    corner.jump(
                        &settings,
                        center_destination,
                        cursor_dimensions.into(),
                        corner_ranks[id],
                    )
                }
            }
            for corner in self.corners.iter_mut() {
                let corner_animating = corner.update(
                    cursor_dimensions.into(),
                    center_destination,
                    dt,
                    immediate_movement,
                );

                animating |= corner_animating;
            }

            let mut vfx_animating = false;

            for vfx in self.cursor_vfxs.iter_mut() {
                let ret = vfx.update(
                    &settings,
                    center_destination,
                    cursor_dimensions,
                    immediate_movement,
                    dt,
                );
                if !vfx_animating {
                    vfx_animating = ret;
                }
            }

            animating |= vfx_animating;
        }
        self.jumped = false;

        let blink_animating = settings.smooth_blink && self.blink_status.should_animate();

        animating |= blink_animating;

        if !animating {
            self.previous_editor_mode = current_mode.clone();
        }
        tracy_plot!("cursor animating", animating as u8 as f64);
        animating
    }

    fn draw_rectangle(&self, canvas: &Canvas, paint: &Paint) -> Path {
        // The cursor is made up of four points, so I create a path with each of the four
        // corners.
        let mut path = Path::new();

        path.move_to(to_skia_point(self.corners[0].current_position.round()));
        path.line_to(to_skia_point(self.corners[1].current_position.round()));
        path.line_to(to_skia_point(self.corners[2].current_position.round()));
        path.line_to(to_skia_point(self.corners[3].current_position.round()));
        path.close();

        canvas.draw_path(&path, paint);
        path
    }

    fn draw_rectangular_outline(&self, canvas: &Canvas, paint: &Paint, outline_width: f32) -> Path {
        let mut rectangle = Path::new();
        rectangle.move_to(to_skia_point(self.corners[0].current_position.round()));
        rectangle.line_to(to_skia_point(self.corners[1].current_position.round()));
        rectangle.line_to(to_skia_point(self.corners[2].current_position.round()));
        rectangle.line_to(to_skia_point(self.corners[3].current_position.round()));
        rectangle.close();

        let offsets: [PixelVec<f32>; 4] = [
            (outline_width, outline_width).into(),
            (-outline_width, outline_width).into(),
            (-outline_width, -outline_width).into(),
            (outline_width, -outline_width).into(),
        ];

        let mut subtract = Path::new();
        subtract.move_to(to_skia_point(self.corners[0].current_position + offsets[0]));
        subtract.line_to(to_skia_point(self.corners[1].current_position + offsets[1]));
        subtract.line_to(to_skia_point(self.corners[2].current_position + offsets[2]));
        subtract.line_to(to_skia_point(self.corners[3].current_position + offsets[3]));
        subtract.close();

        // We have two "rectangles"; create an outline path by subtracting the smaller rectangle
        // from the larger one. This can fail in which case we return a full "rectangle".
        let path = op(&rectangle, &subtract, skia_safe::PathOp::Difference).unwrap_or(rectangle);

        canvas.draw_path(&path, paint);
        path
    }

    pub fn get_destination(&self) -> PixelPos<f32> {
        self.destination
    }
}
