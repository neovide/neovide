use objc2::rc::Retained;
use objc2_app_kit::NSApplication;
use objc2_app_kit::NSView;
use objc2_app_kit::NSWindow;
use objc2_foundation::MainThreadMarker;
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

pub fn activate_app_and_focus_window(window: &NSWindow) {
    let mtm = MainThreadMarker::new().expect("Window activation must be on the main thread.");
    let app = NSApplication::sharedApplication(mtm);
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);
    window.makeKeyAndOrderFront(None);
}

pub fn activate_and_focus_existing_window() -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };

    let app = NSApplication::sharedApplication(mtm);
    app.mainWindow()
        .or_else(|| app.keyWindow())
        .or_else(|| app.windows().firstObject())
        .map(|window| activate_app_and_focus_window(&window))
        .is_some()
}
