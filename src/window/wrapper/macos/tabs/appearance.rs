//! Adjust AppKit titlebar glass for inline system tabs on *macOS 27* and later.
//!
//! WindowSystemTabs refreshes after opacity changes. AppKit callbacks retry
//! when tabs attach or the selected window becomes main, since either event
//! can replace private titlebar views. Each pass reads the current view tree.
//!
//! An opaque window puts a solid, pass-through view behind the native toolbar
//! controls. On macOS 27, recognized glass backgrounds use half alpha
//! and the native tab track material is hidden so the color can show through.

use objc2::MainThreadOnly;
use objc2::define_class;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2_app_kit::NSAutoresizingMaskOptions;
use objc2_app_kit::NSColor;
use objc2_app_kit::NSView;
use objc2_app_kit::NSWindow;
use objc2_app_kit::NSWindowOrderingMode;
use objc2_foundation::MainThreadMarker;
use objc2_foundation::NSPoint;
use objc2_foundation::NSRect;
use objc2_quartz_core::CALayer;
use objc2_quartz_core::CATransaction;
use skia_safe::Color;

use super::layout::class_name;
use super::layout::find_descendant;
use super::layout::titlebar_view;
use super::registry;
use crate::window::macos::version::at_least;

const TRANSLUCENT_ALPHA: f64 = 0.5;
const BACKGROUND_VIEW_CLASS: &str = "SystemTabBackgroundView";

define_class!(
    #[derive(Debug)]
    #[name = "SystemTabBackgroundView"]
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    struct SystemTabBackgroundView;
    impl SystemTabBackgroundView {
        #[unsafe(method(hitTest:))]
        fn hit_test(&self, _point: NSPoint) -> *mut NSView {
            std::ptr::null_mut()
        }
    }
);

impl SystemTabBackgroundView {
    fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
        unsafe { msg_send![Self::alloc(mtm), initWithFrame: frame] }
    }
}

pub fn update(window: &NSWindow) {
    let group = window.tabGroup();
    let selected = group.as_ref().and_then(|group| group.selectedWindow());
    let source = selected.as_deref().unwrap_or(window);
    let translucent = registry::translucent_background(source)
        .or_else(|| registry::translucent_background(window))
        .unwrap_or(false);

    let color = tab_chrome_bg_color(source, window, translucent);
    update_window(window, translucent, color);

    let Some(group) = group else { return };
    for member in group.windows().iter() {
        if should_update_member_appearance(&member, window) {
            update_window(&member, translucent, color);
        }
    }
}

fn tab_chrome_bg_color(source: &NSWindow, fallback: &NSWindow, translucent: bool) -> Option<Color> {
    if translucent {
        return None;
    }

    registry::background_color(source).or_else(|| registry::background_color(fallback))
}

fn should_update_member_appearance(member: &NSWindow, window: &NSWindow) -> bool {
    !std::ptr::eq(member, window) && registry::is_registered(member)
}

fn update_window(window: &NSWindow, translucent_background: bool, color: Option<Color>) {
    let Some(titlebar) = titlebar_view(window) else { return };

    CATransaction::begin();
    CATransaction::setDisableActions(true);
    let has_color = update_toolbar_background(&titlebar, color);
    if at_least(27, 0, 0) {
        let translucent = translucent_background || has_color;
        update_backgrounds(&titlebar, translucent);
        if let Some(tab_bar) = find_descendant(&titlebar, "NSTabBar")
            && let Some(layer) = tab_bar.layer()
        {
            update_track_material(&layer, translucent);
        }
    }

    CATransaction::commit();
}

fn update_toolbar_background(titlebar: &NSView, color: Option<Color>) -> bool {
    let Some(toolbar) = find_descendant(titlebar, "NSToolbarView") else { return false };
    let Some(color) = color else {
        remove_toolbar_background(&toolbar);
        return false;
    };

    let Some(background) = find_or_install_toolbar_background(&toolbar) else { return false };
    paint_toolbar_background(&background, toolbar.bounds(), color)
}

fn remove_toolbar_background(toolbar: &NSView) {
    if let Some(background) = find_descendant(toolbar, BACKGROUND_VIEW_CLASS) {
        background.removeFromSuperview();
    }
}

fn find_or_install_toolbar_background(toolbar: &NSView) -> Option<Retained<NSView>> {
    find_descendant(toolbar, BACKGROUND_VIEW_CLASS).or_else(|| install_toolbar_background(toolbar))
}

fn paint_toolbar_background(background: &NSView, bounds: NSRect, color: Color) -> bool {
    background.setFrame(bounds);
    background.setWantsLayer(true);
    let Some(layer) = background.layer() else { return false };
    let ns_color = NSColor::colorWithSRGBRed_green_blue_alpha(
        f64::from(color.r()) / 255.0,
        f64::from(color.g()) / 255.0,
        f64::from(color.b()) / 255.0,
        1.0,
    );

    layer.setBackgroundColor(Some(&ns_color.CGColor()));
    true
}

fn install_toolbar_background(toolbar: &NSView) -> Option<Retained<NSView>> {
    let mtm = MainThreadMarker::new()?;
    let view = SystemTabBackgroundView::new(mtm, toolbar.bounds());
    view.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    toolbar.addSubview_positioned_relativeTo(&view, NSWindowOrderingMode::Below, None);
    Some(view.into_super())
}

fn update_backgrounds(view: &NSView, translucent: bool) {
    let subviews = view.subviews();
    let name = class_name(view.as_ref());
    let alpha = if translucent { TRANSLUCENT_ALPHA } else { 1.0 };
    if (name == "NSGlassContainerView" || name.ends_with("NSSubduedGlassEffectView"))
        && let Some(background) = subviews.iter().next()
        && class_name(background.as_ref()) == "NSView"
        && background.subviews().is_empty()
    {
        // the first child is an empty NSView that carries the glass.
        if background.alphaValue() != alpha {
            background.setAlphaValue(alpha);
        }
    }

    for child in subviews.iter() {
        update_backgrounds(&child, translucent);
    }
}

fn update_track_material(layer: &CALayer, translucent: bool) {
    if layer.name().is_some_and(|name| name.to_string() == "NSTabBarTrackFilterHost") {
        layer.setHidden(translucent);
        layer.setOpacity(if translucent { 0.0 } else { 1.0 });
    }

    if let Some(children) = unsafe { layer.sublayers() } {
        for child in children.iter() {
            update_track_material(&child, translucent);
        }
    }
}
