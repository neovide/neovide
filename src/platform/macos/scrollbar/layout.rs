use objc2::{class, msg_send};
use objc2_foundation::{NSPoint, NSRect, NSSize};

use crate::editor::ScrollbarGrid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScrollbarSide {
    Left,
    Right,
}

/// One control interval in the last completed grid layout.
///
/// The first interval starts at the lane top. an open end reaches its bottom.
/// boundaries use the next window starting row, giving the preceding control
/// its statusline space without depending on viewport margins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollbarSegment {
    pub grid: u64,
    pub start_row: u64,
    pub end_row: Option<u64>,
}

pub fn partition_lane(selected: &[u64], grids: &[ScrollbarGrid]) -> Vec<ScrollbarSegment> {
    let mut occupants: Vec<_> = grids.iter().filter(|grid| selected.contains(&grid.grid)).collect();
    occupants.sort_unstable_by_key(|grid| (grid.layout.rect.min.y, grid.grid));
    // degenerate layouts must not produce two overlapping controls at one row.
    occupants.dedup_by_key(|grid| grid.layout.rect.min.y);
    occupants
        .iter()
        .enumerate()
        .map(|(index, grid)| ScrollbarSegment {
            grid: grid.grid,
            start_row: if index == 0 { 0 } else { grid.layout.rect.min.y },
            end_row: occupants.get(index + 1).map(|next| next.layout.rect.min.y),
        })
        .collect()
}

impl ScrollbarSegment {
    pub fn frame(&self, lane: NSRect, cell_height: f64) -> Option<NSRect> {
        if !cell_height.is_finite()
            || cell_height <= 0.0
            || lane.size.width <= 0.0
            || lane.size.height <= 0.0
        {
            return None;
        }

        let bottom = lane.origin.y + lane.size.height;
        let top = (lane.origin.y + self.start_row as f64 * cell_height).min(bottom);
        let end = self
            .end_row
            .map_or(bottom, |row| (lane.origin.y + row as f64 * cell_height).min(bottom));

        (end > top).then(|| {
            NSRect::new(NSPoint::new(lane.origin.x, top), NSSize::new(lane.size.width, end - top))
        })
    }
}

/// Geometry of the outer lanes, in AppKit points rather than drawable grid cells.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrollbarLaneGeometry {
    pub edge_x: f64,
    pub top: f64,
    pub height: f64,
    pub left_width: f64,
    pub cell_height: f64,
}

impl ScrollbarLaneGeometry {
    pub fn from_pixels(
        scale: f64,
        edge_x: f64,
        top: f64,
        height: f64,
        left_width: f64,
        cell_height: f64,
    ) -> Self {
        if !scale.is_finite() || scale <= 0.0 {
            return Self::default();
        }
        Self {
            edge_x: (edge_x / scale).max(0.0),
            top: (top / scale).max(0.0),
            height: (height / scale).max(0.0),
            left_width: (left_width / scale).max(0.0),
            cell_height: (cell_height / scale).max(0.0),
        }
    }

    pub fn frame(&self, side: ScrollbarSide, bounds: NSRect, fallback_width: f64) -> NSRect {
        let layout = if self.height > 0.0 {
            ScrollbarLayout::RendererProvided(RendererScrollbarLayout {
                edge_x: self.edge_x,
                top: self.top,
                height: self.height,
            })
        } else {
            ScrollbarLayout::TrailingEdge { width: fallback_width }
        };

        let right = scrollbar_frame(bounds, layout);
        match side {
            ScrollbarSide::Left => left_scrollbar_frame(right, self.left_width),
            ScrollbarSide::Right => right,
        }
    }
}

pub struct Scroller;
impl Scroller {
    pub const WIDTH_FALLBACK: f64 = 12.0;
    pub const CONTROL_SIZE_REGULAR: i64 = 0;
    pub const STYLE_LEGACY: i64 = 0;
}

#[derive(Clone, Copy, Debug)]
pub enum ScrollbarLayout {
    TrailingEdge { width: f64 },
    RendererProvided(RendererScrollbarLayout),
}

#[derive(Clone, Copy, Debug)]
pub struct RendererScrollbarLayout {
    pub edge_x: f64,
    pub top: f64,
    pub height: f64,
}

/// Mirrors the right lane's vertical bounds into the reserved left padding.
pub fn left_scrollbar_frame(right_frame: NSRect, width: f64) -> NSRect {
    NSRect::new(
        NSPoint::new(0.0, right_frame.origin.y),
        NSSize::new(width.clamp(0.0, right_frame.origin.x), right_frame.size.height),
    )
}

pub fn scrollbar_width() -> f64 {
    appkit_scrollbar_width().unwrap_or(Scroller::WIDTH_FALLBACK)
}

fn appkit_scrollbar_width() -> Option<f64> {
    let width = unsafe {
        msg_send![
            class!(NSScroller),
            scrollerWidthForControlSize: Scroller::CONTROL_SIZE_REGULAR,
            scrollerStyle: Scroller::STYLE_LEGACY
        ]
    };

    (width > 0.0).then_some(width)
}

pub fn scrollbar_frame(bounds: NSRect, layout: ScrollbarLayout) -> NSRect {
    match layout {
        ScrollbarLayout::TrailingEdge { width } => trailing_edge_frame(bounds, width),
        ScrollbarLayout::RendererProvided(layout) => renderer_provided_frame(bounds, layout),
    }
}

fn trailing_edge_frame(bounds: NSRect, width: f64) -> NSRect {
    let origin = NSPoint::new((bounds.size.width - width).max(0.0), 0.0);
    let width = width.min(bounds.size.width - origin.x).max(0.0);

    NSRect::new(origin, NSSize::new(width, bounds.size.height))
}

fn renderer_provided_frame(bounds: NSRect, layout: RendererScrollbarLayout) -> NSRect {
    let origin = NSPoint::new(
        layout.edge_x.clamp(0.0, bounds.size.width),
        layout.top.clamp(0.0, bounds.size.height),
    );
    let width = (bounds.size.width - origin.x).max(0.0);
    let height = layout.height.min(bounds.size.height - origin.y).max(0.0);

    NSRect::new(origin, NSSize::new(width, height))
}
