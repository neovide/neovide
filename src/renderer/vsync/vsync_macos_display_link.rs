use log::{error, trace, warn};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use winit::{event_loop::EventLoopProxy, window::Window};

use crate::window::{EventPayload, UserEvent};

use std::{ffi::c_void, marker::PhantomPinned, pin::Pin, ptr::NonNull};

use crate::{platform::macos::get_ns_window, profiling::tracy_zone};

use objc2_core_foundation::CFRetained;
use objc2_core_graphics::CGDirectDisplayID;
// CVDisplayLink* functions are deprecated in the latest version of macOS in favor
// of new APIs of handling display link. However, all old versions of macOS do not
// support them. So we still use these old APIs.
#[allow(deprecated)]
use objc2_core_video::{
    kCVReturnDisplayLinkAlreadyRunning, kCVReturnSuccess, CVDisplayLink,
    CVDisplayLinkCreateWithCGDisplay, CVDisplayLinkSetOutputCallback, CVDisplayLinkStart,
    CVDisplayLinkStop, CVReturn, CVTimeStamp,
};
use objc2_foundation::{ns_string, NSNumber};

// Here is the doc about how to do this. https://developer.apple.com/documentation/appkit/nsscreen/1388360-devicedescription?language=objc
pub fn get_display_id_of_window(window: &Window) -> Option<CGDirectDisplayID> {
    let ns_window = get_ns_window(window);
    let screen = ns_window.screen()?;
    let description = screen.deviceDescription();
    let display_id_ns_number = description
        .objectForKey(ns_string!("NSScreenNumber"))?
        .downcast::<NSNumber>();
    if let Ok(ns_number) = display_id_ns_number {
        Some(ns_number.unsignedIntValue())
    } else {
        error!("Failed to get display id of the window.");
        None
    }
}

struct MacosDisplayLinkCallbackContext {
    proxy: EventLoopProxy<EventPayload>,
    redraw_requested: Arc<AtomicBool>,
    window_id: winit::window::WindowId,
    _pin: PhantomPinned,
}

#[allow(unused_variables, non_snake_case)]
unsafe extern "C-unwind" fn display_link_callback(
    displayLink: NonNull<CVDisplayLink>,
    inNow: NonNull<CVTimeStamp>,
    inOutputTime: NonNull<CVTimeStamp>,
    flagsIn: u64,
    flagsOut: NonNull<u64>,
    displayLinkContext: *mut c_void,
) -> CVReturn {
    tracy_zone!("VSyncDisplayLinkCallback");

    // The display link should be dropped before vsync, so this should be safe.
    let context = unsafe { &mut *(displayLinkContext as *mut MacosDisplayLinkCallbackContext) };

    if context.redraw_requested.swap(false, Ordering::Relaxed) {
        let _ = context.proxy.send_event(EventPayload::new(
            UserEvent::RedrawRequested,
            context.window_id,
        ));
    }

    kCVReturnSuccess
}
pub struct VSyncMacosDisplayLink {
    redraw_requested: Arc<AtomicBool>,
    // CGDirectDisplayID is used to save the display id of the display link.
    display_link: Option<(CFRetained<CVDisplayLink>, CGDirectDisplayID)>,
    // The context must be pinned since it is passed as a pointer to callback. If it moves, the pointer will be dangling.
    context: Pin<Box<MacosDisplayLinkCallbackContext>>,
}

impl VSyncMacosDisplayLink {
    pub fn new(window: &Window, proxy: EventLoopProxy<EventPayload>) -> VSyncMacosDisplayLink {
        let redraw_requested = Arc::new(AtomicBool::new(false));

        let context = Box::pin(MacosDisplayLinkCallbackContext {
            proxy,
            redraw_requested: redraw_requested.clone(),
            window_id: window.id(),
            _pin: PhantomPinned,
        });

        let mut vsync = Self {
            redraw_requested,
            display_link: None,
            context,
        };

        vsync.create_and_start_display_link(window);

        vsync
    }

    fn create_and_start_display_link(&mut self, window: &Window) {
        if let Some(display_id) = get_display_id_of_window(window) {
            unsafe {
                let mut display_link_ptr: *mut CVDisplayLink = std::ptr::dangling_mut();

                #[allow(deprecated)]
                let return_code = CVDisplayLinkCreateWithCGDisplay(
                    display_id,
                    NonNull::from(&mut display_link_ptr),
                );

                if return_code != kCVReturnSuccess {
                    error!("Failed to create display link, CVReturn code: {return_code}.");
                    return;
                }

                let display_link = CFRetained::from_raw(NonNull::new_unchecked(display_link_ptr));

                #[allow(deprecated)]
                let return_code = CVDisplayLinkSetOutputCallback(
                    display_link.as_ref(),
                    Some(display_link_callback),
                    &*self.context as *const MacosDisplayLinkCallbackContext as *mut c_void,
                );

                if return_code != kCVReturnSuccess {
                    error!("Failed to set output callback of display link, CVReturn code: {return_code}.");
                    return;
                }

                trace!("Succeeded to create display link.");

                #[allow(deprecated)]
                let return_code = CVDisplayLinkStart(display_link.as_ref());

                match return_code {
                    #[allow(non_upper_case_globals)]
                    kCVReturnSuccess => {
                        trace!("Display link started.");
                    }
                    #[allow(non_upper_case_globals)]
                    kCVReturnDisplayLinkAlreadyRunning => {
                        warn!("Display link already started. This does not affect function. But it might be a bug.");
                    }
                    code => {
                        error!("Failed to start display link, CVReturn code: {code}.");
                        return;
                    }
                }

                self.display_link = Some((display_link, display_id));
            }
        } else {
            self.stop_display_link();
            self.display_link = None;
        }
    }

    fn stop_display_link(&mut self) {
        if let Some((display_link, _)) = self.display_link.as_ref() {
            #[allow(deprecated)]
            CVDisplayLinkStop(display_link.as_ref());
        }
    }

    pub fn wait_for_vsync(&mut self) {}

    pub fn request_redraw(&mut self) {
        self.redraw_requested.store(true, Ordering::Relaxed);
    }

    pub fn update(&mut self, window: &Window) {
        let new_display = get_display_id_of_window(window);
        if new_display != self.display_link.as_ref().map(|d| d.1) {
            trace!("Window moved to a new screen, try to re-create the display link.");
            self.create_and_start_display_link(window);
        }
    }
}

impl Drop for VSyncMacosDisplayLink {
    fn drop(&mut self) {
        self.stop_display_link();
    }
}
