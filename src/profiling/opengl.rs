use std::ffi::CString;
use std::sync::atomic::AtomicU8;
use std::sync::atomic::Ordering;

use gl::GenQueries;
use gl::GetInteger64v;
use gl::GetQueryObjectiv;
use gl::GetQueryObjectui64v;
use gl::QUERY_RESULT;
use gl::QUERY_RESULT_AVAILABLE;
use gl::QueryCounter;
use gl::TIMESTAMP;
use tracy_client_sys::___tracy_emit_gpu_context_name;
use tracy_client_sys::___tracy_emit_gpu_new_context;
use tracy_client_sys::___tracy_emit_gpu_time_serial;
use tracy_client_sys::___tracy_emit_gpu_zone_begin_serial;
use tracy_client_sys::___tracy_emit_gpu_zone_end_serial;
use tracy_client_sys::___tracy_gpu_context_name_data;
use tracy_client_sys::___tracy_gpu_new_context_data;
use tracy_client_sys::___tracy_gpu_time_data;
use tracy_client_sys::___tracy_gpu_zone_begin_data;
use tracy_client_sys::___tracy_gpu_zone_end_data;
use tracy_client_sys::___tracy_source_location_data;

use crate::profiling::GpuContextType;
use crate::profiling::GpuCtx;

static CONTEXT_ID: AtomicU8 = AtomicU8::new(0);

struct GpuCtxOpenGL {
    id: u8,
    query: Vec<u32>,
    head: usize,
    tail: usize,
}

impl GpuCtxOpenGL {
    fn new() -> Self {
        let len = 64 * 1024;
        let mut query = Vec::with_capacity(len);
        let remaining = query.spare_capacity_mut();
        unsafe {
            GenQueries(remaining.len() as i32, remaining.as_mut_ptr() as *mut u32);
            query.set_len(len);
        }

        Self { id: CONTEXT_ID.fetch_add(1, Ordering::Relaxed), query, head: 0, tail: 0 }
    }

    fn next_query_id(&mut self) -> usize {
        let query = self.head;
        self.head = (self.head + 1) % self.query.len();
        assert!(self.head != self.tail);
        query
    }
}

pub fn create_opengl_gpu_context(name: &str) -> Box<dyn GpuCtx> {
    let ret = Box::new(GpuCtxOpenGL::new());

    let mut timestamp: i64 = 0;
    unsafe {
        GetInteger64v(TIMESTAMP, &mut timestamp);
    }

    let ctxt_data = ___tracy_gpu_new_context_data {
        gpuTime: timestamp,
        period: 1.0,
        context: ret.id,
        flags: 0,
        type_: GpuContextType::OpenGl as u8,
    };
    let namestring = CString::new(name).unwrap();
    let name_data = ___tracy_gpu_context_name_data {
        context: ret.id,
        name: namestring.as_ptr(),
        len: name.len() as u16,
    };
    unsafe {
        ___tracy_emit_gpu_new_context(ctxt_data);
        ___tracy_emit_gpu_context_name(name_data);
    }
    ret
}

impl GpuCtx for GpuCtxOpenGL {
    fn gpu_collect(&mut self) {
        while self.tail != self.head {
            let mut available: i32 = 0;
            unsafe {
                GetQueryObjectiv(self.query[self.tail], QUERY_RESULT_AVAILABLE, &mut available);
            }
            if available <= 0 {
                break;
            }

            let mut time: u64 = 0;
            unsafe {
                GetQueryObjectui64v(self.query[self.tail], QUERY_RESULT, &mut time);
            }
            let time_data = ___tracy_gpu_time_data {
                gpuTime: time as i64,
                queryId: self.tail as u16,
                context: self.id,
            };
            unsafe {
                ___tracy_emit_gpu_time_serial(time_data);
            }
            self.tail = (self.tail + 1) % self.query.len();
        }
    }

    fn gpu_begin(&mut self, loc_data: &___tracy_source_location_data) -> i64 {
        let query = self.next_query_id();
        let glquery = self.query[query];
        let context = self.id;

        let gpu_data = ___tracy_gpu_zone_begin_data {
            srcloc: (loc_data as *const ___tracy_source_location_data) as u64,
            queryId: query as u16,
            context,
        };
        unsafe {
            QueryCounter(glquery, TIMESTAMP);
            ___tracy_emit_gpu_zone_begin_serial(gpu_data);
        }
        // Any positive id is fine here, since the opengl implementation does not use it
        1
    }

    fn gpu_end(&mut self, _query_id: i64) {
        let query = self.next_query_id();
        let glquery = self.query[query];
        let context = self.id;

        let gpu_data = ___tracy_gpu_zone_end_data { queryId: query as u16, context };
        unsafe {
            QueryCounter(glquery, TIMESTAMP);
            ___tracy_emit_gpu_zone_end_serial(gpu_data);
        }
    }
}
