use std::{os::raw::c_char, ptr::null};

use tracy_client_sys::{
    ___tracy_alloc_srcloc_name, ___tracy_c_zone_context, ___tracy_connected,
    ___tracy_emit_frame_mark, ___tracy_emit_plot, ___tracy_emit_zone_begin,
    ___tracy_emit_zone_begin_alloc, ___tracy_emit_zone_end, ___tracy_fiber_enter,
    ___tracy_fiber_leave, ___tracy_source_location_data, ___tracy_startup_profiler,
};

pub struct _LocationData {
    pub data: ___tracy_source_location_data,
}

unsafe impl Send for _LocationData {}
unsafe impl Sync for _LocationData {}

#[allow(unconditional_panic)]
#[allow(clippy::out_of_bounds_indexing)]
const fn illegal_null_in_string() {
    [][0]
}

#[doc(hidden)]
pub const fn validate_cstr_contents(bytes: &[u8]) {
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\0' {
            illegal_null_in_string();
        }
        i += 1;
    }
}

macro_rules! cstr {
    ( $s:literal ) => {{
        $crate::profiling::validate_cstr_contents($s.as_bytes());
        unsafe { std::mem::transmute::<_, &std::ffi::CStr>(concat!($s, "\0")) }
    }};
}

macro_rules! file_cstr {
    ( ) => {{
        unsafe { std::mem::transmute::<_, &std::ffi::CStr>(concat!(std::file!(), "\0")) }
    }};
}

pub const fn _create_location_data(
    name: &std::ffi::CStr,
    function: &std::ffi::CStr,
    file: &std::ffi::CStr,
    line: u32,
    color: u32,
) -> _LocationData {
    _LocationData {
        data: ___tracy_source_location_data {
            name: name.as_ptr(),
            function: function.as_ptr(),
            file: file.as_ptr(),
            line,
            color,
        },
    }
}

#[allow(dead_code)]
pub fn is_connected() -> bool {
    unsafe { ___tracy_connected() > 0 }
}

pub fn gpu_enabled() -> bool {
    false
}

pub struct _Zone {
    context: ___tracy_c_zone_context,
    gpu_id: i64,
}

impl _Zone {
    pub fn new(loc_data: &___tracy_source_location_data, gpu: bool) -> Self {
        let context = unsafe { ___tracy_emit_zone_begin(loc_data, 1) };
        let gpu_id = {
            if gpu && gpu_enabled() {
                gpu_begin(loc_data)
            } else {
                -1
            }
        };
        _Zone { context, gpu_id }
    }

    pub fn new_dynamic(line: u32, source: &str, name: &str, gpu: bool) -> Self {
        let function = "Unknown";

        let srcloc = unsafe {
            ___tracy_alloc_srcloc_name(
                line,
                source.as_ptr() as *const c_char,
                source.len(),
                function.as_ptr() as *const c_char,
                function.len(),
                name.as_ptr() as *const c_char,
                name.len(),
            )
        };
        let context = unsafe { ___tracy_emit_zone_begin_alloc(srcloc, 1) };
        let gpu = gpu && gpu_enabled();
        let gpu_id = if gpu {
            unsafe { gpu_begin(&*(srcloc as *const ___tracy_source_location_data)) }
        } else {
            -1
        };
        _Zone { context, gpu_id }
    }
}

impl Drop for _Zone {
    fn drop(&mut self) {
        if self.gpu_id >= 0 && gpu_enabled() {
            gpu_end(self.gpu_id);
        }
        unsafe {
            ___tracy_emit_zone_end(self.context);
        }
    }
}

pub fn startup_profiler() {
    unsafe {
        ___tracy_startup_profiler();
    }
}

#[inline(always)]
pub fn tracy_frame() {
    unsafe {
        ___tracy_emit_frame_mark(null());
    }
}

#[inline(always)]
fn gpu_begin(_loc_data: &___tracy_source_location_data) -> i64 {
    0
}
#[inline(always)]
fn gpu_end(_query_id: i64) {}

#[inline(always)]
pub fn _tracy_named_frame(name: &std::ffi::CStr) {
    unsafe {
        ___tracy_emit_frame_mark(name.as_ptr());
    }
}

#[inline(always)]
pub fn _tracy_plot(name: &std::ffi::CStr, value: f64) {
    unsafe {
        ___tracy_emit_plot(name.as_ptr(), value);
    }
}

#[inline(always)]
pub fn _tracy_fiber_enter(name: &std::ffi::CStr) {
    unsafe {
        ___tracy_fiber_enter(name.as_ptr());
    }
}

#[inline(always)]
pub fn tracy_fiber_leave() {
    unsafe {
        ___tracy_fiber_leave();
    }
}

#[macro_export]
macro_rules! location_data {
    ($name: expr, $color: expr) => {{
        static LOC: $crate::profiling::_LocationData = $crate::profiling::_create_location_data(
            $crate::profiling::cstr!($name),
            // There does not seem to be any way of getting a c string to the current
            // function until this is implemented
            // https://github.com/rust-lang/rust/issues/63084
            // So use Unknown for now
            $crate::profiling::cstr!("Unknown"),
            $crate::profiling::file_cstr!(),
            std::line!(),
            $color,
        );
        &LOC.data
    }};
}

#[macro_export]
macro_rules! tracy_zone {
    ($name: expr, $color: expr) => {
        let _tracy_zone =
            $crate::profiling::_Zone::new($crate::profiling::location_data!($name, $color), false);
    };
    ($name: expr) => {
        $crate::profiling::tracy_zone!($name, 0)
    };
}

#[macro_export]
macro_rules! tracy_dynamic_zone {
    ($name: expr, $color: expr) => {
        let _tracy_zone =
            $crate::profiling::_Zone::new_dynamic(std::line!(), std::file!(), $name, false);
    };
    ($name: expr) => {
        $crate::profiling::tracy_dynamic_zone!($name, 0)
    };
}

#[macro_export]
macro_rules! tracy_named_frame {
    ($name: expr) => {
        $crate::profiling::_tracy_named_frame($crate::profiling::cstr!($name))
    };
}

#[macro_export]
macro_rules! tracy_plot {
    ($name: expr, $dt: expr) => {
        $crate::profiling::_tracy_plot($crate::profiling::cstr!($name), $dt)
    };
}

#[macro_export]
macro_rules! tracy_fiber_enter {
    ($name: expr) => {
        $crate::profiling::_tracy_fiber_enter($crate::profiling::cstr!($name))
    };
}

pub(crate) use cstr;
pub(crate) use file_cstr;
pub(crate) use location_data;
pub(crate) use tracy_dynamic_zone;
pub(crate) use tracy_fiber_enter;
pub(crate) use tracy_named_frame;
pub(crate) use tracy_plot;
pub(crate) use tracy_zone;
