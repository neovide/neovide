#[inline(always)]
pub fn startup_profiler() {}

#[inline(always)]
pub fn tracy_frame() {}

#[inline(always)]
pub fn tracy_create_gpu_context(_name: &str) {}

#[inline(always)]
pub fn tracy_gpu_collect() {}

macro_rules! tracy_zone {
    ($name: expr, $color: expr) => {};
    ($name: expr) => {};
}
macro_rules! tracy_dynamic_zone {
    ($name: expr, $color: expr) => {};
    ($name: expr) => {};
}
macro_rules! tracy_gpu_zone {
    ($name: expr, $color: expr) => {};
    ($name: expr) => {};
}
macro_rules! tracy_named_frame {
    ($name: expr) => {};
}

#[macro_export]
macro_rules! tracy_plot {
    ($name: expr, $dt: expr) => {};
}

#[macro_export]
macro_rules! tracy_fiber_enter {
    ($name: expr) => {};
}

#[inline(always)]
pub fn tracy_fiber_leave() {}

pub(crate) use tracy_dynamic_zone;
pub(crate) use tracy_fiber_enter;
pub(crate) use tracy_gpu_zone;
pub(crate) use tracy_named_frame;
pub(crate) use tracy_plot;
pub(crate) use tracy_zone;
