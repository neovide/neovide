use nix::poll::{PollFd, PollFlags};
use std::{
    os::fd::AsRawFd,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use winit::platform::wayland::WindowExtWayland;

use wayland_backend::sys::client::Backend;
use wayland_client::{
    backend::ObjectId, protocol::wl_callback::WlCallback, protocol::wl_surface::WlSurface,
    Connection, Dispatch, EventQueue, Proxy, QueueHandle,
};
use wayland_sys::client::{wl_display, wl_proxy};

use crate::renderer::WindowedContext;

struct VSyncDispatcher {
    vsync_signaled: Arc<AtomicBool>,
}

pub struct VSyncWayland {
    wl_surface: WlSurface,
    event_queue: EventQueue<VSyncDispatcher>,
    event_queue_handle: QueueHandle<VSyncDispatcher>,
    dispatcher: VSyncDispatcher,
    vsync_signaled: Arc<AtomicBool>,
}

impl VSyncWayland {
    pub fn new(_vsync_enabled: bool, context: &WindowedContext) -> Self {
        let window = context.window();

        let surface = window
            .wayland_surface()
            .expect("Failed to get the wayland surface of the window")
            as *mut wl_proxy;

        let interface = WlSurface::interface();

        let id = unsafe { ObjectId::from_ptr(interface, surface) }
            .expect("Failed to get wayland surface id");

        let display = window
            .wayland_display()
            .expect("Failed to get the wayland display of the window")
            as *mut wl_display;

        let backend = unsafe { Backend::from_foreign_display(display) };
        let conn = Connection::from_backend(backend);
        let event_queue = conn.new_event_queue::<VSyncDispatcher>();
        let wl_surface =
            <WlSurface as Proxy>::from_id(&conn, id).expect("Failed to create wl_surface proxy");
        let vsync_signaled = Arc::new(AtomicBool::new(true));
        let dispatcher = VSyncDispatcher {
            vsync_signaled: vsync_signaled.clone(),
        };
        let event_queue_handle = event_queue.handle();

        Self {
            wl_surface,
            event_queue,
            event_queue_handle,
            dispatcher,
            vsync_signaled,
        }
    }

    pub fn wait_for_vsync(&mut self) {
        while !self.vsync_signaled.load(Ordering::Relaxed) {
            self.event_queue.flush().unwrap();
            let read_guard = self.event_queue.prepare_read().unwrap();
            if self
                .event_queue
                .dispatch_pending(&mut self.dispatcher)
                .unwrap()
                == 0
            {
                let mut fds = [PollFd::new(
                    read_guard.connection_fd().as_raw_fd(),
                    PollFlags::POLLIN | nix::poll::PollFlags::POLLERR,
                )];

                let n = loop {
                    match nix::poll::poll(&mut fds, 100) {
                        Ok(n) => break n,
                        Err(nix::errno::Errno::EINTR) => continue,
                        Err(_) => break 0,
                    }
                };
                if n > 0 {
                    read_guard.read().unwrap();
                } else {
                    break;
                }
            }
        }
        self.vsync_signaled.store(false, Ordering::Relaxed);
        let _callback = self.wl_surface.frame(&self.event_queue_handle, ());
    }
}

impl Dispatch<WlCallback, ()> for VSyncDispatcher {
    fn event(
        state: &mut Self,
        _proxy: &WlCallback,
        _event: <WlCallback as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        state.vsync_signaled.store(true, Ordering::Relaxed);
    }
}
