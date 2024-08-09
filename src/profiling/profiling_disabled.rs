#[inline(always)]
pub fn startup_profiler() {}

#[inline(always)]
#[allow(unused)]
pub fn tracy_frame() {}

macro_rules! tracy_zone {
    ($name: expr, $color: expr) => {};
    ($name: expr) => {};
}
macro_rules! tracy_dynamic_zone {
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
pub(crate) use tracy_named_frame;
pub(crate) use tracy_plot;
pub(crate) use tracy_zone;
