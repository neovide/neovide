use objc2::{class, msg_send};
use objc2_foundation::{NSPoint, NSRect, NSSize};

use crate::{
    editor::ScrollbarGrid,
    units::{GridSize, PixelPos, PixelSize},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScrollbarSide {
    Left,
    Right,
}

/// The painted slot and its trailing transparent margin, in appkit points.
///
/// the control frame places the whole scroller in the content view. the painted
/// slot is only the part reported by rectForPart(KnobSlot), where the track and
/// thumb are drawn. the frame can also contain transparent space, so its width
/// isn't the width of the visible track.
///
/// `width` measures the slot itself and determines the scrollbar's contribution
/// to the outer window padding. `trailing_inset` measures the distance from the
/// slot's right edge to the right edge of the control's bounds. both come from
/// the actual appkit control rather than an assumed border size.
///
/// on the right, [`ScrollbarLaneGeometry::frame`] extends the control frame by
/// that inset. the transparent margin ends up beyond the content edge, leaving
/// the painted slot flush with it. this adjusts the frame, not the slot or thumb
/// width.
///
/// on the left, extending the frame toward the grid would put the control over
/// text. [`left_scrollbar_bounds_origin`] instead shifts the bounds origin so
/// the slot's right edge meets the frame's right edge. the frame stays inside
/// the reserved padding and the slot and thumb keep their dimensions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollbarTrackMetrics {
    pub width: f64,
    pub trailing_inset: f64,
}

impl ScrollbarTrackMetrics {
    pub fn from_slot(bounds: NSRect, slot: NSRect) -> Option<Self> {
        let right = bounds.origin.x + bounds.size.width;
        let slot_end = slot.origin.x + slot.size.width;
        (right.is_finite()
            && slot.size.width > 0.0
            && slot.origin.x >= bounds.origin.x
            && slot_end <= right)
            .then_some(Self { width: slot.size.width, trailing_inset: right - slot_end })
    }
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
        if segment_geometry_is_invalid(lane, cell_height) {
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

fn segment_geometry_is_invalid(lane: NSRect, cell_height: f64) -> bool {
    !cell_height.is_finite()
        || cell_height <= 0.0
        || lane.size.width <= 0.0
        || lane.size.height <= 0.0
}

/// Committed grid dimensions and placement, before conversion to AppKit points.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrollbarLayoutInput {
    pub root_grid_size: GridSize<u64>,
    pub grid_origin: PixelPos<f64>,
    /// Drawable content size after subtracting the outer window padding.
    pub content_size: PixelSize<f64>,
    pub cell_size: PixelSize<f64>,
    pub left_lane_enabled: bool,
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

/// Inputs that can change control frames without changing their grid assignments.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollbarFrameInputs {
    pub geometry: ScrollbarLaneGeometry,
    pub bounds: NSRect,
    pub scale_factor: f64,
    pub track_metrics: ScrollbarTrackMetrics,
}

impl ScrollbarLaneGeometry {
    pub fn from_layout(layout: ScrollbarLayoutInput, backing_scale: f64) -> Self {
        if !backing_scale.is_finite() || backing_scale <= 0.0 {
            return Self::default();
        }

        let edge_x =
            layout.grid_origin.x + layout.root_grid_size.width as f64 * layout.cell_size.width;
        let left_width = if layout.left_lane_enabled { layout.grid_origin.x } else { 0.0 };

        Self {
            edge_x: (edge_x / backing_scale).max(0.0),
            top: (layout.grid_origin.y / backing_scale).max(0.0),
            height: (layout.content_size.height / backing_scale).max(0.0),
            left_width: (left_width / backing_scale).max(0.0),
            cell_height: (layout.cell_size.height / backing_scale).max(0.0),
        }
    }

    pub fn frame(
        &self,
        side: ScrollbarSide,
        bounds: NSRect,
        fallback_width: f64,
        right_trailing_inset: f64,
    ) -> NSRect {
        let layout = if self.height > 0.0 {
            ScrollbarLayout::RendererProvided(RendererScrollbarLayout {
                edge_x: self.edge_x,
                top: self.top,
                height: self.height,
            })
        } else {
            ScrollbarLayout::TrailingEdge { width: fallback_width }
        };

        let mut right = scrollbar_frame(bounds, layout);
        match side {
            ScrollbarSide::Left => left_scrollbar_frame(right, self.left_width),
            ScrollbarSide::Right => {
                // Put the transparent margin outside the content view, keeping the
                // slot and knob flush with its edge without widening either part.
                if right.size.width > 0.0 {
                    right.size.width += right_trailing_inset;
                }

                right
            }
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

/// Aligns the left slot with the grid edge without extending the control over text.
/// AppKit's slot and knob keep their native dimensions; only the bounds origin shifts.
pub fn left_scrollbar_bounds_origin(bounds: NSRect, slot: NSRect) -> NSPoint {
    let slot_end = slot.origin.x + slot.size.width;
    if !bounds.size.width.is_finite()
        || bounds.size.width <= 0.0
        || !slot.size.width.is_finite()
        || slot.size.width <= 0.0
        || !slot_end.is_finite()
    {
        return bounds.origin;
    }

    NSPoint::new(slot_end - bounds.size.width, bounds.origin.y)
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
