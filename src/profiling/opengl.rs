use gl::{
    GenQueries, GetInteger64v, GetQueryObjectiv, GetQueryObjectui64v, QueryCounter, QUERY_RESULT,
    QUERY_RESULT_AVAILABLE, TIMESTAMP,
};
use std::{
    cell::RefCell,
    ffi::CString,
    sync::atomic::{AtomicU8, Ordering},
};

use tracy_client_sys::{
    ___tracy_emit_gpu_context_name, ___tracy_emit_gpu_new_context, ___tracy_emit_gpu_time_serial,
    ___tracy_emit_gpu_zone_begin_serial, ___tracy_emit_gpu_zone_end_serial,
    ___tracy_gpu_context_name_data, ___tracy_gpu_new_context_data, ___tracy_gpu_time_data,
    ___tracy_gpu_zone_begin_data, ___tracy_gpu_zone_end_data, ___tracy_source_location_data,
};

use crate::profiling::{gpu_enabled, tracy_zone};

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

pub fn gpu_begin(loc_data: &___tracy_source_location_data) {
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

pub fn gpu_end() {
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
