use std::{rc::Rc, sync::Arc};

use skia_safe::{
    gpu::{
        d3d::{BackendContext, TextureResourceInfo},
        surfaces::wrap_backend_render_target,
        BackendRenderTarget, DirectContext, FlushInfo, Protected, SurfaceOrigin, SyncCpu,
    },
    surface::BackendSurfaceAccess,
    Canvas, ColorSpace, ColorType, PixelGeometry, Surface, SurfaceProps, SurfacePropsFlags,
};
use windows::core::{Interface, Result, PCWSTR};
use windows::Win32::Graphics::DirectComposition::{
    DCompositionCreateDevice2, IDCompositionDevice, IDCompositionTarget, IDCompositionVisual,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_UNKNOWN,
    DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIAdapter1, IDXGIFactory2, IDXGISwapChain1, IDXGISwapChain3,
    DXGI_ADAPTER_FLAG, DXGI_ADAPTER_FLAG_SOFTWARE, DXGI_SCALING_STRETCH,
    DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT, DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
    DXGI_USAGE_RENDER_TARGET_OUTPUT,
};
use windows::Win32::Graphics::{Direct3D::D3D_FEATURE_LEVEL_11_0, Dxgi::DXGI_SWAP_CHAIN_DESC1};
use windows::Win32::Graphics::{
    Direct3D12::{
        D3D12CreateDevice, ID3D12CommandQueue, ID3D12Device, ID3D12Fence, ID3D12Resource,
        D3D12_COMMAND_LIST_TYPE_DIRECT, D3D12_COMMAND_QUEUE_DESC, D3D12_COMMAND_QUEUE_FLAG_NONE,
        D3D12_FENCE_FLAG_NONE, D3D12_RESOURCE_STATE_PRESENT,
    },
    Dxgi::DXGI_SWAP_CHAIN_FLAG,
};
#[cfg(feature = "d3d_debug")]
use windows::Win32::Graphics::{
    Direct3D12::{D3D12GetDebugInterface, ID3D12Debug},
    Dxgi::{CreateDXGIFactory2, DXGI_CREATE_FACTORY_DEBUG},
};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObjectEx, INFINITE};
use windows::Win32::{
    Foundation::{CloseHandle, HANDLE, HWND},
    Graphics::Dxgi::DXGI_PRESENT,
};
use winit::{
    event_loop::EventLoopProxy,
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::Window,
};

use super::{vsync::VSyncWinSwapChain, RendererSettings, SkiaRenderer, VSync};
#[cfg(feature = "gpu_profiling")]
use crate::profiling::{d3d::create_d3d_gpu_context, GpuCtx};
use crate::{
    profiling::{tracy_gpu_zone, tracy_zone},
    settings::Settings,
    window::EventPayload,
};

fn get_hardware_adapter(factory: &IDXGIFactory2) -> Result<IDXGIAdapter1> {
    tracy_zone!("get_hardware_adapter");
    for i in 0.. {
        let adapter = unsafe { factory.EnumAdapters1(i)? };
        let desc = unsafe { adapter.GetDesc1() }?;

        if DXGI_ADAPTER_FLAG(desc.Flags as i32).contains(DXGI_ADAPTER_FLAG_SOFTWARE) {
            continue;
        }

        unsafe {
            if D3D12CreateDevice(
                &adapter,
                D3D_FEATURE_LEVEL_11_0,
                &mut Option::<ID3D12Device>::None,
            )
            .is_ok()
            {
                return Ok(adapter);
            }
        }
    }

    // As this function returns `Ok()` when successfully enumerated all of adapters
    // or `Err()` when failed, this code will never reach here.
    unreachable!()
}

pub struct D3DSkiaRenderer {
    gr_context: DirectContext,
    swap_chain: IDXGISwapChain3,
    swap_chain_desc: DXGI_SWAP_CHAIN_DESC1,
    swap_chain_waitable: HANDLE,
    pub command_queue: ID3D12CommandQueue,
    buffers: Vec<ID3D12Resource>,
    surfaces: Vec<Surface>,
    fence_values: Vec<u64>,
    fence: ID3D12Fence,
    fence_event: HANDLE,
    frame_swapped: bool,
    frame_index: usize,
    _backend_context: BackendContext,
    #[cfg(feature = "gpu_profiling")]
    pub device: ID3D12Device,
    _adapter: IDXGIAdapter1,
    _composition_device: IDCompositionDevice,
    _target: IDCompositionTarget,
    _visual: IDCompositionVisual,
    window: Rc<Window>,

    settings: Arc<Settings>,
}

impl D3DSkiaRenderer {
    pub fn new(window: Rc<Window>, settings: Arc<Settings>) -> Self {
        tracy_zone!("D3DSkiaRenderer::new");
        #[cfg(feature = "d3d_debug")]
        let dxgi_factory: IDXGIFactory2 = unsafe {
            let mut debug_controller: Option<ID3D12Debug> = None;
            D3D12GetDebugInterface(&mut debug_controller)
                .expect("Failed to create Direct3D debug controller");

            debug_controller
                .expect("Failed to enable debug layer")
                .EnableDebugLayer();

            CreateDXGIFactory2(DXGI_CREATE_FACTORY_DEBUG).expect("Failed to create DXGI factory")
        };

        #[cfg(not(feature = "d3d_debug"))]
        let dxgi_factory: IDXGIFactory2 =
            unsafe { CreateDXGIFactory1().expect("Failed to create DXGI factory") };

        let adapter = get_hardware_adapter(&dxgi_factory)
            .expect("Failed to find any suitable Direct3D 12 adapters");

        let mut device: Option<ID3D12Device> = None;
        unsafe {
            tracy_zone!("create_device");
            D3D12CreateDevice(&adapter, D3D_FEATURE_LEVEL_11_0, &mut device)
                .expect("Failed to create a Direct3D 12 device");
        }
        let device = device.expect("Failed to create a Direct3D 12 device");

        // Describe and create the command queue.
        let queue_desc = D3D12_COMMAND_QUEUE_DESC {
            Flags: D3D12_COMMAND_QUEUE_FLAG_NONE,
            Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
            ..Default::default()
        };
        let command_queue: ID3D12CommandQueue = unsafe {
            device
                .CreateCommandQueue(&queue_desc)
                .expect("Failed to create the Direct3D command queue")
        };

        let mut size = window.inner_size();
        size.width = size.width.max(1);
        size.height = size.height.max(1);

        // Describe and create the swap chain.
        let swap_chain_desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: size.width,
            Height: size.height,
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            Stereo: false.into(),
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2,
            Scaling: DXGI_SCALING_STRETCH,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
            AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
            Flags: DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT.0 as u32,
        };

        let hwnd = if let RawWindowHandle::Win32(handle) = window
            .window_handle()
            .expect("Failed to fetch window handle")
            .as_raw()
        {
            HWND(handle.hwnd.get() as *mut _)
        } else {
            panic!("Not a Win32 window");
        };

        let swap_chain = unsafe {
            tracy_zone!("create swap_chain");
            dxgi_factory
                .CreateSwapChainForComposition(&command_queue, &swap_chain_desc, None)
                .expect("Failed to create the Direct3D swap chain")
        };

        let swap_chain: IDXGISwapChain3 =
            IDXGISwapChain1::cast(&swap_chain).expect("Failed to cast");

        unsafe {
            swap_chain
                .SetMaximumFrameLatency(1)
                .expect("Failed to set maximum frame latency");
        }
        let composition_device: IDCompositionDevice = unsafe {
            DCompositionCreateDevice2(None).expect("Could not create composition device")
        };
        let target = unsafe {
            composition_device
                .CreateTargetForHwnd(hwnd, true)
                .expect("Could not create composition target")
        };
        let visual = unsafe {
            composition_device
                .CreateVisual()
                .expect("Could not create composition visual")
        };

        unsafe {
            visual
                .SetContent(&swap_chain)
                .expect("Failed to set composition content");
            target
                .SetRoot(&visual)
                .expect("Failed to set composition root");
            composition_device
                .Commit()
                .expect("Failed to commit composition");
        }

        let swap_chain_waitable = unsafe { swap_chain.GetFrameLatencyWaitableObject() };
        if swap_chain_waitable.is_invalid() {
            panic!("Failed to get swapchain waitable object");
        }

        // use a high value to make it easier to track these in PIX
        let fence_values = vec![10000; swap_chain_desc.BufferCount as usize];
        let fence: ID3D12Fence = unsafe {
            device
                .CreateFence(fence_values[0], D3D12_FENCE_FLAG_NONE)
                .expect("Failed to create fence")
        };

        let fence_event = unsafe {
            CreateEventW(None, false, false, PCWSTR::null()).expect("Failed to create event")
        };
        let frame_index = unsafe { swap_chain.GetCurrentBackBufferIndex() as usize };

        let backend_context = BackendContext {
            adapter: adapter.clone(),
            device: device.clone(),
            queue: command_queue.clone(),
            memory_allocator: None,
            protected_context: Protected::No,
        };
        let gr_context = unsafe {
            tracy_zone!("create skia context");
            DirectContext::new_d3d(&backend_context, None).expect("Failed to create Skia context")
        };

        let mut ret = Self {
            _adapter: adapter,
            #[cfg(feature = "gpu_profiling")]
            device,
            command_queue,
            swap_chain,
            swap_chain_desc,
            swap_chain_waitable,
            gr_context,
            _backend_context: backend_context,
            buffers: Vec::new(),
            surfaces: Vec::new(),
            fence_values,
            fence,
            fence_event,
            frame_swapped: true,
            frame_index,
            _composition_device: composition_device,
            _target: target,
            _visual: visual,
            window,

            settings,
        };
        ret.setup_surfaces();

        ret
    }

    fn move_to_next_frame(&mut self) {
        if self.frame_swapped {
            tracy_gpu_zone!("move_to_next_frame");
            unsafe {
                let current_fence_value = self.fence_values[self.frame_index];

                // Schedule a Signal command in the queue.
                self.command_queue
                    .Signal(&self.fence, current_fence_value)
                    .unwrap();

                // Update the frame index.
                self.frame_index = self.swap_chain.GetCurrentBackBufferIndex() as usize;
                let old_fence_value = self.fence_values[self.frame_index];

                // If the next frame is not ready to be rendered yet, wait until it is ready.
                if self.fence.GetCompletedValue() < old_fence_value {
                    self.fence
                        .SetEventOnCompletion(old_fence_value, self.fence_event)
                        .unwrap();
                    WaitForSingleObjectEx(self.fence_event, INFINITE, false);
                }

                // Set the fence value for the next frame.
                self.fence_values[self.frame_index] = current_fence_value + 1;
                self.frame_swapped = false;
            }
        }
    }

    fn wait_for_gpu(&mut self) {
        unsafe {
            let current_fence_value = *self.fence_values.iter().max().unwrap();
            // Schedule a Signal command in the queue.
            self.command_queue
                .Signal(&self.fence, current_fence_value)
                .unwrap();

            // Wait until the fence has been processed.
            self.fence
                .SetEventOnCompletion(current_fence_value, self.fence_event)
                .unwrap();
            WaitForSingleObjectEx(self.fence_event, INFINITE, false);

            // Increment all fence values
            for v in &mut self.fence_values {
                *v = current_fence_value + 1;
            }
        }
    }

    fn setup_surfaces(&mut self) {
        tracy_zone!("setup_surfaces");
        let size = self.window.inner_size();
        let size = (
            size.width.try_into().expect("Could not convert width"),
            size.height.try_into().expect("Could not convert height"),
        );

        self.buffers.clear();
        self.surfaces.clear();
        for i in 0..self.swap_chain_desc.BufferCount {
            let buffer: ID3D12Resource = unsafe {
                self.swap_chain
                    .GetBuffer(i)
                    .expect("Could not get swapchain buffer")
            };
            self.buffers.push(buffer.clone());

            let info = TextureResourceInfo {
                resource: buffer,
                alloc: None,
                resource_state: D3D12_RESOURCE_STATE_PRESENT,
                format: self.swap_chain_desc.Format,
                sample_count: self.swap_chain_desc.SampleDesc.Count,
                level_count: 1,
                sample_quality_pattern: 0,
                protected: Protected::No,
            };

            let render_settings = self.settings.get::<RendererSettings>();

            let surface_props = SurfaceProps::new_with_text_properties(
                SurfacePropsFlags::default(),
                PixelGeometry::default(),
                render_settings.text_contrast,
                render_settings.text_gamma,
            );

            let surface = wrap_backend_render_target(
                &mut self.gr_context,
                &BackendRenderTarget::new_d3d(size, &info),
                SurfaceOrigin::TopLeft,
                ColorType::RGBA8888,
                ColorSpace::new_srgb(),
                Some(surface_props).as_ref(),
            )
            .expect("Could not create backend render target");
            self.surfaces.push(surface);
        }
        self.frame_index = unsafe { self.swap_chain.GetCurrentBackBufferIndex() as usize };
    }
}

impl SkiaRenderer for D3DSkiaRenderer {
    fn window(&self) -> Rc<Window> {
        Rc::clone(&self.window)
    }

    fn flush(&mut self) {}

    fn swap_buffers(&mut self) {
        unsafe {
            tracy_gpu_zone!("submit surface");
            // Switch the back buffer resource state to present For some reason the
            // DirectContext::flush_and_submit does not do that for us automatically.
            let buffer_index = self.swap_chain.GetCurrentBackBufferIndex() as usize;
            self.gr_context.flush_surface_with_access(
                &mut self.surfaces[buffer_index],
                BackendSurfaceAccess::Present,
                &FlushInfo::default(),
            );
            self.gr_context.submit(Some(SyncCpu::No));

            tracy_gpu_zone!("present");
            if self.swap_chain.Present(1, DXGI_PRESENT(0)).is_ok() {
                self.frame_swapped = true;
            }
        }
    }

    fn canvas(&mut self) -> &Canvas {
        // Only block the cpu when whe actually need to draw to the canvas
        if self.frame_swapped {
            self.move_to_next_frame();
        }
        self.surfaces[self.frame_index].canvas()
    }

    fn resize(&mut self) {
        // Clean up any outstanding resources in command lists
        self.gr_context.flush_submit_and_sync_cpu();

        self.wait_for_gpu();

        self.surfaces.clear();
        self.buffers.clear();

        let mut size = self.window.inner_size();
        size.width = size.width.max(1);
        size.height = size.height.max(1);

        unsafe {
            self.swap_chain
                .ResizeBuffers(
                    0,
                    size.width,
                    size.height,
                    DXGI_FORMAT_UNKNOWN,
                    DXGI_SWAP_CHAIN_FLAG(self.swap_chain_desc.Flags as i32),
                )
                .expect("Failed to resize buffers");
        }
        self.setup_surfaces();
    }

    fn create_vsync(&self, proxy: EventLoopProxy<EventPayload>) -> VSync {
        VSync::WindowsSwapChain(VSyncWinSwapChain::new(proxy, self.swap_chain_waitable))
    }

    #[cfg(feature = "gpu_profiling")]
    fn tracy_create_gpu_context(&self, name: &str) -> Box<dyn GpuCtx> {
        create_d3d_gpu_context(name, self)
    }
}

impl Drop for D3DSkiaRenderer {
    fn drop(&mut self) {
        unsafe {
            self.gr_context.release_resources_and_abandon();
            self.wait_for_gpu();
            CloseHandle(self.fence_event).unwrap();
        }
    }
}
