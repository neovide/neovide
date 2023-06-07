#[inline(always)]
pub fn startup_profiler() {}

#[inline(always)]
pub fn emit_frame_mark() {}

#[inline(always)]
pub fn tracy_create_gpu_context(_name: &str) {}

#[inline(always)]
pub fn tracy_gpu_collect() {}

macro_rules! tracy_zone {
    ($name: expr, $color: expr) => {};
    ($name: expr) => {};
}
macro_rules! tracy_gpu_zone {
    ($name: expr, $color: expr) => {};
    ($name: expr) => {};
}

pub(crate) use tracy_gpu_zone;
pub(crate) use tracy_zone;
