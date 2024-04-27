use std::{
    collections::VecDeque,
    ffi::{c_void, CString},
    mem,
    ptr::null_mut,
    slice::from_raw_parts,
    sync::atomic::{AtomicU8, Ordering},
};

use windows::core::Interface;
use windows::Win32::Graphics::Direct3D12::{
    ID3D12CommandAllocator, ID3D12CommandQueue, ID3D12Device, ID3D12Fence,
    ID3D12GraphicsCommandList, ID3D12PipelineState, ID3D12QueryHeap, ID3D12Resource,
    D3D12_COMMAND_LIST_TYPE_COPY, D3D12_COMMAND_LIST_TYPE_DIRECT, D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
    D3D12_FEATURE_D3D12_OPTIONS3, D3D12_FEATURE_DATA_D3D12_OPTIONS3, D3D12_FENCE_FLAG_NONE,
    D3D12_HEAP_FLAG_NONE, D3D12_HEAP_PROPERTIES, D3D12_HEAP_TYPE_READBACK,
    D3D12_MEMORY_POOL_UNKNOWN, D3D12_QUERY_HEAP_DESC, D3D12_QUERY_HEAP_TYPE_COPY_QUEUE_TIMESTAMP,
    D3D12_QUERY_HEAP_TYPE_TIMESTAMP, D3D12_QUERY_TYPE_TIMESTAMP, D3D12_RANGE, D3D12_RESOURCE_DESC,
    D3D12_RESOURCE_DIMENSION_BUFFER, D3D12_RESOURCE_FLAG_NONE, D3D12_RESOURCE_STATE_COPY_DEST,
    D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_UNKNOWN, DXGI_SAMPLE_DESC};
use windows::Win32::System::Performance::QueryPerformanceFrequency;

use tracy_client_sys::{
    ___tracy_emit_gpu_calibration_serial, ___tracy_emit_gpu_context_name,
    ___tracy_emit_gpu_new_context, ___tracy_emit_gpu_time_serial,
    ___tracy_emit_gpu_zone_begin_serial, ___tracy_emit_gpu_zone_end_serial,
    ___tracy_gpu_calibration_data, ___tracy_gpu_context_name_data, ___tracy_gpu_new_context_data,
    ___tracy_gpu_time_data, ___tracy_gpu_zone_begin_data, ___tracy_gpu_zone_end_data,
    ___tracy_source_location_data,
};

use crate::profiling::{is_connected, GpuContextType, GpuCtx};
use crate::renderer::d3d::D3DSkiaRenderer;

static CONTEXT_ID: AtomicU8 = AtomicU8::new(0);

struct D3D12QueryPayload {
    query_id_start: u32,
    query_count: u32,
}

struct GpuCtxD3D {
    id: u8,
    _device: ID3D12Device,
    queue: ID3D12CommandQueue,
    query_heap: ID3D12QueryHeap,
    readback_buffer: ID3D12Resource,
    payload_fence: ID3D12Fence,
    command_allocator: ID3D12CommandAllocator,
    command_list: ID3D12GraphicsCommandList,
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
                .Signal(&self.payload_fence, self.active_payload as u64)
                .unwrap();
        }
    }
}

fn get_performance_counter_frequency() -> u64 {
    let mut t = 0;
    unsafe {
        QueryPerformanceFrequency(&mut t).unwrap();
    }
    t as u64
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
            let success = device
                .CheckFeatureSupport(
                    D3D12_FEATURE_D3D12_OPTIONS3,
                    p_feature_data,
                    mem::size_of_val(&feature_data) as u32,
                )
                .is_ok();
            if !(success && feature_data.CopyQueueTimestampQueriesSupported != true) {
                panic!("Platform does not support profiling of copy queues.");
            }
        }

        let timestamp_frequency = queue.GetTimestampFrequency().unwrap();

        let mut cpu_timestamp = 0;
        let mut gpu_timestamp = 0;

        if queue
            .GetClockCalibration(&mut gpu_timestamp, &mut cpu_timestamp)
            .is_err()
        {
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

        let query_heap = loop {
            let mut query_heap: Option<ID3D12QueryHeap> = None;
            if device.CreateQueryHeap(&heap_desc, &mut query_heap).is_ok() {
                break query_heap.unwrap();
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

        let mut readback_buffer: Option<ID3D12Resource> = None;
        device
            .CreateCommittedResource(
                &readback_heap_props,
                D3D12_HEAP_FLAG_NONE,
                &readback_buffer_desc,
                D3D12_RESOURCE_STATE_COPY_DEST,
                None,
                &mut readback_buffer,
            )
            .expect("Failed to create query readback buffer.");

        let payload_fence: ID3D12Fence = device
            .CreateFence(0, D3D12_FENCE_FLAG_NONE)
            .expect("Failed to create payload fence.");

        let command_allocator: ID3D12CommandAllocator = device
            .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
            .expect("Failed to create command allocator");

        let command_list: ID3D12GraphicsCommandList = device
            .CreateCommandList(
                0,
                D3D12_COMMAND_LIST_TYPE_DIRECT,
                &command_allocator,
                &ID3D12PipelineState::from_raw(null_mut()),
            )
            .expect("Failed to create command list");

        (
            Box::new(GpuCtxD3D {
                id: ctx_id,
                _device: device,
                queue,
                query_heap,
                readback_buffer: readback_buffer.unwrap(),
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
            if unsafe {
                self.readback_buffer
                    .Map(0, Some(&map_range), Some(&mut readback_buffer_mapping))
                    .is_err()
            } {
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
                self.readback_buffer.Unmap(0, None);
            }

            // Recalibrate to account for drift.

            let mut cpu_timestamp = 0;
            let mut gpu_timestamp = 0;

            if unsafe {
                self.queue
                    .GetClockCalibration(&mut gpu_timestamp, &mut cpu_timestamp)
                    .is_err()
            } {
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
                .EndQuery(&self.query_heap, D3D12_QUERY_TYPE_TIMESTAMP, query);
            self.command_list.Close().unwrap();
            let command_list = [Some(self.command_list.cast().unwrap())];
            self.queue.ExecuteCommandLists(&command_list);
            self.command_list
                .Reset(&self.command_allocator, None)
                .unwrap();
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
            self.command_list
                .EndQuery(&self.query_heap, D3D12_QUERY_TYPE_TIMESTAMP, end_query_id);
            self.command_list.ResolveQueryData(
                &self.query_heap,
                D3D12_QUERY_TYPE_TIMESTAMP,
                query_id as u32,
                2,
                &self.readback_buffer,
                query_id as u64 * mem::size_of::<u64>() as u64,
            );
            self.command_list.Close().unwrap();
            let command_list = [Some(self.command_list.cast().unwrap())];
            self.queue.ExecuteCommandLists(&command_list);
            self.command_list
                .Reset(&self.command_allocator, None)
                .unwrap();
            ___tracy_emit_gpu_zone_end_serial(gpu_data);
        }
    }
}
