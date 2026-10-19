//! Split-aware scrollbars in the outer window padding.
//!
//! [`WindowScrollbars`] owns the component for one OS window. grid ids belong to
//! that owner's session, so matching ids in other sessions don't share state.
//! [`MacosWindowFeature`] passes window, renderer and settings changes into it.
//!
//! grid state, controls and tracks have different jobs. each grid keeps one
//! viewport, scroll range and animation history. controls are keyed by grid and
//! side, so a full-width split can have a control in both lanes without keeping
//! two copies of its scrolling state.
//!
//! tracks belong to lanes rather than buffers. they fill the reserved outer
//! padding behind the controls and remain visible when no thumb is needed.
//! a short buffer therefore doesn't remove the track or return its space to the
//! editor. lane selection depends on the split layout, not the buffer length.
//!
//! [`WindowScrollbars::apply_snapshot`] reads the snapshot committed with the
//! renderer's draw batch. paused redraws keep the controls and text on the same
//! layout. the snapshot updates grid state and lane assignments, with controls
//! created or retired when assignments or window and buffer identities change.
//!
//! geometry has a separate update path. [`WindowScrollbars::set_layout`] converts
//! renderer measurements into appkit points, then updates frames when their
//! inputs change. resizing or changing fonts doesn't require new control owners.
//!
//! between snapshots, [`WindowScrollbars::update_from_renderer`] advances the
//! thumb animation once per grid and updates its existing controls. it doesn't
//! select lanes or rebuild their frames. while a knob is being dragged, appkit
//! owns that control's position and its counterpart can follow the shared value.
//!
//! See https://developer.apple.com/documentation/appkit/nsscroller

mod appearance;
mod appkit;
mod controller;
mod entry;
mod geometry;
mod interaction;
mod layout;
mod metrics;
mod registry;
mod selection;
mod state;

use std::{cell::Cell, collections::HashMap};

use objc2_app_kit::NSWindow;
use skia_safe::Color4f;

use crate::{
    bridge::NeovimHandler, cmd_line::CmdLineSettings, editor::ScrollbarSnapshot,
    renderer::Renderer, settings::Settings, units::PixelSize, window::WindowSettings,
};

pub use self::layout::ScrollbarLayoutInput;
use self::{
    appearance::ScrollbarColors,
    entry::{ScrollbarControlKey, ScrollbarEntry},
    layout::{
        ScrollbarFrameInputs, ScrollbarLaneGeometry, ScrollbarSegment, ScrollbarSide,
        partition_lane,
    },
    selection::{ScrollbarLanes, select_lanes},
    state::{ScrollbarGridState, sync_grid_states},
};
use super::MacosWindowFeature;

/// Owns the scrollbar state and AppKit views for one OS window.
#[derive(Debug, Default)]
pub struct WindowScrollbars {
    // Drop grid state first, invalidating callback targets before controls retire.
    grids: HashMap<u64, ScrollbarGridState>,
    controls: HashMap<ScrollbarControlKey, ScrollbarEntry>,
    tracks: HashMap<ScrollbarSide, ScrollbarEntry>,
    lanes: ScrollbarLanes,
    colors: Option<ScrollbarColors>,
    segments: HashMap<ScrollbarControlKey, ScrollbarSegment>,
    geometry: ScrollbarLaneGeometry,
    frame_inputs: Cell<Option<ScrollbarFrameInputs>>,
}

fn scrollbar_enabled(settings: &Settings) -> bool {
    // single-grid uis have no win_pos layout to assign controls to these lanes
    settings.get::<WindowSettings>().system_scrollbar
        && !settings.get::<CmdLineSettings>().no_multi_grid
}

impl MacosWindowFeature {
    pub fn apply_scrollbar_snapshot(&mut self, renderer: &Renderer) {
        self.scrollbars.apply_snapshot(
            &self.ns_window,
            &self.neovim_handler,
            renderer,
            scrollbar_enabled(&self.settings),
        );
    }

    pub fn scrollbar_left_reserved_width_in_pixels(&self) -> u32 {
        self.scrollbars
            .left_reserved_width_in_pixels(&self.ns_window, scrollbar_enabled(&self.settings))
    }

    pub fn scrollbar_reserved_width_in_pixels(&self) -> u32 {
        self.scrollbars.reserved_width_in_pixels(&self.ns_window, scrollbar_enabled(&self.settings))
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

    pub fn set_scrollbar_layout(&mut self, layout: ScrollbarLayoutInput) {
        self.scrollbars.set_layout(&self.ns_window, layout, scrollbar_enabled(&self.settings));
    }

    pub fn refresh_scrollbar_appearance(&self) {
        self.scrollbars.refresh_appearance(&self.ns_window, scrollbar_enabled(&self.settings));
    }

    pub fn update_scrollbar_from_renderer(&mut self, renderer: &Renderer, dt: f32) {
        self.scrollbars.update_from_renderer(
            &self.neovim_handler,
            renderer,
            scrollbar_enabled(&self.settings),
            dt,
        );
    }

    pub fn update_scrollbar_frames(&self) {
        self.scrollbars.update_frames(&self.ns_window, scrollbar_enabled(&self.settings));
    }
}

impl WindowScrollbars {
    /// Applies document state separately from control ownership and lane reservation.
    pub fn apply_snapshot(
        &mut self,
        window: &NSWindow,
        handler: &NeovimHandler,
        renderer: &Renderer,
        enabled: bool,
    ) {
        let ownership_changed = self.sync_snapshot(renderer.scrollbar_snapshot.as_ref(), enabled);
        let colors = &renderer.grid_renderer.default_style.colors;
        self.set_colors(colors.foreground, colors.background);
        self.update_states(handler, None, enabled, None);
        if ownership_changed {
            self.reconcile_controls(window, enabled);
        }
        self.update_thumbs();
    }

    fn sync_snapshot(&mut self, snapshot: Option<&ScrollbarSnapshot>, enabled: bool) -> bool {
        let lanes = select_lanes(enabled, snapshot);
        let mut segments = HashMap::new();
        if let Some(snapshot) = snapshot {
            for (side, selected) in
                [(ScrollbarSide::Left, &lanes.left), (ScrollbarSide::Right, &lanes.right)]
            {
                if let Some(selected) = selected {
                    segments.extend(partition_lane(selected, &snapshot.grids).into_iter().map(
                        |segment| (ScrollbarControlKey { grid: segment.grid, side }, segment),
                    ));
                }
            }
        }

        let layout_changed = self.lanes != lanes || self.segments != segments;
        self.lanes = lanes;
        self.segments = segments;
        let targets_changed = sync_grid_states(&mut self.grids, snapshot);
        layout_changed || targets_changed
    }

    fn set_colors(&mut self, foreground: Option<Color4f>, background: Option<Color4f>) {
        let colors = ScrollbarColors::from_default_colors(foreground, background);
        if self.colors == colors {
            return;
        }
        self.colors = colors;
        for entry in self.controls.values().chain(self.tracks.values()) {
            entry.set_colors(colors);
        }
    }

    pub fn left_reserved_width_in_pixels(&self, window: &NSWindow, enabled: bool) -> u32 {
        if self.lanes.left.is_some() { self.reserved_width_in_pixels(window, enabled) } else { 0 }
    }

    pub fn reserved_width_in_pixels(&self, window: &NSWindow, enabled: bool) -> u32 {
        if !enabled {
            return 0;
        }
        self.tracks.get(&ScrollbarSide::Right).map_or(0, |track| {
            (track.track_metrics().width * window.backingScaleFactor()).ceil() as u32
        })
    }

    pub fn set_layout(&mut self, window: &NSWindow, layout: ScrollbarLayoutInput, enabled: bool) {
        self.geometry = ScrollbarLaneGeometry::from_layout(layout, window.backingScaleFactor());
        self.update_frames(window, enabled);
    }

    pub fn refresh_appearance(&self, window: &NSWindow, enabled: bool) {
        for entry in self.controls.values().chain(self.tracks.values()) {
            entry.refresh_appearance(window);
        }
        self.frame_inputs.set(None);
        self.update_frames(window, enabled);
    }

    pub fn update_from_renderer(
        &mut self,
        handler: &NeovimHandler,
        renderer: &Renderer,
        enabled: bool,
        dt: f32,
    ) {
        if !enabled {
            return;
        }
        self.update_states(handler, Some(renderer), enabled, Some(dt));
        self.update_thumbs();
    }

    fn update_states(
        &mut self,
        handler: &NeovimHandler,
        renderer: Option<&Renderer>,
        enabled: bool,
        dt: Option<f32>,
    ) {
        for (&grid, state) in &mut self.grids {
            let offset = renderer
                .and_then(|renderer| renderer.rendered_windows.get(&grid))
                .map_or(0.0, |window| window.scroll_animation.position as f64);

            let dragged_value = self
                .controls
                .iter()
                .filter(|(key, entry)| {
                    key.grid == grid
                        && entry.window == Some(state.window)
                        && entry.buffer == state.buffer
                })
                .find_map(|(_, entry)| entry.dragged_value());

            // advance once per grid, not once for each control showing that grid.
            state.update(handler, enabled, offset, dt, dragged_value);
        }
    }

    fn reconcile_controls(&mut self, window: &NSWindow, enabled: bool) {
        self.refresh_tracks(window, enabled);
        self.controls.retain(|control_key, entry| {
            self.segments.contains_key(control_key)
                && self.grids.get(&control_key.grid).is_some_and(|state| {
                    entry.window == Some(state.window) && entry.buffer == state.buffer
                })
        });

        for &key in self.segments.keys() {
            if let Some(state) = self.grids.get(&key.grid)
                && let Some(track) = self.tracks.get(&key.side)
            {
                self.controls.entry(key).or_insert_with(|| {
                    ScrollbarEntry::control(window, key.side, state, self.colors, track)
                });
            }
        }

        // New controls need frames even when the lane geometry is unchanged.
        self.frame_inputs.set(None);
        self.update_frames(window, enabled);
    }

    fn update_thumbs(&self) {
        for (key, entry) in &self.controls {
            entry.set_thumb(self.grids.get(&key.grid).and_then(|state| state.thumb));
        }
    }

    fn refresh_tracks(&mut self, window: &NSWindow, enabled: bool) {
        let right = enabled && self.lanes.right.is_some();
        let left = enabled && self.lanes.left.is_some();

        self.tracks.retain(|side, _| match side {
            ScrollbarSide::Left => left,
            ScrollbarSide::Right => right,
        });

        for (side, visible) in [(ScrollbarSide::Left, left), (ScrollbarSide::Right, right)] {
            if visible {
                self.tracks
                    .entry(side)
                    .or_insert_with(|| ScrollbarEntry::track(window, side, self.colors));
            }
        }
    }

    pub fn update_frames(&self, window: &NSWindow, enabled: bool) {
        let Some(content_view) = window.contentView() else {
            return;
        };

        let Some(track) = self.tracks.get(&ScrollbarSide::Right) else {
            return;
        };

        let bounds = content_view.bounds();
        let scale_factor = window.backingScaleFactor();
        let metrics = track.track_metrics();
        let inputs = ScrollbarFrameInputs {
            geometry: self.geometry,
            bounds,
            scale_factor,
            track_metrics: metrics,
        };

        if self.frame_inputs.replace(Some(inputs)) == Some(inputs) {
            return;
        }

        let width = if scale_factor > 0.0 {
            f64::from(self.reserved_width_in_pixels(window, enabled)) / scale_factor
        } else {
            metrics.width
        };

        for (key, entry) in &self.controls {
            let lane = self.geometry.frame(key.side, bounds, width, metrics.trailing_inset);
            let frame = self
                .segments
                .get(key)
                .and_then(|segment| segment.frame(lane, self.geometry.cell_height));

            if let Some(frame) = frame {
                entry.set_frame(window, frame);
            }

            entry.set_visible(frame.is_some());
        }

        for (&side, entry) in &self.tracks {
            let frame = self.geometry.frame(side, bounds, width, metrics.trailing_inset);
            entry.set_frame(window, frame);
            entry.set_visible(frame.size.width > 0.0 && frame.size.height > 0.0);
        }
    }
}
