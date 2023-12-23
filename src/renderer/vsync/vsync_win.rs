use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{spawn, JoinHandle},
    time::Duration,
};
use winapi::{
    shared::ntdef::LARGE_INTEGER,
    um::dwmapi::{DwmGetCompositionTimingInfo, DWM_TIMING_INFO},
    um::profileapi::{QueryPerformanceCounter, QueryPerformanceFrequency},
};

use winit::event_loop::EventLoopProxy;

use spin_sleep::SpinSleeper;

use crate::{
    profiling::{tracy_plot, tracy_zone},
    window::UserEvent,
};

pub struct VSyncWin {
    should_exit: Arc<AtomicBool>,
    vsync_thread: Option<JoinHandle<()>>,
    redraw_requested: Arc<AtomicBool>,
}

/// Calculates the time until the vblank, taking into account that the vblank is cyclic, so this
/// always finds the next vblank forward
fn time_until_vblank_forward(delay: f64, period: f64) -> f64 {
    delay.rem_euclid(period)
}

/// Calculates the time to wait to hit the vblank + offset given a period
///
/// Note: always tries to wait as close to 1 frame as possible, so the actual wait time lies
/// between 0.5 * offset and 1.5 * offset
fn vblank_wait_time(delay: f64, period: f64, offset: f64) -> f64 {
    let time_until_vblank = time_until_vblank_forward(delay + offset, period);
    if time_until_vblank < 0.5 * period {
        return time_until_vblank + period;
    }
    time_until_vblank
}

impl VSyncWin {
    // On Windows the fake vsync is always enabled
    // Everything else is very jerky
    pub fn new(proxy: EventLoopProxy<UserEvent>) -> Self {
        let should_exit = Arc::new(AtomicBool::new(false));
        let redraw_requested = Arc::new(AtomicBool::new(false));

        // When using OpenGL on Windows in windowed mode, swap_buffers does not seem to be
        // synchronized with the Desktop Window Manager. So work around that by manually waiting
        // for the middle of the vblank. Experimentally that seems to be optimal with the least
        // amount of stutter.
        let vsync_thread = {
            let should_exit = Arc::clone(&should_exit);
            let redraw_requested = Arc::clone(&redraw_requested);
            Some(spawn(move || {
                let performance_frequency = unsafe {
                    let mut performance_frequency: LARGE_INTEGER = std::mem::zeroed();
                    QueryPerformanceFrequency(&mut performance_frequency as *mut LARGE_INTEGER);
                    *performance_frequency.QuadPart() as f64
                };
                let sleeper = SpinSleeper::default();
                while !should_exit.load(Ordering::SeqCst) {
                    tracy_zone!("VSyncThread");
                    let (_vblank_delay, _sleep_time) = unsafe {
                        let mut timing_info: DWM_TIMING_INFO = std::mem::zeroed();
                        timing_info.cbSize = std::mem::size_of::<DWM_TIMING_INFO>() as u32;
                        DwmGetCompositionTimingInfo(
                            std::ptr::null_mut(),
                            &mut timing_info as *mut DWM_TIMING_INFO,
                        );
                        let mut time_now: LARGE_INTEGER = std::mem::zeroed();
                        QueryPerformanceCounter(&mut time_now as *mut LARGE_INTEGER);
                        let time_now = *time_now.QuadPart() as f64;
                        let vblank_delay =
                            (timing_info.qpcVBlank as f64 - time_now) / performance_frequency;
                        let period = ((timing_info.qpcRefreshPeriod as f64)
                            / performance_frequency)
                            .max(0.001);

                        // Target the middle of the vblank, which gives maximum time for both us and the compositor
                        let sleep_time = vblank_wait_time(vblank_delay, period, 0.5 * period);
                        sleeper.sleep(Duration::from_secs_f64(sleep_time));

                        (vblank_delay, sleep_time)
                    };
                    tracy_plot!("Vblank_delay", _vblank_delay);
                    tracy_plot!("sleep_time", _sleep_time);

                    if redraw_requested.swap(false, Ordering::Relaxed) {
                        let _ = proxy.send_event(UserEvent::RedrawRequested);
                    }
                }
            }))
        };

        Self {
            should_exit,
            vsync_thread,
            redraw_requested,
        }
    }

    pub fn wait_for_vsync(&mut self) {}

    pub fn request_redraw(&mut self) {
        self.redraw_requested.store(true, Ordering::Relaxed);
    }
}

impl Drop for VSyncWin {
    fn drop(&mut self) {
        self.should_exit.store(true, Ordering::SeqCst);
        self.vsync_thread.take().unwrap().join().unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_until_vblank_forward() {
        assert_abs_diff_eq!(time_until_vblank_forward(0.3, 1.0), 0.3);
        assert_abs_diff_eq!(time_until_vblank_forward(0.9, 1.0), 0.9);
        assert_abs_diff_eq!(time_until_vblank_forward(1.0, 1.0), 0.0);
        assert_abs_diff_eq!(time_until_vblank_forward(1.1, 1.0), 0.1);
        assert_abs_diff_eq!(time_until_vblank_forward(-0.2, 1.0), 0.8);
        assert_abs_diff_eq!(time_until_vblank_forward(-1.0, 1.0), 0.0);
        assert_abs_diff_eq!(time_until_vblank_forward(0.001, 1.0 / 120.0), 0.001);
        assert_abs_diff_eq!(
            time_until_vblank_forward(0.009, 1.0 / 120.0),
            0.009 - 1.0 / 120.0
        );
        assert_abs_diff_eq!(time_until_vblank_forward(0.006739, 1.0 / 144.0), 0.006739);
    }

    #[test]
    fn test_vblank_wait_time() {
        assert_abs_diff_eq!(vblank_wait_time(0.3, 1.0, 0.0), 1.3);
        assert_abs_diff_eq!(vblank_wait_time(0.9, 1.0, 0.0), 0.9);
        assert_abs_diff_eq!(vblank_wait_time(1.0, 1.0, 0.0), 1.0);
        assert_abs_diff_eq!(vblank_wait_time(1.1, 1.0, 0.0), 1.1);
        assert_abs_diff_eq!(vblank_wait_time(2.1, 1.0, 0.0), 1.1);
        assert_abs_diff_eq!(vblank_wait_time(-0.2, 1.0, 0.0), 0.8);
        assert_abs_diff_eq!(vblank_wait_time(-1.0, 1.0, 0.0), 1.0);
        assert_abs_diff_eq!(vblank_wait_time(-1.4, 1.0, 0.0), 0.6);
        assert_abs_diff_eq!(
            vblank_wait_time(0.001, 1.0 / 120.0, 0.0),
            1.0 / 120.0 + 0.001
        );
        assert_abs_diff_eq!(vblank_wait_time(0.009, 1.0 / 120.0, 0.0), 0.009);
        assert_abs_diff_eq!(vblank_wait_time(0.006739, 1.0 / 144.0, 0.0), 0.006739);
    }

    #[test]
    fn test_vblank_wait_time_with_half_offset() {
        assert_abs_diff_eq!(vblank_wait_time(0.3, 1.0, 0.5), 0.8);
        assert_abs_diff_eq!(vblank_wait_time(0.9, 1.0, 0.5), 1.4);
        assert_abs_diff_eq!(vblank_wait_time(1.0, 1.0, 0.5), 0.5);
        assert_abs_diff_eq!(vblank_wait_time(1.1, 1.0, 0.5), 0.6);
        assert_abs_diff_eq!(vblank_wait_time(2.1, 1.0, 0.5), 0.6);
        assert_abs_diff_eq!(vblank_wait_time(-0.2, 1.0, 0.5), 1.3);
        assert_abs_diff_eq!(vblank_wait_time(-1.0, 1.0, 0.5), 0.5);
        assert_abs_diff_eq!(vblank_wait_time(-1.4, 1.0, 0.5), 1.1);
        assert_abs_diff_eq!(vblank_wait_time(0.001, 1.0 / 120.0, 0.004), 0.005);
        assert_abs_diff_eq!(
            vblank_wait_time(0.009, 1.0 / 120.0, 0.004),
            0.013 - 1.0 / 120.0
        );
    }
}
