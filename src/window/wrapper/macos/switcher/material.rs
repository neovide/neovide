use objc2::MainThreadOnly;
use objc2::rc::Retained;
use objc2::runtime::AnyClass;
use objc2::sel;
use objc2_app_kit::NSAppearance;
use objc2_app_kit::NSAppearanceCustomization;
use objc2_app_kit::NSAppearanceNameAccessibilityHighContrastDarkAqua;
use objc2_app_kit::NSAppearanceNameDarkAqua;
use objc2_app_kit::NSColor;
use objc2_app_kit::NSGlassEffectView;
use objc2_app_kit::NSGlassEffectViewStyle;
use objc2_app_kit::NSView;
use objc2_app_kit::NSWindow;
use objc2_app_kit::NSWorkspace;
use objc2_app_kit::NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification;
use objc2_foundation::MainThreadMarker;
use objc2_foundation::NSRect;

use super::layout::Panel;
use crate::window::macos::version::at_least;

const BOTTOM_BAR_ALPHA: f64 = 0.10;
const CLEAR_DIMMING_ALPHA: f64 = 0.30;

fn glass_style(value: &str) -> NSGlassEffectViewStyle {
    match value {
        "clear" => NSGlassEffectViewStyle::Clear,
        "regular" => NSGlassEffectViewStyle::Regular,
        _ => {
            log::warn!("Unknown switcher glass style {value:?}; using regular");
            NSGlassEffectViewStyle::Regular
        }
    }
}

fn clear_dimming_alpha(value: &str) -> f64 {
    match value.parse::<f64>() {
        Ok(alpha) if (0.0..=1.0).contains(&alpha) => alpha,
        _ => {
            log::warn!("Invalid switcher glass dimming {value:?}; using {CLEAR_DIMMING_ALPHA}");
            CLEAR_DIMMING_ALPHA
        }
    }
}

pub fn is_clear(glass: Option<&NSGlassEffectView>) -> bool {
    glass.is_some_and(|glass| glass.style() == NSGlassEffectViewStyle::Clear)
}

pub fn window_glass_view(window: &NSWindow) -> Option<Retained<NSGlassEffectView>> {
    AnyClass::get(c"NSGlassEffectView")?;
    window.contentView()?.downcast().ok()
}

pub fn update_clear_appearance(glass: &NSGlassEffectView) {
    let high_contrast = NSWorkspace::sharedWorkspace().accessibilityDisplayShouldIncreaseContrast();
    let appearance_name = unsafe {
        if high_contrast {
            NSAppearanceNameAccessibilityHighContrastDarkAqua
        } else {
            NSAppearanceNameDarkAqua
        }
    };

    glass.setAppearance(NSAppearance::appearanceNamed(appearance_name).as_deref());
}

pub fn background_color(has_glass: bool) -> Retained<NSColor> {
    if has_glass { NSColor::clearColor() } else { NSColor::controlBackgroundColor() }
}

pub fn bottom_bar_background_color(has_glass: bool) -> Retained<NSColor> {
    if has_glass {
        NSColor::clearColor()
    } else {
        NSColor::blackColor().colorWithAlphaComponent(BOTTOM_BAR_ALPHA)
    }
}

pub fn glass_view(
    mtm: MainThreadMarker,
    frame: NSRect,
    style: &str,
) -> Option<Retained<NSGlassEffectView>> {
    if !at_least(26, 0, 0) || AnyClass::get(c"NSGlassEffectView").is_none() {
        return None;
    }

    let glass = NSGlassEffectView::initWithFrame(NSGlassEffectView::alloc(mtm), frame);
    glass.setStyle(glass_style(style));
    glass.setCornerRadius(Panel::GLASS_CORNER_RADIUS);

    Some(glass)
}

pub fn install_content_view(
    window: &NSWindow,
    content_view: &NSView,
    glass: Option<Retained<NSGlassEffectView>>,
    dimming: &str,
) {
    let Some(glass) = glass else {
        window.setContentView(Some(content_view));
        return;
    };

    let clear_glass = is_clear(Some(&glass));
    if clear_glass {
        configure_clear_glass_content(&glass, content_view, dimming);
    }

    // NOTE: the window retains the glass and the glass retains its content, so the
    // local glass handle can drop after installation. The appearance callback finds
    // the glass through `fn window_glass_view` on each notification, so both views
    // must be attached before registering the Clear appearance observer.
    glass.setContentView(Some(content_view));
    window.setContentView(Some(glass.as_ref()));

    if clear_glass {
        register_clear_appearance_observer(window);
    }
}

fn configure_clear_glass_content(glass: &NSGlassEffectView, content_view: &NSView, dimming: &str) {
    // prepares the 'clear glass' mode for the switcher before the content is attached. Its
    // bright controls use a dark appearance with the current macOS 'increase contrast'
    // setting, while the editor keeps its own theme outside the glass subtree.
    update_clear_appearance(glass);
    content_view.setWantsLayer(true);
    let Some(layer) = content_view.layer() else {
        return;
    };

    let background = NSColor::blackColor().colorWithAlphaComponent(clear_dimming_alpha(dimming));
    layer.setBackgroundColor(Some(&background.CGColor()));
    layer.setCornerRadius(Panel::GLASS_CORNER_RADIUS);
}

fn register_clear_appearance_observer(window: &NSWindow) {
    let notification_center = NSWorkspace::sharedWorkspace().notificationCenter();

    // The center's zeroing reference does not extend the window's lifetime.
    // SAFETY: installation calls this once on an EditorSwitcherWindow, whose
    // selector accepts a notification and reads the already-installed glass.
    unsafe {
        notification_center.addObserver_selector_name_object(
            window,
            sel!(editorSwitcherAccessibilityChanged:),
            Some(NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification),
            None,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switcher_glass_styles_and_fallback() {
        assert_eq!(glass_style("clear"), NSGlassEffectViewStyle::Clear);
        for value in ["regular", "unknown", ""] {
            assert_eq!(glass_style(value), NSGlassEffectViewStyle::Regular);
        }
    }

    #[test]
    fn switcher_glass_dimming_range_and_fallback() {
        for (value, expected) in [("0", 0.0), ("0.30", 0.30), ("1", 1.0)] {
            assert_eq!(clear_dimming_alpha(value), expected);
        }
        for value in ["-0.1", "1.1", "NaN", "inf", "-inf", "invalid", ""] {
            assert_eq!(clear_dimming_alpha(value), CLEAR_DIMMING_ALPHA, "{value}");
        }
    }
}
