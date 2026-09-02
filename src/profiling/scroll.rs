use std::sync::atomic::{AtomicU64, Ordering};

use super::tracy_plot;

static SCROLL_WHEEL_TICKS_TOTAL: AtomicU64 = AtomicU64::new(0);

/// Records how much wheel input is represented by one executed scroll command.
pub fn scroll_tick_count(count: u32) {
    tracy_plot!("scroll wheel ticks per command", count as f64);

    let total = SCROLL_WHEEL_TICKS_TOTAL.fetch_add(count as u64, Ordering::Relaxed) + count as u64;
    tracy_plot!("scroll wheel ticks total", total as f64);
}
