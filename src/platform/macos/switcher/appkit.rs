use objc2::{MainThreadOnly, define_class, msg_send, rc::Retained, runtime::Sel};
use objc2_app_kit::{
    NSBackingStoreType, NSControl, NSControlTextEditingDelegate, NSEvent, NSEventType, NSTextField,
    NSTextFieldDelegate, NSTextView, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSNotification, NSObject, NSObjectProtocol, NSRect};

use super::{
    activate_editor_switcher_row, activate_selected_editor_switcher_row, close_editor_switcher,
    handle_editor_switcher_command_selector, handle_editor_switcher_key,
    handle_editor_switcher_toggle_key, update_editor_switcher_query_from_search_field,
};

define_class!(
    #[derive(Debug)]
    #[unsafe(super = NSWindow)]
    #[thread_kind = MainThreadOnly]
    pub struct EditorSwitcherWindow;

    impl EditorSwitcherWindow {
        #[unsafe(method(sendEvent:))]
        fn send_event(&self, event: &NSEvent) {
            if event.r#type() == NSEventType::KeyDown && handle_editor_switcher_toggle_key(event) {
                return;
            }

            unsafe {
                let _: () = msg_send![super(self), sendEvent: event];
            }
        }
    }
);

impl EditorSwitcherWindow {
    pub fn new(
        mtm: MainThreadMarker,
        frame: NSRect,
        style: NSWindowStyleMask,
    ) -> Retained<EditorSwitcherWindow> {
        unsafe {
            msg_send![
                Self::alloc(mtm),
                initWithContentRect: frame,
                styleMask: style,
                backing: NSBackingStoreType::Buffered,
                defer: false,
            ]
        }
    }
}

define_class!(
    #[derive(Debug)]
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    pub struct EditorSwitcherView;

    impl EditorSwitcherView {
        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            handle_editor_switcher_key(event);
        }
    }
);

impl EditorSwitcherView {
    pub fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
        unsafe { msg_send![Self::alloc(mtm), initWithFrame: frame] }
    }
}

define_class!(
    #[derive(Debug)]
    #[unsafe(super = NSTextField)]
    #[thread_kind = MainThreadOnly]
    pub struct EditorSwitcherSearchField;

    impl EditorSwitcherSearchField {
        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            if handle_editor_switcher_toggle_key(event) {
                return;
            }

            unsafe {
                let _: () = msg_send![super(self), keyDown: event];
            }
        }
    }
);

impl EditorSwitcherSearchField {
    pub fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
        unsafe { msg_send![Self::alloc(mtm), initWithFrame: frame] }
    }
}

define_class!(
    #[derive(Debug)]
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    pub struct EditorSwitcherDocumentView;

    impl EditorSwitcherDocumentView {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }
    }
);

impl EditorSwitcherDocumentView {
    pub fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
        unsafe { msg_send![Self::alloc(mtm), initWithFrame: frame] }
    }
}

define_class!(
    #[derive(Debug)]
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    pub struct EditorSwitcherActionHandler;

    impl EditorSwitcherActionHandler {
        #[unsafe(method(editorSwitcherRowClicked:))]
        fn row_clicked(&self, sender: &NSControl) {
            activate_editor_switcher_row(sender.tag() as usize);
        }

        #[unsafe(method(editorSwitcherOpenSelected:))]
        fn open_selected(&self, _sender: &NSControl) {
            activate_selected_editor_switcher_row();
        }

        #[unsafe(method(editorSwitcherClose:))]
        fn close(&self, _sender: &NSControl) {
            close_editor_switcher();
        }
    }

    unsafe impl NSObjectProtocol for EditorSwitcherActionHandler {}
);

impl EditorSwitcherActionHandler {
    pub fn new(mtm: MainThreadMarker) -> Retained<Self> {
        unsafe { msg_send![Self::alloc(mtm), init] }
    }
}

define_class!(
    #[derive(Debug)]
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    pub struct EditorSwitcherSearchDelegate;

    impl EditorSwitcherSearchDelegate {
        #[unsafe(method(controlTextDidChange:))]
        fn control_text_did_change(&self, _obj: &NSNotification) {
            update_editor_switcher_query_from_search_field();
        }

        #[unsafe(method(control:textView:doCommandBySelector:))]
        unsafe fn control_text_view_do_command_by_selector(
            &self,
            _control: &NSControl,
            _text_view: &NSTextView,
            command_selector: Sel,
        ) -> bool {
            handle_editor_switcher_command_selector(command_selector)
        }
    }

    unsafe impl NSObjectProtocol for EditorSwitcherSearchDelegate {}
    unsafe impl NSControlTextEditingDelegate for EditorSwitcherSearchDelegate {}
    unsafe impl NSTextFieldDelegate for EditorSwitcherSearchDelegate {}
);

impl EditorSwitcherSearchDelegate {
    pub fn new(mtm: MainThreadMarker) -> Retained<Self> {
        unsafe { msg_send![Self::alloc(mtm), init] }
    }
}
