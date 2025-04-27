use log::{error, trace, warn};
use objc2_core_video::{kCVReturnDisplayLinkAlreadyRunning, kCVReturnDisplayLinkNotRunning};
use std::{
    ffi::c_void,
    marker::PhantomPinned,
    pin::Pin,
    ptr::NonNull,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use winit::{event_loop::EventLoopProxy, window::Window};

use crate::window::UserEvent;

use crate::{profiling::tracy_zone, window::macos::get_ns_window};

use objc2::rc::Retained;
use objc2_core_graphics::CGDirectDisplayID;
use objc2_foundation::{ns_string, NSNumber};
// Though many APIs here are deprecated by Apple, which are also marked by objc2 correspondingly,
// the new APIs to replace them are only supported in very new version of macOS. Using them
// would break old macOS versions.
#[allow(deprecated)]
use objc2_core_video::{
    kCVReturnSuccess, CVDisplayLink, CVOptionFlags, CVReturn,
    CVTimeStamp,
};

// Display link api reference: https://developer.apple.com/documentation/corevideo/cvdisplaylink?language=objc
fn check_cvreturn(value: CVReturn) -> Result<(), CVReturn> {
    match value {
        kCVReturnSuccess => Ok(()),
        _ => Err(value),
    }
}

// Here is the doc about how to do this. https://developer.apple.com/documentation/appkit/nsscreen/1388360-devicedescription?language=objc
fn get_display_id_of_window(window: &Window) -> Option<CGDirectDisplayID> {
    let ns_window = get_ns_window(window);
    let screen = ns_window.screen()?; // window may be offscreen
    let desc = screen.deviceDescription();
    let display_id: Retained<NSNumber> = unsafe {
        Retained::cast_unchecked(
            desc.objectForKey(ns_string!("NSScreenNumber"))
                .expect("Failed to get CGDirectDisplayID of a NSScreen"),
        )
    };
    Some(display_id.unsignedIntValue() as CGDirectDisplayID)
}

struct DisplayLinkCallbackContext {
    proxy: EventLoopProxy<UserEvent>,
    redraw_requested: Arc<AtomicBool>,
    // enable this struct to be pinned
    _pin: PhantomPinned,
}

// See signature at
// https://developer.apple.com/documentation/corevideo/cvdisplaylinkoutputcallback?language=objc
unsafe extern "C-unwind" fn display_link_callback<UserData>(
    _displayLink: NonNull<CVDisplayLink>,
    _inNow: NonNull<CVTimeStamp>,
    _inOutputTime: NonNull<CVTimeStamp>,
    _flagsIn: CVOptionFlags,
    _flagsOut: NonNull<CVOptionFlags>,
    displayLinkContext: *mut c_void,
) -> CVReturn {
    tracy_zone!("VSyncDisplayLinkCallback");

    // The display link should be dropped before vsync, so this should be safe.
    let context = displayLinkContext
        .cast::<DisplayLinkCallbackContext>()
        .as_mut()
        .expect("DisplayLinkContext ptr is null.");

    if context.redraw_requested.swap(false, Ordering::Relaxed) {
        let _ = context.proxy.send_event(UserEvent::RedrawRequested);
    }

    kCVReturnSuccess
}

struct DisplayLink {
    display_id: CGDirectDisplayID,
    display_link: Retained<CVDisplayLink>,
    context: *mut DisplayLinkCallbackContext,
}

impl DisplayLink {
    fn new(
        display_id: CGDirectDisplayID,
        context: *mut DisplayLinkCallbackContext,
    ) -> Result<DisplayLink, CVReturn> {
        unsafe {
            let mut display_link: *mut CVDisplayLink;

            check_cvreturn(CVDisplayLink::create_with_cg_display(
                display_id,
                NonNull::new_unchecked(&mut display_link),
            ))?;

            let display_link = Retained::from_raw(display_link).unwrap();

            check_cvreturn(
                display_link.set_output_callback(Some(display_link_callback), context.cast()),
            )?;

            Ok(DisplayLink {
                display_id,
                display_link,
                context,
            })
        }
    }

    fn display_id(display_link: &Option<DisplayLink>) -> Option<CGDirectDisplayID> {
        if let Some(display_link) = display_link {
            Some(display_link.display_id)
        } else {
            None
        }
    }

    fn start(&self) -> Result<bool, CVReturn> {
        unsafe {
            let r = self.display_link.start();
            match r {
                kCVReturnSuccess => Ok(true),
                kCVReturnDisplayLinkAlreadyRunning => Ok(false),
                _ => Err(r),
            }
        }
    }

    // Because display link is destroyed directly, this function is unnecessary
    #[allow(dead_code)]
    fn stop(&self) -> Result<bool, CVReturn> {
        unsafe {
            let r = self.display_link.stop();

            match r {
                kCVReturnSuccess => Ok(true),
                kCVReturnDisplayLinkNotRunning => Ok(false),
                _ => Err(r),
            }
        }
    }
}

pub struct VSyncMacosDisplayLink {
    display_link: Option<DisplayLink>,
    context: Pin<Box<DisplayLinkCallbackContext>>,
}

impl VSyncMacosDisplayLink {
    pub fn new(window: &Window, proxy: EventLoopProxy<UserEvent>) -> VSyncMacosDisplayLink {
        let redraw_requested = AtomicBool::new(false).into();
        let mut vsync = VSyncMacosDisplayLink {
            display_link: None,
            context: Box::pin(DisplayLinkCallbackContext {
                proxy,
                redraw_requested,
                _pin: PhantomPinned,
            }),
        };

        vsync.update(window);
        vsync
    }


    pub fn wait_for_vsync(&mut self) {}

    pub fn request_redraw(&mut self) {
        self.context.redraw_requested.store(true, Ordering::Relaxed);
    }

    pub fn update(&mut self, window: &Window) {
        let new_display_id = get_display_id_of_window(window);

        if self.display_link.as_ref().map(|dl| dl.display_id) == new_display_id {
            return;
        }

        self.display_link = None;

        if let Some(display_id) = new_display_id {
            match DisplayLink::new(display_id, &raw mut *self.context) {
                Ok(new_link) => {
                    trace!("Succeeded to create display link.");

                    match new_link.start() {
                        Ok(did) => match did {
                            true => 
                                trace!("Display link started."),
                            false => 
                                warn!("Display link already started. This does not affect function. But it might be a bug.")
                        },
                        Err(code) => 
                            error!("Failed to start display link, CVReturn code: {}.", code)
                        
                    };
                }
                Err(code) => {
                    // TODO: add some logic to limit retry count.
                    warn!("Failed to create display link, CVReturn code: {}.", code);
                    return;
                }
            };
        }
    }
}
