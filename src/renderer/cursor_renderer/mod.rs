mod blink;
mod cursor_vfx;

use std::collections::HashMap;

use itertools::Itertools;
use palette::{Srgba, WithAlpha};
use vide::{parley::style::StyleProperty, Layer, Path, PathCommand, Scene};
use winit::event::WindowEvent;

use crate::{
    bridge::EditorMode,
    editor::{Cursor, CursorShape},
    profiling::{tracy_plot, tracy_zone},
    renderer::{animation_utils::*, GridRenderer, RenderedWindow},
    settings::{ParseFromValue, SETTINGS},
    units::{GridPos, GridScale, PixelPos, PixelSize},
    window::ShouldRender,
};

use blink::*;

const DEFAULT_CELL_PERCENTAGE: f32 = 1.0 / 8.0;

const STANDARD_CORNERS: &[(f32, f32); 4] = &[(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)];

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

    vfx_mode: cursor_vfx::VfxMode,
    vfx_opacity: f32,
    vfx_particle_lifetime: f32,
    vfx_particle_density: f32,
    vfx_particle_speed: f32,
    vfx_particle_phase: f32,
    vfx_particle_curl: f32,
}

impl Default for CursorSettings {
    fn default() -> Self {
        CursorSettings {
            antialiasing: true,
            animation_length: 0.06,
            distance_length_adjust: true,
            animate_in_insert_mode: true,
            animate_command_line: true,
            trail_size: 0.7,
            unfocused_outline_width: 1.0 / 8.0,
            smooth_blink: false,
            vfx_mode: cursor_vfx::VfxMode::Disabled,
            vfx_opacity: 200.0,
            vfx_particle_lifetime: 1.2,
            vfx_particle_density: 7.0,
            vfx_particle_speed: 10.0,
            vfx_particle_phase: 1.5,
            vfx_particle_curl: 1.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Corner {
    start_position: PixelPos<f32>,
    current_position: PixelPos<f32>,
    relative_position: GridPos<f32>,
    previous_destination: PixelPos<f32>,
    length_multiplier: f32,
    t: f32,
}

impl Corner {
    pub fn new() -> Corner {
        Corner {
            start_position: PixelPos::default(),
            current_position: PixelPos::default(),
            relative_position: GridPos::<f32>::default(),
            previous_destination: PixelPos::new(-1000.0, -1000.0),
            length_multiplier: 1.0,
            t: 0.0,
        }
    }

    pub fn update(
        &mut self,
        settings: &CursorSettings,
        cursor_dimensions: GridScale,
        destination: PixelPos<f32>,
        dt: f32,
        immediate_movement: bool,
    ) -> bool {
        if destination != self.previous_destination {
            self.t = 0.0;
            self.start_position = self.current_position;
            self.previous_destination = destination;
            self.length_multiplier = if settings.distance_length_adjust {
                (destination - self.current_position)
                    .length()
                    .log10()
                    .max(0.0)
            } else {
                1.0
            }
        }

        // Check first if animation's over
        if (self.t - 1.0).abs() < f32::EPSILON {
            return false;
        }

        // Calculate window-space destination for corner
        let relative_scaled_position = self.relative_position * cursor_dimensions;

        let corner_destination = destination + relative_scaled_position.to_vector();

        if immediate_movement {
            self.t = 1.0;
            self.current_position = corner_destination;
            return true;
        }

        // Calculate how much a corner will be lagging behind based on how much it's aligned
        // with the direction of motion. Corners in front will move faster than corners in the
        // back
        let travel_direction = {
            let d = destination - self.current_position;
            d.normalize()
        };

        let corner_direction = self.relative_position.as_vector().normalize().cast();

        let direction_alignment = travel_direction.dot(corner_direction);

        if (self.t - 1.0).abs() < f32::EPSILON {
            // We are at destination, move t out of 0-1 range to stop the animation
            self.t = 2.0;
        } else {
            let corner_dt = dt
                * lerp(
                    1.0,
                    (1.0 - settings.trail_size).clamp(0.0, 1.0),
                    -direction_alignment,
                );
            self.t =
                (self.t + corner_dt / (settings.animation_length * self.length_multiplier)).min(1.0)
        }

        self.current_position = ease_point(
            ease_out_expo,
            self.start_position,
            corner_destination,
            self.t,
        );

        true
    }
}

pub struct CursorRenderer {
    pub corners: Vec<Corner>,
    cursor: Cursor,
    destination: PixelPos<f32>,
    blink_status: BlinkStatus,
    previous_cursor_shape: Option<CursorShape>,
    previous_editor_mode: EditorMode,
    cursor_vfx: Option<Box<dyn cursor_vfx::CursorVfx>>,
    previous_vfx_mode: cursor_vfx::VfxMode,
    window_has_focus: bool,
}

impl CursorRenderer {
    pub fn new() -> CursorRenderer {
        let mut renderer = CursorRenderer {
            corners: vec![Corner::new(); 4],
            cursor: Cursor::new(),
            destination: (0.0, 0.0).into(),
            blink_status: BlinkStatus::new(),
            previous_cursor_shape: None,
            previous_editor_mode: EditorMode::Normal,
            cursor_vfx: None,
            previous_vfx_mode: cursor_vfx::VfxMode::Disabled,
            window_has_focus: true,
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
                    t: 0.0,
                    start_position: corner.current_position,
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
        let cursor_grid_position = GridPos::<u64>::from(self.cursor.grid_position)
            .try_cast()
            .unwrap();
        if let Some(window) = windows.get(&self.cursor.parent_window_id) {
            let mut grid = cursor_grid_position + window.grid_current_position.to_vector();
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
        } else {
            self.destination = cursor_grid_position * grid_scale;
        }
    }

    pub fn prepare_frame(&mut self) -> ShouldRender {
        self.blink_status.update_status(&self.cursor)
    }

    pub fn draw(&mut self, grid_renderer: &mut GridRenderer, scene: &mut Scene) {
        tracy_zone!("cursor_draw");
        let settings = SETTINGS.get::<CursorSettings>();
        let render = self.blink_status.should_render() || settings.smooth_blink;
        let opacity = match settings.smooth_blink {
            true => self.blink_status.opacity(),
            false => 1.0,
        };
        let alpha = self.cursor.alpha() as f32;

        let character = self.cursor.grid_cell.0.clone();

        if !(self.cursor.enabled && render) {
            return;
        }

        let outline_width = settings.unfocused_outline_width * grid_renderer.em_size;

        // Draw Background
        let background_color = self
            .cursor
            .background(&grid_renderer.default_style.colors)
            .with_alpha(opacity * alpha);
        let layer = Layer::new().with_path(self.draw_cursor_path(&background_color, outline_width));
        scene.add_layer(layer);

        // Draw foreground
        let mask_color = Srgba::new(1.0, 0.0, 0.0, 1.0);
        let mask_layer = Layer::new().with_path(self.draw_cursor_path(&mask_color, outline_width));

        let mut layer = Layer::new().with_mask(mask_layer.clone());

        let foreground_color = self
            .cursor
            .foreground(&grid_renderer.default_style.colors)
            .with_alpha(opacity * alpha);

        let baseline_offset = grid_renderer.shaper.baseline_offset();
        let style = self
            .cursor
            .grid_cell
            .1
            .as_ref()
            .map(|style| style)
            .unwrap_or(&grid_renderer.default_style);

        let layout = grid_renderer
            .shaper
            .layout_with(&character, &style.into(), |builder| {
                builder.push_default(&StyleProperty::Brush(foreground_color));
            });

        layer.add_text_layout(
            &mut scene.resources,
            layout,
            (self.destination.x, self.destination.y + baseline_offset).into(),
        );
        scene.add_layer(layer);
    }

    pub fn animate(
        &mut self,
        current_mode: &EditorMode,
        grid_renderer: &GridRenderer,
        dt: f32,
    ) -> bool {
        tracy_zone!("cursor_animate");
        let settings = SETTINGS.get::<CursorSettings>();

        if settings.vfx_mode != self.previous_vfx_mode {
            self.cursor_vfx = cursor_vfx::new_cursor_vfx(&settings.vfx_mode);
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

            if let Some(vfx) = self.cursor_vfx.as_mut() {
                vfx.restart(center_destination);
            }
        }

        let mut animating = false;

        if center_destination != PixelPos::ZERO {
            let immediate_movement = !settings.animate_in_insert_mode && in_insert_mode
                || !settings.animate_command_line && !changed_to_from_cmdline;
            for corner in self.corners.iter_mut() {
                let corner_animating = corner.update(
                    &settings,
                    GridScale::new(cursor_dimensions),
                    center_destination,
                    dt,
                    immediate_movement,
                );

                animating |= corner_animating;
            }

            let vfx_animating = if let Some(vfx) = self.cursor_vfx.as_mut() {
                vfx.update(
                    &settings,
                    center_destination,
                    cursor_dimensions,
                    immediate_movement,
                    dt,
                )
            } else {
                false
            };

            animating |= vfx_animating;
        }

        let blink_animating = settings.smooth_blink && self.blink_status.should_animate();

        animating |= blink_animating;

        if !animating {
            self.previous_editor_mode = current_mode.clone();
        }
        tracy_plot!("cursor animating", animating as u8 as f64);
        animating
    }

    fn draw_cursor_path(&self, color: &Srgba, outline_width: f32) -> Path {
        if self.window_has_focus || self.cursor.shape != CursorShape::Block {
            self.draw_rectangle(color)
        } else {
            self.draw_rectangular_outline(color, outline_width)
        }
    }

    fn draw_rectangle(&self, color: &Srgba) -> Path {
        let mut path = Path::new_fill(*color, *self.corners[0].current_position.as_untyped());
        path.commands = self.corners[1..]
            .iter()
            .map(|corner| PathCommand::LineTo {
                to: *corner.current_position.as_untyped(),
            })
            .collect_vec();
        path
    }

    fn draw_rectangular_outline(&self, color: &Srgba, outline_width: f32) -> Path {
        let mut path = Path::new_stroke(
            outline_width,
            *color,
            *self.corners[0].current_position.as_untyped(),
        );
        path.commands = self.corners[1..]
            .iter()
            .map(|corner| PathCommand::LineTo {
                to: *corner.current_position.as_untyped(),
            })
            .collect_vec();
        path
    }

    pub fn get_destination(&self) -> PixelPos<f32> {
        self.destination
    }
}
