use objc2::msg_send;
use objc2_app_kit::NSScroller;
use objc2_foundation::{NSPoint, NSRect};

use super::state::{ScrollbarState, ScrollbarViewport};

pub struct Smooth;
impl Smooth {
    pub const THRESHOLD: f64 = 0.01;
    pub const TAU: f64 = 0.12;
    pub const LINE_FACTOR: f64 = 0.75;
    pub const MIN_LINES: f64 = 8.0;
}

pub fn viewport_rows(viewport: &ScrollbarViewport) -> f64 {
    viewport.grid_height.saturating_sub(viewport.top_margin).saturating_sub(viewport.bottom_margin)
        as f64
}

/// Estimates the visible span in buffer lines, the units used by ScrollTo.
///
/// neovim top line is zero-based, while the bottom boundary normally names
/// the next possibly clipped line. excluding that boundary keeps a partially
/// visible wrapped line out of the page size. At EOF the boundary can include
/// one filler line, which must not increase the span either.
///
/// An exactly filled EOF and a clipped final line can have the same boundary.
/// So we prefer a one-line underestimate to making the final line unreachable.
pub fn visible_buffer_lines(viewport: &ScrollbarViewport) -> f64 {
    if viewport_rows(viewport) == 0.0 {
        return 0.0;
    }

    (viewport.bottom_line - viewport.top_line - 1.0).max(1.0)
}

/// Approximates a rendered row offset in buffer lines for thumb animation.
pub fn buffer_line_offset(viewport: &ScrollbarViewport, row_offset: f64) -> f64 {
    let rows = viewport_rows(viewport);
    if rows == 0.0 {
        return 0.0;
    }

    row_offset * visible_buffer_lines(viewport) / rows
}

/// Checks the count against the committed viewport, not the previous buffer length.
///
/// A deletion can legitimately remove most of a buffer without moving the view
/// to the top. neovim also reports one extra bottom line for filler past EOF.
pub fn line_count_is_valid(viewport: &ScrollbarViewport, line_count: f64) -> bool {
    line_count.is_finite()
        && line_count >= 1.0
        && (0.0..line_count).contains(&viewport.top_line)
        && (0.0..line_count).contains(&viewport.current_line)
        && viewport.bottom_line > viewport.top_line
        && viewport.bottom_line <= line_count + 1.0
}

pub fn knob_proportion(visible_lines: f64, line_count: f64) -> f64 {
    let mut knob = (visible_lines / line_count).clamp(0.0, 1.0);
    if knob >= 1.0 {
        knob = 0.99;
    }

    if knob <= 0.0 {
        knob = 0.01;
    }

    knob
}

pub fn point_in_rect(point: NSPoint, rect: NSRect) -> bool {
    point.x >= rect.origin.x
        && point.x <= rect.origin.x + rect.size.width
        && point.y >= rect.origin.y
        && point.y <= rect.origin.y + rect.size.height
}

pub fn live_top_line(scroller: &NSScroller, state: &ScrollbarState) -> f64 {
    let value: f64 = unsafe { msg_send![scroller, doubleValue] };
    if !value.is_finite() {
        return state.metrics.clamp_top_line(state.top_line);
    }

    state.metrics.top_line_for_value(value)
}
