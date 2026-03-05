use std::cell::RefCell;

use objc2::{MainThreadOnly, define_class, msg_send, rc::Retained, sel};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSCursor, NSEvent, NSEventTrackingRunLoopMode, NSScroller,
    NSScrollerPart,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{
    MainThreadMarker, NSDate, NSDefaultRunLoopMode, NSObject, NSPoint, NSRect, NSRunLoop, NSSize,
    NSTimer,
};

use super::{
    appearance::{draw_scrollbar_thumb, draw_scrollbar_track},
    controller::{Diagnostics, Drag, handle_scrollbar_action, send_scroll_to_for_scrollbar_value},
    geometry::point_in_rect,
    interaction::{ScrollbarTracking, drag_value, is_dragging_knob, with_programmatic_update},
    layout::Scroller,
    registry::{is_control, scrollbar_colors, scrollbar_state},
};

thread_local! {
    static SCROLLBAR_TARGET: RefCell<Option<Retained<ScrollbarTarget>>> =
        const { RefCell::new(None) };
}

define_class!(
    #[derive(Debug)]
    #[unsafe(super = NSScroller)]
    #[thread_kind = MainThreadOnly]
    pub struct ScrollbarScroller;

    impl ScrollbarScroller {
        #[unsafe(method(isOpaque))]
        fn is_opaque(&self) -> bool {
            false
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, rect: NSRect) {
            if is_control(self) {
                self.drawKnob();
            } else {
                unsafe {
                    let _: () = msg_send![super(self), drawRect: rect];
                }
            }
        }

        #[unsafe(method(resetCursorRects))]
        fn reset_cursor_rects(&self) {
            let bounds = self.bounds();
            let cursor = NSCursor::arrowCursor();
            self.addCursorRect_cursor(bounds, &cursor);
        }

        #[unsafe(method(drawKnobSlotInRect:highlight:))]
        fn draw_knob_slot_in_rect_highlight(&self, rect: NSRect, highlight: bool) {
            if is_control(self) {
                return;
            }
            if let Some(colors) = scrollbar_colors(self) {
                draw_scrollbar_track(rect, colors);
                return;
            }

            unsafe {
                let _: () = msg_send![super(self), drawKnobSlotInRect: rect, highlight: highlight];
            }
        }

        #[unsafe(method(drawKnob))]
        fn draw_knob(&self) {
            if scrollbar_state(self).is_none() {
                return;
            }

            if let Some(colors) = scrollbar_colors(self) {
                let active = is_dragging_knob(self);
                draw_scrollbar_thumb(self.rectForPart(NSScrollerPart::Knob), colors, active);
                return;
            }

            unsafe {
                let _: () = msg_send![super(self), drawKnob];
            }
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            let point = self.convertPoint_fromView(event.locationInWindow(), None);
            let hit_part = self.testPart(point);
            let knob_rect = self.rectForPart(NSScrollerPart::Knob);
            let dragging_knob =
                hit_part == NSScrollerPart::Knob || point_in_rect(point, knob_rect);
            if Diagnostics::DRAG {
                log::info!(
                    "scrollbar mouseDown dragging_knob={dragging_knob} hit_part={hit_part:?} point=({:.1},{:.1}) knob=({:.1},{:.1},{:.1},{:.1})",
                    point.x,
                    point.y,
                    knob_rect.origin.x,
                    knob_rect.origin.y,
                    knob_rect.size.width,
                    knob_rect.size.height,
                );
            }

            let Some(_tracking) = ScrollbarTracking::begin(self, dragging_knob) else {
                return;
            };

            let timer = unsafe {
                NSTimer::timerWithTimeInterval_target_selector_userInfo_repeats(
                    Drag::POLL_INTERVAL,
                    self,
                    sel!(neovideDragTick:),
                    None,
                    true,
                )
            };

            let run_loop = NSRunLoop::currentRunLoop();
            unsafe {
                run_loop.addTimer_forMode(&timer, NSEventTrackingRunLoopMode);
                let _: () = msg_send![super(self), mouseDown: event];
            }

            timer.invalidate();

            // dont turn a canceled drag into a fresh action on mouse-up
            if let Some(value) = drag_value(self) {
                send_scroll_to_for_scrollbar_value(self, value);
            }
        }

        #[unsafe(method(neovideDragTick:))]
        fn neovide_drag_tick(&self, _timer: &NSTimer) {
            if Diagnostics::DRAG {
                if let Some(value) = drag_value(self) {
                    log::info!("scrollbar drag tick value={value:.4}");
                } else {
                    log::info!("scrollbar drag tick no live value");
                }
            }
            pump_default_run_loop_once();
        }

        #[unsafe(method(setDoubleValue:))]
        fn set_double_value(&self, value: f64) {
            with_programmatic_update(self, || unsafe {
                let _: () = msg_send![super(self), setDoubleValue: value];
            });

            if is_dragging_knob(self) {
                if Diagnostics::DRAG {
                    log::info!("scrollbar setDoubleValue value={value:.4}");
                }
                send_scroll_to_for_scrollbar_value(self, value);
            }
        }

        #[unsafe(method(setFloatValue:knobProportion:))]
        fn set_float_value_knob_proportion(&self, value: f32, proportion: CGFloat) {
            with_programmatic_update(self, || unsafe {
                let _: () = msg_send![super(self), setFloatValue: value, knobProportion: proportion];
            });

            if is_dragging_knob(self) {
                let value = value as f64;
                if Diagnostics::DRAG {
                    log::info!(
                        "scrollbar setFloatValue value={value:.4} proportion={:.4}",
                        proportion
                    );
                }
                send_scroll_to_for_scrollbar_value(self, value);
            }
        }
    }
);

define_class!(
    #[derive(Debug)]
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    pub struct ScrollbarTarget;

    impl ScrollbarTarget {
        #[unsafe(method(scrollbarAction:))]
        fn scrollbar_action(&self, sender: &NSScroller) {
            handle_scrollbar_action(sender);
        }
    }
);

impl ScrollbarTarget {
    fn shared(mtm: MainThreadMarker) -> Retained<Self> {
        SCROLLBAR_TARGET.with(|cell| {
            if let Some(target) = cell.borrow().as_ref() {
                return target.clone();
            }

            let target: Retained<Self> = unsafe { msg_send![Self::alloc(mtm), init] };
            *cell.borrow_mut() = Some(target.clone());
            target
        })
    }
}

pub fn create_scrollbar(mtm: MainThreadMarker) -> Retained<NSScroller> {
    let scroller_frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0));
    let scroller: Retained<ScrollbarScroller> =
        unsafe { msg_send![ScrollbarScroller::alloc(mtm), initWithFrame: scroller_frame] };
    let scroller: Retained<NSScroller> = scroller.into_super();

    scroller.setHidden(true);
    unsafe {
        let _: () = msg_send![&*scroller, setControlSize: Scroller::CONTROL_SIZE_REGULAR];
        let _: () = msg_send![&*scroller, setScrollerStyle: Scroller::STYLE_LEGACY];
        let _: () = msg_send![&*scroller, setEnabled: true];
        let _: () = msg_send![&*scroller, setContinuous: true];
    }

    scroller.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewHeightSizable | NSAutoresizingMaskOptions::ViewMinXMargin,
    );

    let target = ScrollbarTarget::shared(mtm);
    unsafe {
        let _: () = msg_send![&*scroller, setTarget: &*target];
        let _: () = msg_send![&*scroller, setAction: sel!(scrollbarAction:)];
    }

    scroller
}

fn pump_default_run_loop_once() {
    let run_loop = NSRunLoop::currentRunLoop();
    let limit_date = NSDate::dateWithTimeIntervalSinceNow(0.0);
    let _ = unsafe { run_loop.runMode_beforeDate(NSDefaultRunLoopMode, &limit_date) };
}

pub fn request_display(scroller: &NSScroller) {
    unsafe {
        let _: () = msg_send![scroller, setNeedsDisplay: true];
    }
}
