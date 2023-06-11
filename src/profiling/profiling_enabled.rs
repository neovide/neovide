use std::{
    cell::RefCell,
    ffi::CString,
    ptr::null,
    sync::atomic::{AtomicU8, Ordering},
};

use tracy_client_sys::{
    ___tracy_c_zone_context, ___tracy_connected, ___tracy_emit_frame_mark,
    ___tracy_emit_gpu_context_name, ___tracy_emit_gpu_new_context, ___tracy_emit_gpu_time_serial,
    ___tracy_emit_gpu_zone_begin_serial, ___tracy_emit_gpu_zone_end_serial,
    ___tracy_emit_zone_begin, ___tracy_emit_zone_end, ___tracy_gpu_context_name_data,
    ___tracy_gpu_new_context_data, ___tracy_gpu_time_data, ___tracy_gpu_zone_begin_data,
    ___tracy_gpu_zone_end_data, ___tracy_source_location_data, ___tracy_startup_profiler,
};

use gl::{
    GenQueries, GetInteger64v, GetQueryObjectiv, GetQueryObjectui64v, QueryCounter, QUERY_RESULT,
    QUERY_RESULT_AVAILABLE, TIMESTAMP,
};

pub struct _LocationData {
    pub data: ___tracy_source_location_data,
}

unsafe impl Send for _LocationData {}
unsafe impl Sync for _LocationData {}

#[allow(unconditional_panic)]
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
fn is_connected() -> bool {
    unsafe { ___tracy_connected() > 0 }
}

#[cfg(feature = "gpu_profiling")]
fn gpu_enabled() -> bool {
    is_connected()
}

#[cfg(not(feature = "gpu_profiling"))]
fn gpu_enabled() -> bool {
    false
}

pub struct _Zone {
    context: ___tracy_c_zone_context,
    gpu: bool,
}

impl _Zone {
    pub fn new(loc_data: &___tracy_source_location_data, gpu: bool) -> Self {
        let context = unsafe { ___tracy_emit_zone_begin(loc_data, 1) };
        let gpu = gpu && gpu_enabled();
        if gpu {
            let (context, query, glquery) = GPUCTX.with(|ctx| {
                let mut ctx = ctx.borrow_mut();
                let query = ctx.next_query_id();
                (ctx.id, query, ctx.query[query])
            });

            let gpu_data = ___tracy_gpu_zone_begin_data {
                srcloc: (loc_data as *const ___tracy_source_location_data) as u64,
                queryId: query as u16,
                context,
            };
            unsafe {
                QueryCounter(glquery, TIMESTAMP);
                ___tracy_emit_gpu_zone_begin_serial(gpu_data);
            }
        }
        _Zone { context, gpu }
    }
}

impl Drop for _Zone {
    fn drop(&mut self) {
        if self.gpu && gpu_enabled() {
            let (context, query, glquery) = GPUCTX.with(|ctx| {
                let mut ctx = ctx.borrow_mut();
                let query = ctx.next_query_id();
                (ctx.id, query, ctx.query[query])
            });
            let gpu_data = ___tracy_gpu_zone_end_data {
                queryId: query as u16,
                context,
            };
            unsafe {
                QueryCounter(glquery, TIMESTAMP);
                ___tracy_emit_gpu_zone_end_serial(gpu_data);
            }
        }
        unsafe {
            ___tracy_emit_zone_end(self.context);
        }
    }
}

static CONTEXT_ID: AtomicU8 = AtomicU8::new(0);

struct GpuCtx {
    id: u8,
    query: Vec<u32>,
    head: usize,
    tail: usize,
}

impl GpuCtx {
    fn new() -> Self {
        let len = 64 * 1024;
        let mut query = Vec::with_capacity(len);
        let remaining = query.spare_capacity_mut();
        unsafe {
            GenQueries(remaining.len() as i32, remaining.as_mut_ptr() as *mut u32);
            query.set_len(len);
        }

        Self {
            id: CONTEXT_ID.fetch_add(1, Ordering::Relaxed),
            query,
            head: 0,
            tail: 0,
        }
    }

    fn next_query_id(&mut self) -> usize {
        let query = self.head;
        self.head = (self.head + 1) % self.query.len();
        assert!(self.head != self.tail);
        query
    }
}

thread_local! {
    static GPUCTX: RefCell<GpuCtx> = RefCell::new(GpuCtx::new());
}

pub fn startup_profiler() {
    unsafe {
        ___tracy_startup_profiler();
    }
}

#[inline(always)]
pub fn emit_frame_mark() {
    unsafe {
        ___tracy_emit_frame_mark(null());
    }
}

pub fn tracy_create_gpu_context(name: &str) {
    // Don't change order, only add new entries at the end, this is also used on trace dumps!
    #[allow(dead_code)]
    enum GpuContextType {
        Invalid,
        OpenGl,
        Vulkan,
        OpenCL,
        Direct3D12,
        Direct3D11,
    }

    let id = GPUCTX.with(|ctx| {
        let ctx = ctx.borrow();
        ctx.id
    });

    let mut timestamp: i64 = 0;
    unsafe {
        GetInteger64v(TIMESTAMP, &mut timestamp);
    }

    let ctxt_data = ___tracy_gpu_new_context_data {
        gpuTime: timestamp,
        period: 1.0,
        context: id,
        flags: 0,
        type_: GpuContextType::OpenGl as u8,
    };
    let namestring = CString::new(name).unwrap();
    let name_data = ___tracy_gpu_context_name_data {
        context: id,
        name: namestring.as_ptr(),
        len: name.len() as u16,
    };
    unsafe {
        ___tracy_emit_gpu_new_context(ctxt_data);
        ___tracy_emit_gpu_context_name(name_data);
    }
}

pub fn tracy_gpu_collect() {
    tracy_zone!("collect gpu info");
    if !gpu_enabled() {
        return;
    }

    GPUCTX.with(|ctx| {
        let mut ctx = ctx.borrow_mut();

        while ctx.tail != ctx.head {
            let mut available: i32 = 0;
            unsafe {
                GetQueryObjectiv(ctx.query[ctx.tail], QUERY_RESULT_AVAILABLE, &mut available);
            }
            if available <= 0 {
                break;
            }

            let mut time: u64 = 0;
            unsafe {
                GetQueryObjectui64v(ctx.query[ctx.tail], QUERY_RESULT, &mut time);
            }
            let time_data = ___tracy_gpu_time_data {
                gpuTime: time as i64,
                queryId: ctx.tail as u16,
                context: ctx.id,
            };
            unsafe {
                ___tracy_emit_gpu_time_serial(time_data);
            }
            ctx.tail = (ctx.tail + 1) % ctx.query.len();
        }
    });
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
macro_rules! tracy_gpu_zone {
    ($name: expr, $color: expr) => {
        let _tracy_zone =
            $crate::profiling::_Zone::new($crate::profiling::location_data!($name, $color), true);
    };
    ($name: expr) => {
        $crate::profiling::tracy_gpu_zone!($name, 0)
    };
}

pub(crate) use cstr;
pub(crate) use file_cstr;
pub(crate) use location_data;
pub(crate) use tracy_gpu_zone;
pub(crate) use tracy_zone;
