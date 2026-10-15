use objc2::{msg_send, rc::Retained};
use objc2_app_kit::{NSAutoresizingMaskOptions, NSScroller, NSWindow, NSWindowOrderingMode};
use objc2_foundation::{MainThreadMarker, NSRect};

use super::{
    appearance::{ScrollbarColors, sync_scrollbar_appearance},
    appkit::{create_scrollbar, request_display},
    interaction::{
        cancel_tracking, drag_value, is_dragging_knob, retire_tracking, with_programmatic_update,
    },
    layout::ScrollbarSide,
    registry::{register, set_scrollbar_colors, unregister},
    state::{ScrollbarGridState, ScrollbarThumb},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ScrollbarControlKey {
    pub grid: u64,
    pub side: ScrollbarSide,
}

/// Owns one appkit view and its pointer-based callback registration.
///
/// Controls on opposite sides share their grid callback data. dropping one
/// removes only that view registration, not the state used by its counterpart.
#[derive(Debug)]
pub struct ScrollbarEntry {
    scroller: Retained<NSScroller>,
    pub window: Option<u64>,
    pub buffer: Option<u64>,
}

impl ScrollbarEntry {
    pub fn control(
        window: &NSWindow,
        side: ScrollbarSide,
        state: &ScrollbarGridState,
        colors: Option<ScrollbarColors>,
        track: &ScrollbarEntry,
    ) -> Self {
        let mut entry = Self::track(window, side, colors);
        entry.window = Some(state.window);
        entry.buffer = state.buffer;
        register(&entry.scroller, state.callback.clone());
        // segment boundaries are recomputed from rows, not independently stretched by appkit.
        entry.scroller.setAutoresizingMask(NSAutoresizingMaskOptions::ViewNotSizable);
        if let Some(content_view) = window.contentView() {
            content_view.addSubview_positioned_relativeTo(
                &entry.scroller,
                NSWindowOrderingMode::Above,
                Some(&track.scroller),
            );
        }
        entry
    }

    pub fn track(window: &NSWindow, side: ScrollbarSide, colors: Option<ScrollbarColors>) -> Self {
        let mtm = MainThreadMarker::new().expect("Scrollbar must be created on the main thread.");
        let scroller = create_scrollbar(mtm);
        let horizontal_mask = match side {
            ScrollbarSide::Left => NSAutoresizingMaskOptions::ViewMaxXMargin,
            ScrollbarSide::Right => NSAutoresizingMaskOptions::ViewMinXMargin,
        };
        scroller
            .setAutoresizingMask(NSAutoresizingMaskOptions::ViewHeightSizable | horizontal_mask);
        scroller.setEnabled(false);
        sync_scrollbar_appearance(&scroller, window);
        set_scrollbar_colors(&scroller, colors);
        if let Some(content_view) = window.contentView() {
            content_view.addSubview(&scroller);
        }

        Self { scroller, window: None, buffer: None }
    }

    pub fn set_colors(&self, colors: Option<ScrollbarColors>) {
        set_scrollbar_colors(&self.scroller, colors);
        request_display(&self.scroller);
    }

    pub fn refresh_appearance(&self, window: &NSWindow) {
        sync_scrollbar_appearance(&self.scroller, window);
        request_display(&self.scroller);
    }

    pub fn set_frame(&self, window: &NSWindow, frame: NSRect) {
        let old_frame = self.scroller.frame();
        if old_frame != frame {
            with_programmatic_update(&self.scroller, || self.scroller.setFrame(frame));
            if let Some(content_view) = window.contentView() {
                content_view.setNeedsDisplayInRect(old_frame);
            }
            request_display(&self.scroller);
            window.invalidateCursorRectsForView(&self.scroller);
        }
    }

    pub fn set_visible(&self, visible: bool) {
        if !visible {
            cancel_tracking(&self.scroller);
        }
        if self.scroller.isHidden() == visible {
            with_programmatic_update(&self.scroller, || self.scroller.setHidden(!visible));
            request_display(&self.scroller);
        }
    }

    pub fn dragged_value(&self) -> Option<f64> {
        drag_value(&self.scroller)
    }

    pub fn set_thumb(&self, thumb: Option<ScrollbarThumb>) {
        with_programmatic_update(&self.scroller, || match thumb {
            Some(thumb) => {
                // appkit owns the dragged control so only its counterpart should move.
                if !is_dragging_knob(&self.scroller) {
                    unsafe {
                        let _: () = msg_send![&*self.scroller, setDoubleValue: thumb.value];
                        let _: () = msg_send![&*self.scroller, setKnobProportion: thumb.proportion];
                    }
                }
                self.scroller.setEnabled(true);
            }
            None => {
                cancel_tracking(&self.scroller);
                self.scroller.setEnabled(false);
            }
        });
        request_display(&self.scroller);
    }
}

impl Drop for ScrollbarEntry {
    fn drop(&mut self) {
        let defer_removal = retire_tracking(&self.scroller);
        unregister(&self.scroller);
        if let Some(parent) = unsafe { self.scroller.superview() } {
            parent.setNeedsDisplayInRect(self.scroller.frame());
        }
        with_programmatic_update(&self.scroller, || {
            self.scroller.setHidden(true);
            self.scroller.setEnabled(false);
            if !defer_removal {
                self.scroller.removeFromSuperview();
            }
        });
    }
}
