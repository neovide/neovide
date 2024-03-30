use std::{
    collections::VecDeque,
    ffi::CString,
    mem,
    ptr::{null, null_mut},
    slice::from_raw_parts,
    sync::atomic::{AtomicU8, Ordering},
};

use winapi::{
    ctypes::c_void,
    shared::{
        dxgiformat::DXGI_FORMAT_UNKNOWN,
        dxgitype::DXGI_SAMPLE_DESC,
        minwindef::BOOL,
        ntdef::LARGE_INTEGER,
        winerror::{FAILED, SUCCEEDED},
    },
    um::{
        d3d12::{
            ID3D12CommandAllocator, ID3D12CommandList, ID3D12CommandQueue, ID3D12Device,
            ID3D12Fence, ID3D12GraphicsCommandList, ID3D12QueryHeap, ID3D12Resource,
            D3D12_COMMAND_LIST_TYPE_COPY, D3D12_COMMAND_LIST_TYPE_DIRECT,
            D3D12_CPU_PAGE_PROPERTY_UNKNOWN, D3D12_FENCE_FLAG_NONE, D3D12_HEAP_FLAG_NONE,
            D3D12_HEAP_PROPERTIES, D3D12_HEAP_TYPE_READBACK, D3D12_MEMORY_POOL_UNKNOWN,
            D3D12_QUERY_HEAP_DESC, D3D12_QUERY_TYPE_TIMESTAMP, D3D12_RANGE, D3D12_RESOURCE_DESC,
            D3D12_RESOURCE_DIMENSION_BUFFER, D3D12_RESOURCE_FLAG_NONE,
            D3D12_RESOURCE_STATE_COPY_DEST, D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
        },
        profileapi::QueryPerformanceFrequency,
    },
    ENUM, STRUCT,
};

use wio::com::ComPtr;

use tracy_client_sys::{
    ___tracy_emit_gpu_calibration_serial, ___tracy_emit_gpu_context_name,
    ___tracy_emit_gpu_new_context, ___tracy_emit_gpu_time_serial,
    ___tracy_emit_gpu_zone_begin_serial, ___tracy_emit_gpu_zone_end_serial,
    ___tracy_gpu_calibration_data, ___tracy_gpu_context_name_data, ___tracy_gpu_new_context_data,
    ___tracy_gpu_time_data, ___tracy_gpu_zone_begin_data, ___tracy_gpu_zone_end_data,
    ___tracy_source_location_data,
};

use crate::profiling::{is_connected, GpuContextType, GpuCtx};
use crate::renderer::d3d::{call_com_fn, D3DSkiaRenderer};

// The winapi crate does not expose these for some reason
ENUM! {
    enum D3d12ViewInstancingTier {
    D3D12_VIEW_INSTANCING_TIER_NOT_SUPPORTED = 0,
    D3D12_VIEW_INSTANCING_TIER_1 = 1,
    D3D12_VIEW_INSTANCING_TIER_2 = 2,
    D3D12_VIEW_INSTANCING_TIER_3 = 3,
}}
#[allow(non_camel_case_types)]
type D3D12_VIEW_INSTANCING_TIER = D3d12ViewInstancingTier;

ENUM! {
    enum D3d12CommandListSupportFlags {
    D3D12_COMMAND_LIST_SUPPORT_FLAG_NONE = 0,
    D3D12_COMMAND_LIST_SUPPORT_FLAG_DIRECT,
    D3D12_COMMAND_LIST_SUPPORT_FLAG_BUNDLE,
    D3D12_COMMAND_LIST_SUPPORT_FLAG_COMPUTE,
    D3D12_COMMAND_LIST_SUPPORT_FLAG_COPY,
    D3D12_COMMAND_LIST_SUPPORT_FLAG_VIDEO_DECODE,
    D3D12_COMMAND_LIST_SUPPORT_FLAG_VIDEO_PROCESS,
    D3D12_COMMAND_LIST_SUPPORT_FLAG_VIDEO_ENCODE,
}}
#[allow(non_camel_case_types)]
type D3D12_COMMAND_LIST_SUPPORT_FLAGS = D3d12CommandListSupportFlags;

STRUCT! {
    #[allow(non_snake_case)]
    struct D3D12_FEATURE_DATA_D3D12_OPTIONS3 {
    CopyQueueTimestampQueriesSupported: BOOL,
    CastingFullyTypedFormatSupported: BOOL,
    WriteBufferImmediateSupportFlags: D3D12_COMMAND_LIST_SUPPORT_FLAGS,
    ViewInstancingTier: D3D12_VIEW_INSTANCING_TIER,
    BarycentricsSupported: BOOL,
}}

ENUM! {
    enum D3d12Feature {
    D3D12_FEATURE_D3D12_OPTIONS = 0,
    D3D12_FEATURE_ARCHITECTURE = 1,
    D3D12_FEATURE_FEATURE_LEVELS = 2,
    D3D12_FEATURE_FORMAT_SUPPORT = 3,
    D3D12_FEATURE_MULTISAMPLE_QUALITY_LEVELS = 4,
    D3D12_FEATURE_FORMAT_INFO = 5,
    D3D12_FEATURE_GPU_VIRTUAL_ADDRESS_SUPPORT = 6,
    D3D12_FEATURE_SHADER_MODEL = 7,
    D3D12_FEATURE_D3D12_OPTIONS1 = 8,
    D3D12_FEATURE_PROTECTED_RESOURCE_SESSION_SUPPORT = 10,
    D3D12_FEATURE_ROOT_SIGNATURE = 12,
    D3D12_FEATURE_ARCHITECTURE1 = 16,
    D3D12_FEATURE_D3D12_OPTIONS2 = 18,
    D3D12_FEATURE_SHADER_CACHE = 19,
    D3D12_FEATURE_COMMAND_QUEUE_PRIORITY = 20,
    D3D12_FEATURE_D3D12_OPTIONS3 = 21,
    D3D12_FEATURE_EXISTING_HEAPS = 22,
    D3D12_FEATURE_D3D12_OPTIONS4 = 23,
    D3D12_FEATURE_SERIALIZATION = 24,
    D3D12_FEATURE_CROSS_NODE = 25,
    D3D12_FEATURE_D3D12_OPTIONS5 = 27,
    D3D12_FEATURE_DISPLAYABLE,
    D3D12_FEATURE_D3D12_OPTIONS6 = 30,
    D3D12_FEATURE_QUERY_META_COMMAND = 31,
    D3D12_FEATURE_D3D12_OPTIONS7 = 32,
    D3D12_FEATURE_PROTECTED_RESOURCE_SESSION_TYPE_COUNT = 33,
    D3D12_FEATURE_PROTECTED_RESOURCE_SESSION_TYPES = 34,
    D3D12_FEATURE_D3D12_OPTIONS8 = 36,
    D3D12_FEATURE_D3D12_OPTIONS9 = 37,
    D3D12_FEATURE_D3D12_OPTIONS10,
    D3D12_FEATURE_D3D12_OPTIONS11,
    D3D12_FEATURE_D3D12_OPTIONS12,
    D3D12_FEATURE_D3D12_OPTIONS13,
}}

ENUM! {enum D3d12QueryHeapType {
  D3D12_QUERY_HEAP_TYPE_OCCLUSION = 0,
  D3D12_QUERY_HEAP_TYPE_TIMESTAMP = 1,
  D3D12_QUERY_HEAP_TYPE_PIPELINE_STATISTICS = 2,
  D3D12_QUERY_HEAP_TYPE_SO_STATISTICS = 3,
  D3D12_QUERY_HEAP_TYPE_VIDEO_DECODE_STATISTICS = 4,
  D3D12_QUERY_HEAP_TYPE_COPY_QUEUE_TIMESTAMP = 5,
  D3D12_QUERY_HEAP_TYPE_PIPELINE_STATISTICS1,
}}

impl Default for D3D12_FEATURE_DATA_D3D12_OPTIONS3 {
    fn default() -> Self {
        unsafe { mem::zeroed() }
    }
}

static CONTEXT_ID: AtomicU8 = AtomicU8::new(0);

struct D3D12QueryPayload {
    query_id_start: u32,
    query_count: u32,
}

struct GpuCtxD3D {
    id: u8,
    _device: ComPtr<ID3D12Device>,
    queue: ComPtr<ID3D12CommandQueue>,
    query_heap: ComPtr<ID3D12QueryHeap>,
    readback_buffer: ComPtr<ID3D12Resource>,
    payload_fence: ComPtr<ID3D12Fence>,
    command_allocator: ComPtr<ID3D12CommandAllocator>,
    command_list: ComPtr<ID3D12GraphicsCommandList>,
    query_limit: u32,
    prev_calibration: u64,
    qpc_to_ns: u64,
    query_counter: u32,
    prev_counter: u32,
    payload_queue: VecDeque<D3D12QueryPayload>,
    active_payload: usize,
}

impl GpuCtxD3D {
    fn next_query_id(&mut self) -> u32 {
        let query_counter = self.query_counter;
        if self.query_counter >= self.query_limit {
            panic!("Submitted too many GPU queries! Consider increasing MAXQUERIES.")
        }
        self.query_counter += 2;
        (self.prev_counter + query_counter) % self.query_limit
    }
}

impl GpuCtxD3D {
    fn new_frame(&mut self) {
        if !is_connected() {
            return;
        }
        let query_counter = self.query_counter;
        self.query_counter = 0;
        self.payload_queue.push_back(D3D12QueryPayload {
            query_id_start: self.prev_counter,
            query_count: query_counter,
        });

        self.prev_counter += query_counter;
        if self.prev_counter >= self.query_limit {
            self.prev_counter -= self.query_limit;
        }

        self.active_payload += 1;
        unsafe {
            self.queue
                .Signal(self.payload_fence.as_raw(), self.active_payload as u64);
        }
    }
}

fn get_performance_counter_frequency() -> u64 {
    let mut t = LARGE_INTEGER::default();
    unsafe {
        QueryPerformanceFrequency(&mut t);
        *t.QuadPart() as u64
    }
}

const MAXQUERIES: u32 = 64 * 1024; // Queries are begin and end markers, so we can store half as many total time durations. Must be even!

pub fn create_d3d_gpu_context(name: &str, renderer: &D3DSkiaRenderer) -> Box<dyn GpuCtx> {
    let queue = renderer.command_queue.clone();
    let device = renderer.device.clone();
    let ctx_id = CONTEXT_ID.fetch_add(1, Ordering::Relaxed);
    let (gpu_ctx, gpu_timestamp, timestamp_frequency) = unsafe {
        if queue.GetDesc().Type == D3D12_COMMAND_LIST_TYPE_COPY {
            let mut feature_data = D3D12_FEATURE_DATA_D3D12_OPTIONS3::default();
            let p_feature_data =
                &mut feature_data as *mut D3D12_FEATURE_DATA_D3D12_OPTIONS3 as *mut c_void;
            let success = SUCCEEDED(device.CheckFeatureSupport(
                D3D12_FEATURE_D3D12_OPTIONS3,
                p_feature_data,
                mem::size_of_val(&feature_data) as u32,
            ));
            if !(success && feature_data.CopyQueueTimestampQueriesSupported != 0) {
                panic!("Platform does not support profiling of copy queues.");
            }
        }

        let mut timestamp_frequency = 0;

        if FAILED(queue.GetTimestampFrequency(&mut timestamp_frequency)) {
            panic!("Failed to get timestamp frequency.");
        }

        let mut cpu_timestamp = 0;
        let mut gpu_timestamp = 0;

        if FAILED(queue.GetClockCalibration(&mut gpu_timestamp, &mut cpu_timestamp)) {
            panic!("Failed to get queue clock calibration.");
        }

        let qpc_to_ns = 1000000000 / get_performance_counter_frequency();

        // Save the device cpu timestamp, not the profiler's timestamp.
        let prev_calibration = cpu_timestamp * qpc_to_ns;

        let mut heap_desc = D3D12_QUERY_HEAP_DESC {
            Type: if queue.GetDesc().Type == D3D12_COMMAND_LIST_TYPE_COPY {
                D3D12_QUERY_HEAP_TYPE_COPY_QUEUE_TIMESTAMP
            } else {
                D3D12_QUERY_HEAP_TYPE_TIMESTAMP
            },
            ..Default::default()
        };

        let mut query_limit = MAXQUERIES;
        heap_desc.Count = query_limit;
        heap_desc.NodeMask = 0; // #TODO: Support multiple adapters.

        let query_heap: ComPtr<ID3D12QueryHeap> = loop {
            if let Ok(query_heap) =
                call_com_fn(|query_heap, id| device.CreateQueryHeap(&heap_desc, id, query_heap))
            {
                break query_heap;
            } else {
                query_limit /= 2;
                heap_desc.Count = query_limit;
            }
        };

        // Create a readback buffer, which will be used as a destination for the query data.

        let readback_buffer_desc = D3D12_RESOURCE_DESC {
            Alignment: 0,
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Width: query_limit as u64 * mem::size_of::<u64>() as u64,
            Height: 1,
            DepthOrArraySize: 1,
            Format: DXGI_FORMAT_UNKNOWN,
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR, // Buffers are always row major.
            MipLevels: 1,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Flags: D3D12_RESOURCE_FLAG_NONE,
        };

        let readback_heap_props = D3D12_HEAP_PROPERTIES {
            Type: D3D12_HEAP_TYPE_READBACK,
            CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
            MemoryPoolPreference: D3D12_MEMORY_POOL_UNKNOWN,
            CreationNodeMask: 0,
            VisibleNodeMask: 0, // TODO: Support multiple adapters.
        };

        let readback_buffer: ComPtr<ID3D12Resource> = call_com_fn(|readback_buffer, id| {
            device.CreateCommittedResource(
                &readback_heap_props,
                D3D12_HEAP_FLAG_NONE,
                &readback_buffer_desc,
                D3D12_RESOURCE_STATE_COPY_DEST,
                null(),
                id,
                readback_buffer,
            )
        })
        .expect("Failed to create query readback buffer.");

        let payload_fence: ComPtr<ID3D12Fence> =
            call_com_fn(|fence, id| device.CreateFence(0, D3D12_FENCE_FLAG_NONE, id, fence))
                .expect("Failed to create payload fence.");

        let command_allocator: ComPtr<ID3D12CommandAllocator> = call_com_fn(|allocator, id| {
            device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT, id, allocator)
        })
        .expect("Failed to create command allocator");

        let command_list: ComPtr<ID3D12GraphicsCommandList> = call_com_fn(|command_list, id| {
            device.CreateCommandList(
                0,
                D3D12_COMMAND_LIST_TYPE_DIRECT,
                command_allocator.as_raw(),
                null_mut(),
                id,
                command_list,
            )
        })
        .expect("Failed to create command list");

        (
            Box::new(GpuCtxD3D {
                id: ctx_id,
                _device: device,
                queue,
                query_heap,
                readback_buffer,
                command_list,
                payload_fence,
                command_allocator,
                query_limit,
                prev_calibration,
                qpc_to_ns,
                query_counter: 0,
                prev_counter: 0,
                payload_queue: VecDeque::new(),
                active_payload: 0,
            }),
            gpu_timestamp,
            timestamp_frequency,
        )
    };

    enum GpuContextFlags {
        GpuContextCalibration = 1 << 0,
    }

    let period = 1E+09 / timestamp_frequency as f32;

    let ctxt_data = ___tracy_gpu_new_context_data {
        gpuTime: gpu_timestamp as i64,
        period,
        context: ctx_id,
        flags: GpuContextFlags::GpuContextCalibration as u8,
        type_: GpuContextType::Direct3D12 as u8,
    };
    let namestring = CString::new(name).unwrap();
    let name_data = ___tracy_gpu_context_name_data {
        context: ctx_id,
        name: namestring.as_ptr(),
        len: name.len() as u16,
    };
    unsafe {
        ___tracy_emit_gpu_new_context(ctxt_data);
        ___tracy_emit_gpu_context_name(name_data);
    }
    gpu_ctx
}

impl GpuCtx for GpuCtxD3D {
    fn gpu_collect(&mut self) {
        if !is_connected() {
            self.query_counter = 0;
            return;
        }

        // Find out what payloads are available.
        let newest_ready_payload = unsafe { self.payload_fence.GetCompletedValue() as usize };
        let payload_count = self.payload_queue.len() - (self.active_payload - newest_ready_payload);

        if payload_count > 0 {
            let map_range = D3D12_RANGE {
                Begin: 0,
                End: self.query_limit as usize * mem::size_of::<u64>(),
            };

            let mut readback_buffer_mapping = null_mut();
            if FAILED(unsafe {
                self.readback_buffer
                    .Map(0, &map_range, &mut readback_buffer_mapping)
            }) {
                panic!("Failed to map readback buffer.");
            }

            let timestamp_data = unsafe {
                from_raw_parts(
                    readback_buffer_mapping as *const u64,
                    self.query_limit as usize,
                )
            };

            for _ in 0..payload_count {
                if let Some(payload) = &self.payload_queue.front() {
                    for j in 0..payload.query_count {
                        let counter = (payload.query_id_start + j) % self.query_limit;
                        let timestamp = timestamp_data[counter as usize];
                        let query_id = counter;

                        let time_data = ___tracy_gpu_time_data {
                            gpuTime: timestamp as i64,
                            queryId: query_id as u16,
                            context: self.id,
                        };
                        unsafe {
                            ___tracy_emit_gpu_time_serial(time_data);
                        }
                    }
                    self.payload_queue.pop_front();
                }
            }
            unsafe {
                self.readback_buffer.Unmap(0, null());
            }

            // Recalibrate to account for drift.

            let mut cpu_timestamp = 0;
            let mut gpu_timestamp = 0;

            if FAILED(unsafe {
                self.queue
                    .GetClockCalibration(&mut gpu_timestamp, &mut cpu_timestamp)
            }) {
                panic!("Failed to get queue clock calibration.");
            }

            cpu_timestamp *= self.qpc_to_ns;

            let cpu_delta = cpu_timestamp as i64 - self.prev_calibration as i64;
            if cpu_delta > 0 {
                self.prev_calibration = cpu_timestamp;
                let calibration_data = ___tracy_gpu_calibration_data {
                    gpuTime: gpu_timestamp as i64,
                    cpuDelta: cpu_delta,
                    context: self.id,
                };
                unsafe {
                    ___tracy_emit_gpu_calibration_serial(calibration_data);
                }
            }
        }

        self.new_frame();
    }

    fn gpu_begin(&mut self, loc_data: &___tracy_source_location_data) -> i64 {
        let query = self.next_query_id();

        let gpu_data = ___tracy_gpu_zone_begin_data {
            srcloc: (loc_data as *const ___tracy_source_location_data) as u64,
            queryId: query as u16,
            context: self.id,
        };
        unsafe {
            // We don't have access to the skia command list, so we need to use our own, consisting
            // of just a single command to get the order right.
            self.command_list
                .EndQuery(self.query_heap.as_raw(), D3D12_QUERY_TYPE_TIMESTAMP, query);
            self.command_list.Close();
            let command_list = self.command_list.as_raw() as *mut ID3D12CommandList;
            self.queue.ExecuteCommandLists(1, &command_list);
            self.command_list
                .Reset(self.command_allocator.as_raw(), null_mut());
            ___tracy_emit_gpu_zone_begin_serial(gpu_data);
        }
        query as i64
    }

    fn gpu_end(&mut self, query_id: i64) {
        // TODO: Should probly flush Skia here, since it uses it's own command lists
        let end_query_id = query_id as u32 + 1;

        let gpu_data = ___tracy_gpu_zone_end_data {
            queryId: end_query_id as u16,
            context: self.id,
        };
        unsafe {
            self.command_list.EndQuery(
                self.query_heap.as_raw(),
                D3D12_QUERY_TYPE_TIMESTAMP,
                end_query_id,
            );
            self.command_list.ResolveQueryData(
                self.query_heap.as_raw(),
                D3D12_QUERY_TYPE_TIMESTAMP,
                query_id as u32,
                2,
                self.readback_buffer.as_raw(),
                query_id as u64 * mem::size_of::<u64>() as u64,
            );
            self.command_list.Close();
            let command_list = self.command_list.as_raw() as *mut ID3D12CommandList;
            self.queue.ExecuteCommandLists(1, &command_list);
            self.command_list
                .Reset(self.command_allocator.as_raw(), null_mut());
            ___tracy_emit_gpu_zone_end_serial(gpu_data);
        }
    }
}
