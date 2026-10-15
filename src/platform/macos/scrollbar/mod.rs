mod appearance;
mod appkit;
mod controller;
mod entry;
mod geometry;
mod interaction;
mod layout;
mod registry;
mod selection;
mod state;

use skia_safe::Color4f;

use crate::{
    cmd_line::CmdLineSettings, renderer::Renderer, settings::Settings, units::PixelSize,
    window::WindowSettings,
};

pub use self::appearance::ScrollbarColors;
pub use self::entry::{ScrollbarControlKey, ScrollbarEntry};
pub use self::layout::{ScrollbarLaneGeometry, ScrollbarSegment, ScrollbarSide};
use self::layout::{partition_lane, scrollbar_width};
pub use self::selection::ScrollbarLanes;
use self::selection::select_lanes;
pub use self::state::ScrollbarGridState;
use self::state::sync_grid_states;
use super::MacosWindowFeature;

fn scrollbar_enabled(settings: &Settings) -> bool {
    // single-grid uis have no win_pos layout to assign controls to these lanes
    settings.get::<WindowSettings>().system_scrollbar
        && !settings.get::<CmdLineSettings>().no_multi_grid
}

impl MacosWindowFeature {
    /// applies the document state separately from control ownership and lane reservation.
    pub fn apply_scrollbar_snapshot(&mut self, renderer: &Renderer) {
        let snapshot = renderer.scrollbar_snapshot.as_ref();
        self.scrollbar_lanes = select_lanes(scrollbar_enabled(&self.settings), snapshot);
        self.scrollbar_segments.clear();
        if let Some(snapshot) = snapshot {
            for (side, selected) in [
                (ScrollbarSide::Left, &self.scrollbar_lanes.left),
                (ScrollbarSide::Right, &self.scrollbar_lanes.right),
            ] {
                if let Some(selected) = selected {
                    self.scrollbar_segments.extend(
                        partition_lane(selected, &snapshot.grids).into_iter().map(|segment| {
                            (ScrollbarControlKey { grid: segment.grid, side }, segment)
                        }),
                    );
                }
            }
        }

        sync_grid_states(&mut self.scrollbar_grid_states, snapshot);

        let colors = &renderer.grid_renderer.default_style.colors;
        self.set_scrollbar_colors(colors.foreground, colors.background);
        self.update_scrollbar_states(None, None);
        self.refresh_scrollbar_controls();
    }

    pub fn set_scrollbar_colors(
        &mut self,
        foreground: Option<Color4f>,
        background: Option<Color4f>,
    ) {
        let colors = ScrollbarColors::from_default_colors(foreground, background);
        if self.scrollbar_colors == colors {
            return;
        }
        self.scrollbar_colors = colors;
        for entry in self.scrollbar_controls.values().chain(self.scrollbar_tracks.values()) {
            entry.set_colors(colors);
        }
    }

    pub fn scrollbar_left_reserved_width_in_pixels(&self) -> u32 {
        if self.scrollbar_lanes.left.is_some() {
            self.scrollbar_reserved_width_in_pixels()
        } else {
            0
        }
    }

    pub fn scrollbar_reserved_width_in_pixels(&self) -> u32 {
        if !scrollbar_enabled(&self.settings) {
            return 0;
        }
        (scrollbar_width() * self.ns_window.backingScaleFactor()).ceil() as u32
    }

    pub fn content_view_size_in_pixels(&self) -> Option<PixelSize<u32>> {
        let bounds = self.ns_window.contentRectForFrameRect(self.ns_window.frame());
        let scale_factor = self.ns_window.backingScaleFactor();
        if scale_factor <= 0.0 {
            return None;
        }
        Some(PixelSize::new(
            (bounds.size.width * scale_factor).round().max(0.0) as u32,
            (bounds.size.height * scale_factor).round().max(0.0) as u32,
        ))
    }

    pub fn set_scrollbar_geometry_in_pixels(
        &mut self,
        edge_x: f64,
        top: f64,
        height: f64,
        left_width: f64,
        cell_height: f64,
    ) {
        self.scrollbar_geometry = ScrollbarLaneGeometry::from_pixels(
            self.ns_window.backingScaleFactor(),
            edge_x,
            top,
            height,
            left_width,
            cell_height,
        );
        self.refresh_scrollbar_tracks();
        self.update_scrollbar_frames();
    }

    pub fn refresh_scrollbar_appearance(&self) {
        for entry in self.scrollbar_controls.values().chain(self.scrollbar_tracks.values()) {
            entry.refresh_appearance(&self.ns_window);
        }
    }

    pub fn update_scrollbar_from_renderer(&mut self, renderer: &Renderer, dt: f32) {
        if !scrollbar_enabled(&self.settings) {
            return;
        }
        self.update_scrollbar_states(Some(renderer), Some(dt));
        self.refresh_scrollbar_controls();
    }

    fn update_scrollbar_states(&mut self, renderer: Option<&Renderer>, dt: Option<f32>) {
        let enabled = scrollbar_enabled(&self.settings);
        for (&grid, state) in &mut self.scrollbar_grid_states {
            let offset = renderer
                .and_then(|renderer| renderer.rendered_windows.get(&grid))
                .map_or(0.0, |window| window.scroll_animation.position as f64);

            let dragged_value = self
                .scrollbar_controls
                .iter()
                .filter(|(key, entry)| {
                    key.grid == grid
                        && entry.window == Some(state.window)
                        && entry.buffer == state.buffer
                })
                .find_map(|(_, entry)| entry.dragged_value());

            // advance once per grid, not once for each control showing that grid.
            state.update(&self.neovim_handler, enabled, offset, dt, dragged_value);
        }
    }

    fn refresh_scrollbar_controls(&mut self) {
        self.refresh_scrollbar_tracks();
        self.scrollbar_controls.retain(|control_key, entry| {
            self.scrollbar_segments.contains_key(control_key)
                && self.scrollbar_grid_states.get(&control_key.grid).is_some_and(|state| {
                    entry.window == Some(state.window) && entry.buffer == state.buffer
                })
        });

        for &key in self.scrollbar_segments.keys() {
            if let Some(state) = self.scrollbar_grid_states.get(&key.grid)
                && let Some(track) = self.scrollbar_tracks.get(&key.side)
            {
                self.scrollbar_controls.entry(key).or_insert_with(|| {
                    ScrollbarEntry::control(
                        &self.ns_window,
                        key.side,
                        state,
                        self.scrollbar_colors,
                        track,
                    )
                });
            }
        }

        self.update_scrollbar_frames();

        for (key, entry) in &self.scrollbar_controls {
            entry
                .set_thumb(self.scrollbar_grid_states.get(&key.grid).and_then(|state| state.thumb));
        }
    }

    fn refresh_scrollbar_tracks(&mut self) {
        let enabled = scrollbar_enabled(&self.settings);
        let right = enabled && self.scrollbar_lanes.right.is_some();
        let left = enabled && self.scrollbar_lanes.left.is_some();

        self.scrollbar_tracks.retain(|side, _| match side {
            ScrollbarSide::Left => left,
            ScrollbarSide::Right => right,
        });

        for (side, visible) in [(ScrollbarSide::Left, left), (ScrollbarSide::Right, right)] {
            if visible {
                self.scrollbar_tracks.entry(side).or_insert_with(|| {
                    ScrollbarEntry::track(&self.ns_window, side, self.scrollbar_colors)
                });
            }
        }
    }

    pub fn update_scrollbar_frames(&self) {
        let Some(content_view) = self.ns_window.contentView() else {
            return;
        };

        let bounds = content_view.bounds();
        let scale_factor = self.ns_window.backingScaleFactor();
        let width = if scale_factor > 0.0 {
            f64::from(self.scrollbar_reserved_width_in_pixels()) / scale_factor
        } else {
            scrollbar_width()
        };

        for (key, entry) in &self.scrollbar_controls {
            let lane = self.scrollbar_geometry.frame(key.side, bounds, width);
            let frame = self
                .scrollbar_segments
                .get(key)
                .and_then(|segment| segment.frame(lane, self.scrollbar_geometry.cell_height));

            if let Some(frame) = frame {
                entry.set_frame(&self.ns_window, frame);
            }

            entry.set_visible(frame.is_some());
        }

        for (&side, entry) in &self.scrollbar_tracks {
            let frame = self.scrollbar_geometry.frame(side, bounds, width);
            entry.set_frame(&self.ns_window, frame);
            entry.set_visible(frame.size.width > 0.0 && frame.size.height > 0.0);
        }
    }
}
