//! Adjust AppKit titlebar glass for inline system tabs on *macOS 27* and later.
//!
//! WindowSystemTabs refreshes after opacity changes. AppKit callbacks retry
//! when tabs attach or the selected window becomes main, since either event
//! can replace private titlebar views. Each pass reads the current view tree.
//!
//! Recognized glass backgrounds use half alpha for translucent windows, and
//! the native tab track material is hidden. Opaque updates restore full alpha
//! and track visibility. Unfamiliar views and tab controls stay untouched.

use objc2_app_kit::NSView;
use objc2_app_kit::NSWindow;
use objc2_foundation::NSOperatingSystemVersion;
use objc2_foundation::NSProcessInfo;
use objc2_quartz_core::CALayer;
use objc2_quartz_core::CATransaction;

use super::layout::class_name;
use super::layout::find_descendant;
use super::layout::titlebar_view;

const TRANSLUCENT_ALPHA: f64 = 0.5;

pub fn update(window: &NSWindow, translucent: bool) {
    if !NSProcessInfo::processInfo().isOperatingSystemAtLeastVersion(NSOperatingSystemVersion {
        majorVersion: 27,
        minorVersion: 0,
        patchVersion: 0,
    }) {
        return;
    }

    let Some(titlebar) = titlebar_view(window) else { return };
    update_backgrounds(&titlebar, translucent);
    if let Some(tab_bar) = find_descendant(&titlebar, "NSTabBar")
        && let Some(layer) = tab_bar.layer()
    {
        // AppKit may replace the track layer while attaching a tab, so we disable
        // implicit fades as its visibility and opacity change during a merge.
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        update_track_material(&layer, translucent);
        CATransaction::commit();
    }
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
