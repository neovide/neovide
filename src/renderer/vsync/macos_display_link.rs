use std::{ffi::c_void, pin::Pin};

use crate::profiling::tracy_zone;

use self::core_video::CVReturn;

use cocoa::{
    appkit::{NSScreen, NSWindow},
    base::{id, nil},
    foundation::{NSAutoreleasePool, NSDictionary, NSString},
};
use objc::{rc::autoreleasepool, *};

use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use winit::window::Window;

// Display link api reference: https://developer.apple.com/documentation/corevideo/cvdisplaylink?language=objc
#[allow(non_upper_case_globals, non_camel_case_types)]
pub mod core_video {
    use std::ffi::c_void;

    pub type CGDirectDisplayID = u32;

    pub type CVReturn = i32;
    pub const kCVReturnSuccess: CVReturn = 0;
    pub const kCVReturnDisplayLinkAlreadyRunning: CVReturn = -6671;
    pub const kCVReturnDisplayLinkNotRunning: CVReturn = -6672;
    // pub const kCVReturnDisplayLinkCallbacksNotSet: CVReturn = -6673;

    type SInt16 = i16;
    type UInt32 = u32;
    type uint32_t = u32;
    type int32_t = i32;
    type uint64_t = u64;
    type int64_t = i64;
    type double = f64;

    #[repr(C)]
    #[allow(non_snake_case)]
    pub struct CVSMPTETime {
        subframes: SInt16,
        subframeDivisor: SInt16,
        counter: UInt32,
        _type: UInt32,
        flags: UInt32,
        hours: SInt16,
        minutes: SInt16,
        seconds: SInt16,
        frames: SInt16,
    }

    #[repr(C)]
    #[allow(non_snake_case)]
    pub struct CVTimeStamp {
        version: uint32_t,
        videoTimeScale: int32_t,
        videoTime: int64_t,
        hostTime: uint64_t,
        rateScalar: double,
        videoRefreshPeriod: int64_t,
        smpteTime: CVSMPTETime,
        flags: uint64_t,
        reserved: uint64_t,
    }

    pub type CVDisplayLinkRef = *mut c_void;

    pub type CVDisplayLinkOutputCallback = extern "C" fn(
        displayLink: CVDisplayLinkRef,
        inNow: *const CVTimeStamp,
        inOutputTime: *const CVTimeStamp,
        flagsIn: u64,
        flagsOut: *mut u64,
        displayLinkContext: *mut c_void,
    ) -> CVReturn;

    #[link(name = "CoreVideo", kind = "framework")]
    extern "C" {
        pub fn CVDisplayLinkCreateWithCGDisplay(
            displayID: CGDirectDisplayID,
            displayLinkOut: *mut CVDisplayLinkRef,
        ) -> CVReturn;
        pub fn CVDisplayLinkRelease(displayLink: CVDisplayLinkRef);
        pub fn CVDisplayLinkStart(displayLink: CVDisplayLinkRef) -> CVReturn;
        // Because display link is destroyed directly, this function is unnecessary
        #[allow(dead_code)]
        pub fn CVDisplayLinkStop(displayLink: CVDisplayLinkRef) -> CVReturn;
        pub fn CVDisplayLinkSetOutputCallback(
            displayLink: CVDisplayLinkRef,
            callback: CVDisplayLinkOutputCallback,
            userInfo: *mut c_void,
        ) -> CVReturn;
    }
}

pub struct MacosDisplayLinkCallbackArgs {
    // some_info: ... in future
}

pub type MacosDisplayLinkCallback<UserData> = fn(&mut MacosDisplayLinkCallbackArgs, &mut UserData);

struct MacosDisplayLinkCallbackContext<UserData> {
    callback: MacosDisplayLinkCallback<UserData>,
    user_data: UserData,
}

pub struct MacosDisplayLink<UserData> {
    display_link_ref: core_video::CVDisplayLinkRef,
    // The context must be pinned since it is passed as a pointer to callback. If it moves, the pointer will be dangling.
    context: Pin<Box<MacosDisplayLinkCallbackContext<UserData>>>,
}

#[allow(unused_variables, non_snake_case)]
extern "C" fn c_callback<UserData>(
    displayLink: core_video::CVDisplayLinkRef,
    inNow: *const core_video::CVTimeStamp,
    inOutputTime: *const core_video::CVTimeStamp,
    flagsIn: u64,
    flagsOut: *mut u64,
    displayLinkContext: *mut c_void,
) -> core_video::CVReturn {
    tracy_zone!("VSyncDisplayLinkCallback");

    // The display link should be dropped before vsync, so this should be safe.
    let context =
        unsafe { &mut *(displayLinkContext as *mut MacosDisplayLinkCallbackContext<UserData>) };

    let mut args = MacosDisplayLinkCallbackArgs {};

    (context.callback)(&mut args, &mut context.user_data);

    core_video::kCVReturnSuccess
}

impl<UserData> MacosDisplayLink<UserData> {
    pub fn new_from_display(
        display_id: core_video::CGDirectDisplayID,
        callback: MacosDisplayLinkCallback<UserData>,
        user_data: UserData,
    ) -> Result<Self, CVReturn> {
        let mut display_link = Self {
            display_link_ref: std::ptr::null_mut(),
            context: Box::<MacosDisplayLinkCallbackContext<UserData>>::pin(
                MacosDisplayLinkCallbackContext {
                    callback,
                    user_data,
                },
            ),
        };

        unsafe {
            let result = core_video::CVDisplayLinkCreateWithCGDisplay(
                display_id,
                &mut display_link.display_link_ref,
            );

            if result != core_video::kCVReturnSuccess {
                return Err(result);
            }

            core_video::CVDisplayLinkSetOutputCallback(
                display_link.display_link_ref,
                c_callback::<UserData>,
                // Cast the display link to an unsafe pointer and pass to display link.
                &*display_link.context as *const MacosDisplayLinkCallbackContext<UserData>
                    as *mut c_void,
            );
        }

        Ok(display_link)
    }

    pub fn start(&self) -> Result<bool, CVReturn> {
        unsafe {
            let result = core_video::CVDisplayLinkStart(self.display_link_ref);

            match result {
                core_video::kCVReturnSuccess => Ok(true),
                core_video::kCVReturnDisplayLinkAlreadyRunning => Ok(false),
                _ => Err(result),
            }
        }
    }

    // Because display link is destroyed directly, this function is unnecessary
    #[allow(dead_code)]
    pub fn stop(&self) -> Result<bool, CVReturn> {
        unsafe {
            let result = core_video::CVDisplayLinkStop(self.display_link_ref);

            match result {
                core_video::kCVReturnSuccess => Ok(true),
                core_video::kCVReturnDisplayLinkNotRunning => Ok(false),
                _ => Err(result),
            }
        }
    }
}

impl<UserData> Drop for MacosDisplayLink<UserData> {
    fn drop(&mut self) {
        unsafe {
            core_video::CVDisplayLinkRelease(self.display_link_ref);
        }
    }
}

// Here is the doc about how to do this. https://developer.apple.com/documentation/appkit/nsscreen/1388360-devicedescription?language=objc
pub fn get_display_id_of_window(window: &Window) -> core_video::CGDirectDisplayID {
    let mut result = 0;
    autoreleasepool(|| unsafe {
        let key: id = NSString::alloc(nil)
            .init_str("NSScreenNumber")
            .autorelease();
        if let RawWindowHandle::AppKit(handle) = window.raw_window_handle() {
            let ns_window: id = handle.ns_window as id;
            let display_id_ns_number = ns_window.screen().deviceDescription().valueForKey_(key);
            result = msg_send![display_id_ns_number, unsignedIntValue];
        } else {
            // Should be impossible.
            panic!("Not an AppKitWindowHandle.")
        }
    });
    result
}
