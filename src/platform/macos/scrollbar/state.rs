use std::{
    collections::{HashMap, hash_map::Entry},
    sync::{Arc, Mutex},
};

use crate::{
    bridge::NeovimHandler,
    editor::{ScrollbarGrid, ScrollbarSnapshot},
};

use super::{
    geometry::{Smooth, buffer_line_offset},
    metrics::ScrollMetrics,
};

/// Callback data shared by a grid's controls and retained by an active drag.
///
/// Clones refer to the same allocation. Replacing the window or buffer requires
/// invalidating the old handle and creating a new one, not publishing into it.
#[derive(Clone, Debug, Default)]
pub struct SharedScrollbarState {
    inner: Arc<Mutex<Option<ScrollbarState>>>,
}

impl SharedScrollbarState {
    /// Reads the current callback data without retaining the lock.
    pub fn snapshot(&self) -> Option<ScrollbarState> {
        self.inner.lock().unwrap().clone()
    }

    /// Publishes the latest viewport for this handle's existing window and buffer.
    pub fn publish(&self, state: ScrollbarState) {
        *self.inner.lock().unwrap() = Some(state);
    }

    /// Makes callback data unavailable to every holder of this handle.
    pub fn invalidate(&self) {
        *self.inner.lock().unwrap() = None;
    }

    /// Compares allocation identity, regardless of the currently published data.
    pub fn same_instance(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    /// Advances the pending view under one lock, without reviving invalidated data.
    pub fn update_pending_scroll(&self, target_top: f64) {
        let mut guard = self.inner.lock().unwrap();
        let Some(state) = guard.as_mut() else {
            return;
        };

        let target_top = state.metrics.clamp_top_line(target_top);
        let previous_top = state.metrics.clamp_top_line(state.top_line);
        let delta = target_top - previous_top;

        state.top_line = target_top;

        let max_current_line = (state.metrics.line_count() - 1.0).max(0.0);
        state.current_line = (state.current_line + delta).clamp(0.0, max_current_line);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollbarThumb {
    pub value: f64,
    pub proportion: f64,
}

/// Position history for one grid's thumb, independent of document-length caching.
#[derive(Debug, Default, PartialEq)]
pub struct ThumbAnimation {
    position: Option<f64>,
    last_top_line: Option<f64>,
    animating: bool,
}

impl ThumbAnimation {
    pub fn advance(
        &mut self,
        metrics: ScrollMetrics,
        top_line: f64,
        scroll_offset: f64,
        dt: Option<f32>,
        dragged_value: Option<f64>,
    ) -> f64 {
        let target_value = metrics.value_for_top_line(top_line + scroll_offset);
        let mut value = target_value;
        let previous_top = self.last_top_line.unwrap_or(top_line);
        if (top_line - previous_top).abs()
            > (metrics.visible_lines() * Smooth::LINE_FACTOR).max(Smooth::MIN_LINES)
        {
            self.animating = true;
        }

        self.last_top_line = Some(top_line);
        if self.animating {
            if let Some(previous) = self.position {
                let dt = dt.unwrap_or(0.0) as f64;
                let alpha = if dt > 0.0 { 1.0 - (-dt / Smooth::TAU).exp() } else { 1.0 };
                value = previous + (target_value - previous) * alpha;
                if (target_value - value).abs() <= Smooth::THRESHOLD {
                    value = target_value;
                    self.animating = false;
                }
            } else {
                self.animating = false;
            }
        }

        if let Some(dragged_value) = dragged_value.filter(|value| value.is_finite()) {
            value = dragged_value.clamp(0.0, 1.0);
            self.animating = false;
        }

        self.position = Some(value);
        value
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// The viewport and smoothing history belonging to one grid in one Neovim session.
#[derive(Debug)]
pub struct ScrollbarGridState {
    pub window: u64,
    pub buffer: Option<u64>,
    pub eligible: bool,
    pub viewport: Option<ScrollbarViewport>,
    pub callback: SharedScrollbarState,
    pub thumb: Option<ScrollbarThumb>,
    metrics: Option<ScrollMetrics>,
    animation: ThumbAnimation,
}

impl Drop for ScrollbarGridState {
    fn drop(&mut self) {
        self.callback.invalidate();
    }
}

impl ScrollbarGridState {
    pub fn new(record: &ScrollbarGrid) -> Self {
        Self {
            window: record.layout.window,
            buffer: record.metadata.map(|metadata| metadata.buffer),
            eligible: record.metadata.is_some_and(|metadata| metadata.eligible),
            viewport: ScrollbarViewport::from_grid(record),
            callback: SharedScrollbarState::default(),
            thumb: None,
            metrics: None,
            animation: ThumbAnimation::default(),
        }
    }

    pub fn update_viewport(&mut self, record: &ScrollbarGrid) -> bool {
        let buffer = record.metadata.map(|metadata| metadata.buffer);
        let target_changed = self.window != record.layout.window || self.buffer != buffer;
        if target_changed {
            self.metrics = None;
            self.animation.reset();
            self.thumb = None;
            self.callback.invalidate();
            // A still-tracking AppKit control must not acquire the replacement buffer's target.
            self.callback = SharedScrollbarState::default();
        }

        self.window = record.layout.window;
        self.buffer = buffer;
        self.eligible = record.metadata.is_some_and(|metadata| metadata.eligible);
        self.viewport = ScrollbarViewport::from_grid(record);
        target_changed
    }

    pub fn update(
        &mut self,
        handler: &NeovimHandler,
        enabled: bool,
        scroll_offset: f64,
        dt: Option<f32>,
        dragged_value: Option<f64>,
    ) {
        self.thumb = if enabled { self.sample(scroll_offset, dt, dragged_value) } else { None };
        if self.thumb.is_none() {
            self.animation.reset();
        }

        let state = self.thumb.and_then(|_| {
            let viewport = self.viewport.as_ref()?;
            Some(ScrollbarState {
                neovim_handler: handler.clone(),
                window: self.window,
                buffer: self.buffer?,
                metrics: self.metrics?,
                top_line: viewport.top_line,
                current_line: viewport.current_line,
            })
        });

        if let Some(state) = state {
            self.callback.publish(state);
        } else {
            self.callback.invalidate();
        }
    }

    fn sample(
        &mut self,
        scroll_offset: f64,
        dt: Option<f32>,
        dragged_value: Option<f64>,
    ) -> Option<ScrollbarThumb> {
        if !self.eligible {
            return None;
        }

        let viewport = self.viewport.as_ref()?;
        let metrics = ScrollMetrics::from_viewport(viewport, self.metrics)?;
        self.metrics = Some(metrics);
        if !metrics.is_scrollable() {
            return None;
        }

        let scroll_offset = buffer_line_offset(viewport, scroll_offset);
        let value =
            self.animation.advance(metrics, viewport.top_line, scroll_offset, dt, dragged_value);

        Some(ScrollbarThumb { value, proportion: metrics.knob_proportion() })
    }
}

/// Reconciles only document state. the appkit control lifetime is independent of this map.
/// Returns whether grids were added, removed, or assigned a different target.
pub fn sync_grid_states(
    states: &mut HashMap<u64, ScrollbarGridState>,
    snapshot: Option<&ScrollbarSnapshot>,
) -> bool {
    let Some(snapshot) = snapshot else {
        let changed = !states.is_empty();
        states.clear();
        return changed;
    };

    let previous_count = states.len();
    states.retain(|grid, _| snapshot.grids.iter().any(|record| record.grid == *grid));
    let mut changed = states.len() != previous_count;
    for record in &snapshot.grids {
        match states.entry(record.grid) {
            Entry::Occupied(mut entry) => changed |= entry.get_mut().update_viewport(record),
            Entry::Vacant(entry) => {
                entry.insert(ScrollbarGridState::new(record));
                changed = true;
            }
        }
    }
    changed
}

#[derive(Clone, Debug)]
pub struct ScrollbarState {
    pub neovim_handler: NeovimHandler,
    pub window: u64,
    pub buffer: u64,
    pub metrics: ScrollMetrics,
    pub top_line: f64,
    pub current_line: f64,
}

#[derive(Clone, Debug)]
pub struct ScrollbarViewport {
    pub top_line: f64,
    pub bottom_line: f64,
    pub current_line: f64,
    pub line_count: Option<f64>,
    pub grid_height: u64,
    pub top_margin: u64,
    pub bottom_margin: u64,
}

impl ScrollbarViewport {
    pub fn from_grid(record: &ScrollbarGrid) -> Option<Self> {
        let viewport = record.viewport?;
        Some(Self {
            top_line: viewport.top_line,
            bottom_line: viewport.bottom_line,
            current_line: viewport.current_line,
            line_count: viewport.line_count,
            grid_height: record.grid_size.height,
            top_margin: record.top_margin,
            bottom_margin: record.bottom_margin,
        })
    }
}
