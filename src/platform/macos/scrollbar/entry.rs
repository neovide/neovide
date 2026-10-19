use objc2::rc::Retained;
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSScroller, NSScrollerPart, NSWindow, NSWindowOrderingMode,
};
use objc2_foundation::{MainThreadMarker, NSRect, NSSize};

use super::{
    appearance::{ScrollbarColors, sync_scrollbar_appearance},
    appkit::{create_scrollbar, request_display},
    interaction::{
        cancel_tracking, drag_value, is_dragging_knob, retire_tracking, with_programmatic_update,
    },
    layout::{ScrollbarSide, ScrollbarTrackMetrics, left_scrollbar_bounds_origin, scrollbar_width},
    registry::{register, set_scrollbar_colors, unregister},
    state::{ScrollbarGridState, ScrollbarThumb},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ScrollbarControlKey {
    pub grid: u64,
    pub side: ScrollbarSide,
}

#[derive(Debug, Default, PartialEq)]
struct ThumbUpdate {
    value: Option<f64>,
    proportion: Option<f64>,
    enabled: Option<bool>,
}

impl ThumbUpdate {
    fn new(
        current: ScrollbarThumb,
        enabled: bool,
        requested: Option<ScrollbarThumb>,
        dragging: bool,
    ) -> Self {
        let mut update = Self {
            enabled: (enabled != requested.is_some()).then_some(requested.is_some()),
            ..Self::default()
        };

        // appkit owns the dragged control; its counterpart can still be updated.
        if !dragging && let Some(thumb) = requested {
            update.value = (current.value != thumb.value).then_some(thumb.value);
            update.proportion =
                (current.proportion != thumb.proportion).then_some(thumb.proportion);
        }
        update
    }

    fn is_empty(&self) -> bool {
        self.value.is_none() && self.proportion.is_none() && self.enabled.is_none()
    }
}

/// Owns one appkit view and its pointer-based callback registration.
///
/// Controls on opposite sides share their grid callback data. dropping one
/// removes only that view registration, not the state used by its counterpart.
#[derive(Debug)]
pub struct ScrollbarEntry {
    scroller: Retained<NSScroller>,
    side: ScrollbarSide,
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
            // initialize the real track before its slot determines window padding.
            scroller
                .setFrameSize(NSSize::new(scrollbar_width(), content_view.bounds().size.height));
            content_view.addSubview(&scroller);
        }

        Self { scroller, side, window: None, buffer: None }
    }

    pub fn track_metrics(&self) -> ScrollbarTrackMetrics {
        ScrollbarTrackMetrics::from_slot(
            self.scroller.bounds(),
            self.scroller.rectForPart(NSScrollerPart::KnobSlot),
        )
        .unwrap_or_else(|| ScrollbarTrackMetrics { width: scrollbar_width(), trailing_inset: 0.0 })
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
        let mut changed = old_frame != frame;
        if changed {
            with_programmatic_update(&self.scroller, || self.scroller.setFrame(frame));
        }

        if self.side == ScrollbarSide::Left {
            let bounds = self.scroller.bounds();
            let slot = self.scroller.rectForPart(NSScrollerPart::KnobSlot);
            let origin = left_scrollbar_bounds_origin(bounds, slot);
            if bounds.origin != origin {
                with_programmatic_update(&self.scroller, || self.scroller.setBoundsOrigin(origin));
                changed = true;
            }
        }

        if changed {
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
        if thumb.is_none() {
            cancel_tracking(&self.scroller);
        }

        // Native tracking can change these values between renderer updates.
        let current = ScrollbarThumb {
            value: self.scroller.doubleValue(),
            proportion: self.scroller.knobProportion(),
        };
        let update = ThumbUpdate::new(
            current,
            self.scroller.isEnabled(),
            thumb,
            is_dragging_knob(&self.scroller),
        );
        if update.is_empty() {
            return;
        }

        with_programmatic_update(&self.scroller, || {
            if let Some(value) = update.value {
                self.scroller.setDoubleValue(value);
            }
            if let Some(proportion) = update.proportion {
                self.scroller.setKnobProportion(proportion);
            }
            if let Some(enabled) = update.enabled {
                self.scroller.setEnabled(enabled);
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
