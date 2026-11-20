use std::ffi::CStr;
use std::sync::OnceLock;

use objc2::msg_send;
use objc2::runtime::AnyClass;
use objc2::runtime::AnyObject;
use objc2::runtime::MethodImplementation;
use objc2::runtime::Sel;
use objc2::sel;
use objc2_app_kit::NSLayoutAttribute;
use objc2_app_kit::NSTitlebarAccessoryViewController;
use objc2_app_kit::NSWindow;
use objc2_app_kit::NSWindowStyleMask;
use objc2_app_kit::NSWindowTitleVisibility;
use objc2_foundation::NSInteger;
use objc2_foundation::NSString;

use super::appearance;
use super::layout::class_name;
use super::layout::find_descendant;
use super::layout::layout;
use super::registry;
use super::restore_group_height;

/// The callbacks belong to WinitWindow and remain installed after the first
/// inline window closes. The registry determines which windows they can change.
static HOOK_INSTALLED: OnceLock<()> = OnceLock::new();

/// WinitWindow inherits NSWindow. A callback receiver therefore points to an
/// NSWindow for its lifetime.
unsafe fn as_window(this: &AnyObject) -> &NSWindow {
    unsafe { &*(this as *const AnyObject as *const NSWindow) }
}

/// The tab accessory may arrive before its NSTabBar is attached. A direct or
/// descendant NSTabBar identifies it once attached. An empty bottom NSView is
/// treated as the placeholder seen on a nonmain window in a tab group. This is
/// structural evidence. An unrelated empty bottom accessory would also match.
fn is_tab_bar(controller: &NSTitlebarAccessoryViewController) -> bool {
    let view = controller.view();
    find_descendant(&view, "NSTabBar").is_some()
        || class_name(view.as_ref()) == "NSTabBar"
        || (controller.layoutAttribute() == NSLayoutAttribute::Bottom
            && class_name(view.as_ref()) == "NSView"
            && view.subviews().is_empty())
}

/// Keep the compact toolbar row present even before a second tab appears.
/// Native fullscreen hides an empty row, while simple fullscreen suspends it
/// through the registry. A visible tab bar still hides the configured title.
pub fn update_titlebar_visibility(window: &NSWindow) {
    let Some(configured_hidden) = registry::title_hidden(window) else { return };
    let toolbar_enabled = registry::toolbar_enabled(window).unwrap_or(false);
    let has_tabs = window.tabGroup().is_some_and(|group| group.isTabBarVisible());
    let toolbar_visible = toolbar_enabled
        && (!window.styleMask().contains(NSWindowStyleMask::FullScreen) || has_tabs);
    if let Some(toolbar) = window.toolbar()
        && toolbar.isVisible() != toolbar_visible
    {
        toolbar.setVisible(toolbar_visible);
    }

    window.setTitleVisibility(if configured_hidden || has_tabs {
        NSWindowTitleVisibility::Hidden
    } else {
        NSWindowTitleVisibility::Visible
    });

    if registry::translucent_background(window).unwrap_or(false) {
        appearance::update(window, true);
    }
}

unsafe extern "C-unwind" fn add_accessory(
    this: &AnyObject,
    _sel: Sel,
    controller: &NSTitlebarAccessoryViewController,
) {
    let tabs = registry::is_registered(this) && is_tab_bar(controller);
    if tabs {
        // AppKit reads this attribute while adding the controller. Changing it
        // afterward leaves the tab accessory in its separate row.
        controller.setLayoutAttribute(NSLayoutAttribute::Right);
        // A visible title makes AppKit collapse the tab accessory into a toolbar item.
        let window = unsafe { as_window(this) };
        window.setTitleVisibility(NSWindowTitleVisibility::Hidden);
        if let Some(toolbar) = window.toolbar() {
            toolbar.setVisible(true);
        }
    }

    let superclass = AnyClass::get(c"NSWindow").unwrap();
    let _: () = unsafe {
        msg_send![super(this, superclass), addTitlebarAccessoryViewController: controller]
    };

    if tabs {
        layout(unsafe { as_window(this) });
        schedule_layout(this);
        schedule_appearance(this);
    }
}

unsafe extern "C-unwind" fn remove_accessory(this: &AnyObject, _sel: Sel, index: NSInteger) {
    let superclass = AnyClass::get(c"NSWindow").unwrap();
    let _: () = unsafe {
        msg_send![super(this, superclass), removeTitlebarAccessoryViewControllerAtIndex: index]
    };

    if registry::is_registered(this) {
        // The last tab can remove the accessory. Restore the configured title
        // once AppKit reports that the tab bar is no longer visible.
        update_titlebar_visibility(unsafe { as_window(this) });
        schedule_layout(this);
        schedule_appearance(this);
    }
}

unsafe extern "C-unwind" fn become_main_window(this: &AnyObject, _sel: Sel) {
    let superclass = AnyClass::get(c"NSWindow").unwrap();
    let _: () = unsafe { msg_send![super(this, superclass), becomeMainWindow] };
    if registry::is_registered(this) {
        // The newly selected main window can receive an NSTabBar.
        layout(unsafe { as_window(this) });
        schedule_layout(this);
        schedule_appearance(this);
    }
}

unsafe extern "C-unwind" fn set_title(this: &AnyObject, _sel: Sel, title: &NSString) {
    let superclass = AnyClass::get(c"NSWindow").unwrap();
    let _: () = unsafe { msg_send![super(this, superclass), setTitle: title] };
    if registry::is_registered(this) {
        update_titlebar_visibility(unsafe { as_window(this) });
    }
}

unsafe extern "C-unwind" fn deferred_layout(this: &AnyObject, _sel: Sel) {
    if registry::is_registered(this) {
        let window = unsafe { as_window(this) };
        update_titlebar_visibility(window);
        layout(window);
        restore_group_height(window);
        if registry::translucent_background(window).unwrap_or(false) {
            appearance::update(window, true);
        }
        // tab attachment can change the content inset without resizing the
        // window. Redraw so the wrapper can update the grid padding.
        if let Some(content) = window.contentView() {
            content.setNeedsDisplay(true);
        }
    }
}

unsafe extern "C-unwind" fn deferred_appearance(this: &AnyObject, _sel: Sel) {
    if let Some(translucent) = registry::translucent_background(unsafe { as_window(this) }) {
        appearance::update(unsafe { as_window(this) }, translucent);
    }
}

fn schedule_appearance(window: &AnyObject) {
    // AppKit can build the tab button glass after attaching its accessory.
    unsafe {
        let _: () = msg_send![window, performSelector: sel!(neovideUpdateTabAppearance),
            withObject: std::ptr::null_mut::<AnyObject>(), afterDelay: 0.05_f64];
    }
}

fn schedule_layout(window: &AnyObject) {
    // The actual NSTabBar can attach after its accessory controller. The next
    // main run loop turn can see a view tree that the immediate pass could not.
    unsafe {
        let _: () = msg_send![window, performSelector: sel!(neovideLayoutNativeTabs),
            withObject: std::ptr::null_mut::<AnyObject>(), afterDelay: 0.0_f64];
    }
}

/// Reconcile title visibility and tab placement after a window state change.
///
/// The immediate pass uses any tab view already attached. The scheduled pass
/// tries again after AppKit has had a chance to attach or move its views.
pub fn refresh(window: &NSWindow) {
    update_titlebar_visibility(window);
    layout(window);
    schedule_layout(window.as_ref());
}

/// Install class callbacks once for all WinitWindow instances.
///
/// Adding methods to the existing class preserves each window object and its
/// KVO state. Replacing the class of a live window can corrupt the observation
/// lock. Every callback checks the registry before changing inline tab layout.
pub fn install_class_hooks() {
    HOOK_INSTALLED.get_or_init(|| {
        let winit_class = AnyClass::get(c"WinitWindow").expect("winit window class is registered");
        unsafe {
            add_method(
                winit_class,
                sel!(addTitlebarAccessoryViewController:),
                (add_accessory as unsafe extern "C-unwind" fn(_, _, _)).__imp(),
                c"v@:@",
            );
            add_method(
                winit_class,
                sel!(removeTitlebarAccessoryViewControllerAtIndex:),
                (remove_accessory as unsafe extern "C-unwind" fn(_, _, _)).__imp(),
                c"v@:q",
            );
            add_method(
                winit_class,
                sel!(becomeMainWindow),
                (become_main_window as unsafe extern "C-unwind" fn(_, _)).__imp(),
                c"v@:",
            );
            add_method(
                winit_class,
                sel!(setTitle:),
                (set_title as unsafe extern "C-unwind" fn(_, _, _)).__imp(),
                c"v@:@",
            );
            add_method(
                winit_class,
                sel!(neovideLayoutNativeTabs),
                (deferred_layout as unsafe extern "C-unwind" fn(_, _)).__imp(),
                c"v@:",
            );
            add_method(
                winit_class,
                sel!(neovideUpdateTabAppearance),
                (deferred_appearance as unsafe extern "C-unwind" fn(_, _)).__imp(),
                c"v@:",
            );
        }
    });
}

unsafe fn add_method(
    class: &AnyClass,
    selector: Sel,
    implementation: objc2::runtime::Imp,
    encoding: &CStr,
) {
    // Each selector must be absent from WinitWindow when this hook installs.
    // A failed addition means that another implementation already owns it.
    let class_ptr = class as *const AnyClass as *mut AnyClass;
    assert!(
        unsafe {
            objc2::ffi::class_addMethod(class_ptr, selector, implementation, encoding.as_ptr())
        }
        .as_bool()
    );
}
