use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(target_os = "macos")]
use std::time::Duration;

use super::tracy_plot;

static SCROLL_WHEEL_TICKS_TOTAL: AtomicU64 = AtomicU64::new(0);

/// Records how much wheel input is represented by one executed scroll command.
pub fn scroll_tick_count(count: u32) {
    tracy_plot!("scroll wheel ticks per command", count as f64);

    let total = SCROLL_WHEEL_TICKS_TOTAL.fetch_add(count as u64, Ordering::Relaxed) + count as u64;
    tracy_plot!("scroll wheel ticks total", total as f64);
}

/// Records the wall-clock latency of one absolute scrollbar update.
///
/// ScrollTo uses an async RPC, so this is intentionally a plot instead of a
/// zone. holding a tracy zone across .await can start and finish on different
/// tokio worker threads, which makes the trace misleading.
#[cfg(target_os = "macos")]
pub fn scroll_to_exec_lua_latency(latency: Duration) {
    tracy_plot!("ScrollTo exec_lua latency ms", latency.as_secs_f64() * 1000.0);
}
