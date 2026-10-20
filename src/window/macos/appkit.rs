use objc2::rc::Retained;
use objc2_app_kit::NSView;
use objc2_app_kit::NSWindow;
use raw_window_handle::HasWindowHandle;
use raw_window_handle::RawWindowHandle;
use winit::window::Window;

#[link(name = "Quartz", kind = "framework")]
unsafe extern "C" {}

pub fn get_ns_window(window: &Window) -> Retained<NSWindow> {
    match window.window_handle().expect("Failed to fetch window handle").as_raw() {
        RawWindowHandle::AppKit(handle) => {
            let ns_view: Retained<NSView> = unsafe {
                Retained::retain(handle.ns_view.as_ptr().cast())
                    .expect("Failed to get NSView instance.")
            };
            ns_view.window().expect("NSView was not installed in a window")
        }
        _ => panic!("Not an AppKit window"),
    }
}
