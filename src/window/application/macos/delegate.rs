use std::ffi::CString;

use objc2::rc::Retained;
use objc2::runtime::AnyClass;
use objc2::runtime::AnyObject;
use objc2::runtime::ClassBuilder;
use objc2::sel;
use objc2_app_kit::NSApplication;
use objc2_app_kit::NSMenu;
use objc2_app_kit::NSMenuItem;
use objc2_foundation::MainThreadMarker;
use objc2_foundation::NSArray;
use objc2_foundation::NSDictionary;
use objc2_foundation::NSString;
use objc2_foundation::NSUserDefaults;
use objc2_foundation::ns_string;

use super::request_new_window;
use crate::bridge::send_or_queue_file_drop;
use crate::window::macos::appkit::activate_and_focus_existing_window;

pub fn register_file_handler() {
    fn dispatch_file_drops(filenames: &NSArray<NSString>) {
        for filename in filenames.iter() {
            send_or_queue_file_drop(filename.to_string(), None);
        }
    }

    unsafe extern "C-unwind" fn handle_create_window(
        _this: &AnyObject,
        _sel: objc2::runtime::Sel,
        _sender: &AnyObject,
    ) {
        request_new_window();
    }

    // See signature at
    // https://developer.apple.com/documentation/appkit/nsapplicationdelegate/applicationdockmenu(_:)?language=objc
    unsafe extern "C-unwind" fn handle_application_dock_menu(
        this: &AnyObject,
        _sel: objc2::runtime::Sel,
        _sender: &NSApplication,
    ) -> *mut NSMenu {
        let mtm = MainThreadMarker::new().expect("Dock menu must be built on main thread.");
        let menu = NSMenu::new(mtm);
        let create_new_window = NSMenuItem::new(mtm);
        create_new_window.setTitle(ns_string!("New Window"));
        unsafe {
            create_new_window.setAction(Some(sel!(neovideCreateWindow:)));
            create_new_window.setTarget(Some(this));
        }
        menu.addItem(&create_new_window);
        Retained::autorelease_return(menu)
    }

    // See signature at
    // https://developer.apple.com/documentation/appkit/nsapplicationdelegate/application(_:openfiles:)?language=objc
    unsafe extern "C-unwind" fn handle_open_files(
        _this: &mut AnyObject,
        _sel: objc2::runtime::Sel,
        _sender: &objc2::runtime::AnyObject,
        filenames: &NSArray<NSString>,
    ) {
        dispatch_file_drops(filenames);
        activate_and_focus_existing_window();
    }

    let mtm = MainThreadMarker::new().expect("File handler must be registered on main thread.");

    unsafe {
        let app = NSApplication::sharedApplication(mtm);
        let delegate = app.delegate().unwrap();

        // Find out class of the NSApplicationDelegate
        let class: &AnyClass = AnyObject::class(delegate.as_ref());

        // register subclass of whatever was in delegate
        let class_name = CString::new("NeovideApplicationDelegate").unwrap();
        let mut my_class = ClassBuilder::new(class_name.as_c_str(), class).unwrap();
        my_class.add_method(
            sel!(application:openFiles:),
            handle_open_files as unsafe extern "C-unwind" fn(_, _, _, _) -> _,
        );

        my_class.add_method(
            sel!(applicationDockMenu:),
            handle_application_dock_menu as unsafe extern "C-unwind" fn(_, _, _) -> _,
        );

        my_class.add_method(
            sel!(neovideCreateWindow:),
            handle_create_window as unsafe extern "C-unwind" fn(_, _, _) -> _,
        );

        let class = my_class.register();
        // this should be safe as:
        //  * our class is a subclass
        //  * no new ivars
        //  * overridden methods are compatible with old (we implement protocol method)
        AnyObject::set_class(delegate.as_ref(), class);
    }

    // Prevent AppKit from interpreting our command line.
    let keys = &[ns_string!("NSTreatUnknownArgumentsAsOpen")];
    // API requires `AnyObject[]` not `NSString[]`.
    let objects = &[ns_string!("NO") as &AnyObject];
    let dict = NSDictionary::from_slices(keys, objects);
    unsafe {
        NSUserDefaults::standardUserDefaults().registerDefaults(&dict);
    }
}
