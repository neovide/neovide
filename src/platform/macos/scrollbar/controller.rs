use objc2::msg_send;
use objc2_app_kit::NSScroller;

use crate::bridge::{SerialCommand, send_ui};

use super::{
    geometry::live_top_line,
    interaction::{input_state, is_dragging_knob, track_knob, with_programmatic_update},
    metrics::ScrollMetrics,
    registry::update_pending_scroll,
    state::ScrollbarState,
};

pub struct Part;
impl Part {
    pub const DECREMENT_PAGE: i64 = 1;
    pub const KNOB: i64 = 2;
    pub const INCREMENT_PAGE: i64 = 3;
    pub const DECREMENT_LINE: i64 = 4;
    pub const INCREMENT_LINE: i64 = 5;
    pub const KNOB_SLOT: i64 = 6;
}

pub struct Drag;
impl Drag {
    pub const POLL_INTERVAL: f64 = 1.0 / 120.0;
}

pub struct Diagnostics;
impl Diagnostics {
    pub const DRAG: bool = false;
}

/// Target identity and viewport measurements needed to build a scroll command.
#[derive(Clone, Copy)]
struct ScrollRequestInput {
    window: u64,
    buffer: u64,
    metrics: ScrollMetrics,
    top_line: f64,
    current_line: f64,
}

impl From<&ScrollbarState> for ScrollRequestInput {
    fn from(state: &ScrollbarState) -> Self {
        Self {
            window: state.window,
            buffer: state.buffer,
            metrics: state.metrics,
            top_line: state.top_line,
            current_line: state.current_line,
        }
    }
}

/// Builds a command for a resolved zero-based buffer position.
///
/// Callers resolve and clamp the destination before building the request. The
/// cursor keeps its offset within the viewport, limited to the destination page.
fn scroll_to_command(input: ScrollRequestInput, target_top: f64) -> SerialCommand {
    let target_topline = target_top + 1.0;
    let relative_row = (input.current_line - input.top_line).round().max(0.0);
    let max_cursor = (target_topline + input.metrics.visible_lines() - 1.0)
        .min(input.metrics.line_count())
        .max(target_topline);
    let cursor_lnum =
        (target_topline + relative_row).clamp(target_topline, max_cursor).round() as i64;

    SerialCommand::ScrollTo {
        window: input.window as i64,
        buffer: input.buffer as i64,
        topline: target_top as i64 + 1,
        cursor_lnum,
    }
}

pub fn send_scroll_to_for_scrollbar_value(scroller: &NSScroller, value: f64) {
    if !value.is_finite() {
        return;
    }
    let Some(state) = input_state(scroller) else {
        return;
    };

    let metrics = state.metrics;
    if !metrics.is_scrollable() {
        return;
    }

    let current_value = value.clamp(0.0, 1.0);

    let target_top = metrics.top_line_for_value(current_value);
    let command = scroll_to_command(ScrollRequestInput::from(&state), target_top);
    if Diagnostics::DRAG {
        log::info!(
            "scrollbar ScrollTo value={current_value:.4} target_top={target_top:.1} command={command:?}"
        );
    }

    send_ui(command, &state.neovim_handler);
}

pub fn handle_scrollbar_action(scroller: &NSScroller) {
    let Some(state) = input_state(scroller) else {
        return;
    };

    let metrics = state.metrics;
    if !metrics.is_scrollable() {
        return;
    }

    let hit_part: i64 = unsafe { msg_send![scroller, hitPart] };
    let is_drag = matches!(hit_part, Part::KNOB | Part::KNOB_SLOT);
    if is_drag && !is_dragging_knob(scroller) {
        track_knob(scroller);
    }

    let live_top = live_top_line(scroller, &state);
    let base_top = metrics.clamp_top_line(live_top);
    let target_top = match hit_part {
        Part::DECREMENT_LINE => base_top - 1.0,
        Part::INCREMENT_LINE => base_top + 1.0,
        Part::DECREMENT_PAGE => base_top - metrics.visible_lines(),
        Part::INCREMENT_PAGE => base_top + metrics.visible_lines(),
        _ => {
            let value: f64 = unsafe { msg_send![scroller, doubleValue] };
            metrics.top_line_for_value(value)
        }
    };

    let target_top = metrics.clamp_top_line(target_top.round());
    let target_value = metrics.value_for_top_line(target_top);

    with_programmatic_update(scroller, || unsafe {
        let _: () = msg_send![scroller, setDoubleValue: target_value];
    });

    update_pending_scroll(scroller, target_top);
    let command = scroll_to_command(ScrollRequestInput::from(&state), target_top);
    send_ui(command, &state.neovim_handler);
}
