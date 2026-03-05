use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::{
    bridge::NeovimHandler,
    editor::{ScrollbarGrid, ScrollbarSnapshot},
};

use super::geometry::{
    Smooth, buffer_line_offset, knob_proportion, line_count_is_valid, visible_buffer_lines,
};

/// The callback data is shared by the controls for a grid, never by its numeric ID globally.
pub type SharedScrollbarState = Arc<Mutex<Option<ScrollbarState>>>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollbarThumb {
    pub value: f64,
    pub proportion: f64,
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
    line_count: Option<f64>,
    visible_lines: Option<f64>,
    position: Option<f64>,
    last_top_line: Option<f64>,
    animating: bool,
}

impl Drop for ScrollbarGridState {
    fn drop(&mut self) {
        *self.callback.lock().unwrap() = None;
    }
}

impl ScrollbarGridState {
    pub fn new(record: &ScrollbarGrid) -> Self {
        Self {
            window: record.layout.window,
            buffer: record.metadata.map(|metadata| metadata.buffer),
            eligible: record.metadata.is_some_and(|metadata| metadata.eligible),
            viewport: ScrollbarViewport::from_grid(record),
            callback: Arc::new(Mutex::new(None)),
            thumb: None,
            line_count: None,
            visible_lines: None,
            position: None,
            last_top_line: None,
            animating: false,
        }
    }

    pub fn update_viewport(&mut self, record: &ScrollbarGrid) {
        let buffer = record.metadata.map(|metadata| metadata.buffer);
        if self.window != record.layout.window || self.buffer != buffer {
            self.line_count = None;
            self.visible_lines = None;
            self.reset_animation();
            self.thumb = None;
            *self.callback.lock().unwrap() = None;
            // A still-tracking AppKit control must not acquire the replacement buffer's target.
            self.callback = Arc::new(Mutex::new(None));
        }

        self.window = record.layout.window;
        self.buffer = buffer;
        self.eligible = record.metadata.is_some_and(|metadata| metadata.eligible);
        self.viewport = ScrollbarViewport::from_grid(record);
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
            self.reset_animation();
        }

        let state = self.thumb.and_then(|_| {
            let viewport = self.viewport.as_ref()?;
            Some(ScrollbarState {
                neovim_handler: handler.clone(),
                window: self.window,
                buffer: self.buffer?,
                line_count: self.line_count?,
                visible_lines: self.visible_lines?,
                top_line: viewport.top_line,
                current_line: viewport.current_line,
            })
        });

        *self.callback.lock().unwrap() = state;
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
        let visible = visible_buffer_lines(viewport);
        if visible <= 0.0 {
            return None;
        }

        self.visible_lines = Some(visible);
        if let Some(raw) = viewport.line_count
            && line_count_is_valid(viewport, raw)
        {
            self.line_count = Some(raw);
        }

        let line_count = self.line_count.filter(|count| *count > visible)?;
        let max_top = (line_count - visible).max(1.0);
        let scroll_offset = buffer_line_offset(viewport, scroll_offset);
        let target_value = ((viewport.top_line + scroll_offset) / max_top).clamp(0.0, 1.0);
        let mut value = target_value;
        let previous_top = self.last_top_line.unwrap_or(viewport.top_line);
        if (viewport.top_line - previous_top).abs()
            > (visible * Smooth::LINE_FACTOR).max(Smooth::MIN_LINES)
        {
            self.animating = true;
        }

        self.last_top_line = Some(viewport.top_line);
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
        Some(ScrollbarThumb { value, proportion: knob_proportion(visible, line_count) })
    }

    pub fn reset_animation(&mut self) {
        self.position = None;
        self.last_top_line = None;
        self.animating = false;
    }
}

/// Reconciles only document state. the appkit control lifetime is independent of this map.
pub fn sync_grid_states(
    states: &mut HashMap<u64, ScrollbarGridState>,
    snapshot: Option<&ScrollbarSnapshot>,
) {
    let Some(snapshot) = snapshot else {
        states.clear();
        return;
    };

    states.retain(|grid, _| snapshot.grids.iter().any(|record| record.grid == *grid));
    for record in &snapshot.grids {
        states
            .entry(record.grid)
            .and_modify(|state| state.update_viewport(record))
            .or_insert_with(|| ScrollbarGridState::new(record));
    }
}

#[derive(Clone, Debug)]
pub struct ScrollbarState {
    pub neovim_handler: NeovimHandler,
    pub window: u64,
    pub buffer: u64,
    pub line_count: f64,
    /// Estimated visible buffer lines, not the grid's screen-row height.
    pub visible_lines: f64,
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
