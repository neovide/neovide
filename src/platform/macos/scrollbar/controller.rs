use std::time::Duration;

use objc2::msg_send;
use objc2_app_kit::NSScroller;

use crate::bridge::{SerialCommand, send_ui};

use super::{
    geometry::{live_top_line, target_cursor_lnum},
    interaction::{input_state, is_dragging_knob, track_knob, with_programmatic_update},
    registry::{mark_user_input, update_pending_scroll},
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
    pub const USER_INPUT_TTL: Duration = Duration::from_millis(150);
    pub const USER_DRAG_TTL: Duration = Duration::from_millis(400);
}

pub struct Diagnostics;
impl Diagnostics {
    pub const DRAG: bool = false;
}

pub fn send_scroll_to_for_scrollbar_value(scroller: &NSScroller, value: f64) {
    if !value.is_finite() {
        return;
    }
    let Some(state) = input_state(scroller) else {
        return;
    };

    let scrollable_lines = state.line_count - state.visible_lines;
    if scrollable_lines <= 0.0 {
        return;
    }

    let max_top = scrollable_lines.max(1.0);
    let current_value = value.clamp(0.0, 1.0);

    mark_user_input(scroller, Drag::USER_DRAG_TTL);

    let target_top = (current_value * max_top).round().clamp(0.0, max_top);
    let topline = target_top as i64 + 1;
    let cursor_lnum = target_cursor_lnum(&state, target_top + 1.0);
    if Diagnostics::DRAG {
        log::info!(
            "scrollbar ScrollTo value={current_value:.4} target_top={target_top:.1} topline={topline} cursor_lnum={cursor_lnum}"
        );
    }

    send_ui(
        SerialCommand::ScrollTo {
            window: state.window as i64,
            buffer: state.buffer as i64,
            topline,
            cursor_lnum,
        },
        &state.neovim_handler,
    );
}

pub fn handle_scrollbar_action(scroller: &NSScroller) {
    let Some(state) = input_state(scroller) else {
        return;
    };

    let max_top = (state.line_count - state.visible_lines).max(0.0);
    if max_top <= 0.0 {
        return;
    }

    let hit_part: i64 = unsafe { msg_send![scroller, hitPart] };
    let is_drag = matches!(hit_part, Part::KNOB | Part::KNOB_SLOT);
    if is_drag && !is_dragging_knob(scroller) {
        track_knob(scroller);
    }

    let ttl = if is_drag { Drag::USER_DRAG_TTL } else { Drag::USER_INPUT_TTL };
    mark_user_input(scroller, ttl);

    let live_top = live_top_line(scroller, &state);
    let base_top = live_top.clamp(0.0, max_top);
    let target_top = match hit_part {
        Part::DECREMENT_LINE => base_top - 1.0,
        Part::INCREMENT_LINE => base_top + 1.0,
        Part::DECREMENT_PAGE => base_top - state.visible_lines,
        Part::INCREMENT_PAGE => base_top + state.visible_lines,
        Part::KNOB | Part::KNOB_SLOT => {
            let value: f64 = unsafe { msg_send![scroller, doubleValue] };
            value.clamp(0.0, 1.0) * max_top
        }
        _ => {
            let value: f64 = unsafe { msg_send![scroller, doubleValue] };
            value.clamp(0.0, 1.0) * max_top
        }
    };

    let target_top = target_top.round().clamp(0.0, max_top);
    let target_value = if max_top > 0.0 { (target_top / max_top).clamp(0.0, 1.0) } else { 0.0 };

    with_programmatic_update(scroller, || unsafe {
        let _: () = msg_send![scroller, setDoubleValue: target_value];
    });

    update_pending_scroll(scroller, target_top);
    let topline = target_top as i64 + 1;
    let cursor_lnum = target_cursor_lnum(&state, target_top + 1.0);

    send_ui(
        SerialCommand::ScrollTo {
            window: state.window as i64,
            buffer: state.buffer as i64,
            topline,
            cursor_lnum,
        },
        &state.neovim_handler,
    );
}
