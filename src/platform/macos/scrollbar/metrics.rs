use super::{
    geometry::{knob_proportion, line_count_is_valid, visible_buffer_lines},
    state::ScrollbarViewport,
};

/// Validated buffer and viewport lengths, both measured in buffer lines.
///
/// Screen rows aren't interchangeable with buffer lines when text wraps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollMetrics {
    line_count: f64,
    visible_lines: f64,
}

impl ScrollMetrics {
    pub fn new(line_count: f64, visible_lines: f64) -> Option<Self> {
        (line_count.is_finite()
            && line_count >= 1.0
            && visible_lines.is_finite()
            && visible_lines > 0.0)
            .then_some(Self { line_count, visible_lines })
    }

    /// Uses the viewport's line count when it agrees with the reported positions.
    ///
    /// If the count is missing or invalid, it falls back to the previous count.
    /// a valid smaller count replaces the old one, even if no thumb is needed.
    pub fn from_viewport(viewport: &ScrollbarViewport, previous: Option<Self>) -> Option<Self> {
        let visible_lines = visible_buffer_lines(viewport);
        let line_count = viewport
            .line_count
            .filter(|count| line_count_is_valid(viewport, *count))
            .or_else(|| previous.map(Self::line_count))?;
        Self::new(line_count, visible_lines)
    }

    pub fn line_count(self) -> f64 {
        self.line_count
    }

    pub fn visible_lines(self) -> f64 {
        self.visible_lines
    }

    pub fn is_scrollable(self) -> bool {
        self.line_count > self.visible_lines
    }

    pub fn max_top(self) -> f64 {
        (self.line_count - self.visible_lines).max(0.0)
    }

    pub fn clamp_top_line(self, top_line: f64) -> f64 {
        top_line.clamp(0.0, self.max_top())
    }

    /// Fractional positions are kept within the scroll range so the thumb can
    /// follow the scroll animation.
    pub fn value_for_top_line(self, top_line: f64) -> f64 {
        (self.clamp_top_line(top_line) / self.max_top().max(1.0)).clamp(0.0, 1.0)
    }

    /// The value is clamped to 0..=1, then the line is rounded and kept in range.
    pub fn top_line_for_value(self, value: f64) -> f64 {
        self.clamp_top_line((value.clamp(0.0, 1.0) * self.max_top()).round())
    }

    pub fn knob_proportion(self) -> f64 {
        knob_proportion(self.visible_lines, self.line_count)
    }
}
