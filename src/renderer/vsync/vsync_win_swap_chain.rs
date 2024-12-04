use std::{
    sync::mpsc::{channel, Sender},
    thread::{spawn, JoinHandle},
};

use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Threading::WaitForSingleObjectEx;

use winit::{event_loop::EventLoopProxy, window::WindowId};

use crate::{
    profiling::tracy_zone,
    window::{EventPayload, UserEvent},
};

enum Message {
    RequestRedraw,
    Quit,
}

struct SwapChainHandle {
    handle: HANDLE,
}

unsafe impl Send for SwapChainHandle {}
unsafe impl Sync for SwapChainHandle {}

pub struct VSyncWinSwapChain {
    vsync_thread: Option<JoinHandle<()>>,
    sender: Sender<Message>,
}

impl VSyncWinSwapChain {
    pub fn new(proxy: EventLoopProxy<EventPayload>, swap_chain_waitable: HANDLE) -> Self {
        let handle = SwapChainHandle {
            handle: swap_chain_waitable,
        };
        let (sender, receiver) = channel();
        let vsync_thread = spawn(move || {
            // Removing this asignment causes a build failure complaining that
            // `*mut c_void` cannot be sent between threads safely.
            #[allow(clippy::redundant_locals)]
            let handle = handle;
            while let Ok(Message::RequestRedraw) = receiver.recv() {
                tracy_zone!("wait for vblank");
                unsafe {
                    WaitForSingleObjectEx(handle.handle, 1000, true);
                }
                proxy
                    .send_event(EventPayload::new(
                        UserEvent::RedrawRequested,
                        WindowId::from(0),
                    ))
                    .ok();
            }
        });
        Self {
            vsync_thread: Some(vsync_thread),
            sender,
        }
    }

    pub fn wait_for_vsync(&mut self) {}

    pub fn request_redraw(&mut self) {
        self.sender.send(Message::RequestRedraw).ok();
    }
}

impl Drop for VSyncWinSwapChain {
    fn drop(&mut self) {
        self.sender.send(Message::Quit).ok();
        self.vsync_thread.take().unwrap().join().unwrap();
    }
}
