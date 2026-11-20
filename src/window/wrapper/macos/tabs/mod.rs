//! Place AppKit system tabs beside the macOS window buttons.
//!
//! For full or transparent frames that request system tabs, `MacosWindowFeature`
//! creates a `WindowSystemTabs` before native tabbing starts and windows merge.
//! Simple fullscreen defers creation until the window resumes native tabbing.
//! AppKit still owns the tab group, tab selection, and the native `NSTabBar`.
//!
//! Each owner keeps a compact toolbar row visible in a normal window before
//! another tab appears. AppKit places titlebar accessories below the titlebar
//! by default, so the hook marks the tab accessory for placement on the right
//! before attachment. Layout constrains the attached tab view to the toolbar.
//!
//! Winit creates each Neovide window on macOS as `WinitWindow`, and the first
//! owner adds callbacks to that class for every instance. The registry limits
//! their layout and title changes to registered windows until their owners drop.
//!
//! AppKit may attach the `NSTabBar` after its accessory controller or to another
//! window when the selected tab changes. Adding the tab accessory or becoming
//! main tries layout at once and schedules a later pass. Removing an accessory
//! updates title visibility and schedules only the later layout pass, while size,
//! focus, and appearance changes call `WindowSystemTabs::refresh` to retry both.

mod appearance;
mod hooks;
mod layout;
mod registry;

use objc2::MainThreadOnly;
use objc2::rc::Retained;
use objc2_app_kit::NSToolbar;
use objc2_app_kit::NSView;
use objc2_app_kit::NSWindow;
use objc2_app_kit::NSWindowTabGroup;
use objc2_app_kit::NSWindowToolbarStyle;
use objc2_foundation::MainThreadMarker;
use objc2_foundation::NSPoint;
use objc2_foundation::ns_string;

use super::MacosWindowFeature;
use crate::cmd_line::CmdLineSettings;
use crate::frame::Frame;
use crate::window::WindowSettings;

/// Owns inline AppKit tab placement for one OS window.
///
/// The owner retains its toolbar across simple fullscreen, which temporarily
/// removes it from the borderless window. It keeps the window registered for
/// class callbacks until the owner drops.
#[derive(Debug)]
pub struct WindowSystemTabs {
    window: Retained<NSWindow>,
    toolbar: Retained<NSToolbar>,
}

pub fn supports_system_tabs(frame: Frame) -> bool {
    matches!(frame, Frame::Full | Frame::Transparent)
}

impl WindowSystemTabs {
    pub fn install(
        window: Retained<NSWindow>,
        mtm: MainThreadMarker,
        hidden: bool,
        host_content_height: Option<f64>,
    ) -> Self {
        hooks::install_class_hooks();
        let toolbar =
            NSToolbar::initWithIdentifier(NSToolbar::alloc(mtm), ns_string!("NeovideInlineTabs"));
        let tabs = Self { window, toolbar };
        registry::register(&tabs.window, hidden, host_content_height);
        tabs.window.setToolbar(Some(&tabs.toolbar));
        tabs.window.setToolbarStyle(NSWindowToolbarStyle::UnifiedCompact);
        tabs.toolbar.setVisible(true);
        tabs
    }

    pub fn refresh(&self) { hooks::refresh(&self.window); }

    pub fn set_translucent_background(&self, translucent: bool) {
        let was_translucent = registry::set_translucent_background(&self.window, translucent);
        if translucent || was_translucent {
            appearance::update(&self.window, translucent);
        }
    }

    pub fn set_toolbar_enabled(&self, enabled: bool) {
        registry::set_toolbar_enabled(&self.window, enabled);
        // We hide the toolbar while it is attached to the window, then update
        // title visibility once more after AppKit has removed the toolbar.
        if !enabled {
            hooks::update_titlebar_visibility(&self.window);
            self.window.setToolbar(None);
            hooks::update_titlebar_visibility(&self.window);
            return;
        }

        self.toolbar.setVisible(true);
        self.window.setToolbar(Some(&self.toolbar));
        self.window.setToolbarStyle(NSWindowToolbarStyle::UnifiedCompact);
        hooks::update_titlebar_visibility(&self.window);
    }

    /// Discard the one-time host height when the new window did not join it.
    pub fn cancel_host_height_restore(&self) {
        registry::clear_pending_content_height(&self.window);
    }

    /// Try to restore the host height after activation, when AppKit may still
    /// be attaching the new tab view. Retry after layout if it is not ready.
    pub fn restore_host_height_after_activation(&self) {
        if registry::pending_content_height(&self.window).is_some() {
            restore_group_height(&self.window);
            if registry::pending_content_height(&self.window).is_some() {
                hooks::refresh(&self.window);
            }
        }
    }

    /// Reserve toolbar space while tabs are visible or the toolbar is empty.
    /// Prefer the selected tab inset, falling back to the largest measurable
    /// group inset. Without a visible tab bar, use the empty toolbar with the
    /// same clip offset that layout applies to an attached system tab view.
    pub fn chrome_inset_in_pixels(&self) -> Option<u32> {
        let points = match self.window.tabGroup() {
            Some(group) if group.isTabBarVisible() => tab_group_inset_in_points(&group)?,
            _ => visible_toolbar_inset_in_points(&self.window)?,
        };

        let scale = self.window.backingScaleFactor();
        (points.is_finite() && scale.is_finite() && scale > 0.0)
            .then_some((points * scale).ceil() as u32)
    }

    pub fn set_title_hidden(&self, hidden: bool) {
        registry::set_title_hidden(&self.window, hidden);
        hooks::update_titlebar_visibility(&self.window);
    }
}

fn tab_group_inset_in_points(group: &NSWindowTabGroup) -> Option<f64> {
    let selected_inset = group.selectedWindow().and_then(|window| tab_inset_in_points(&window));
    selected_inset.or_else(|| {
        group.windows().iter().filter_map(|window| tab_inset_in_points(&window)).reduce(f64::max)
    })
}

fn visible_toolbar_inset_in_points(window: &NSWindow) -> Option<f64> {
    let toolbar = window.toolbar()?;
    if !toolbar.isVisible() {
        return None;
    }

    let content = window.contentView()?;
    Some(content_layout_top_in_points(window, &content) + layout::TOP_INSET)
}

fn tab_inset_in_points(window: &NSWindow) -> Option<f64> {
    let content = window.contentView()?;
    let layout_top = content_layout_top_in_points(window, &content);

    // NOTE: during fullscreen transitions, AppKit may update content bounds before
    // titlebar geometry. This helper ignores tab clip and toolbar bottoms past
    // layout_top + TOP_INSET, so stale views cannot enlarge the grid inset.
    let points = clipped_tab_inset_in_points(window, &content, layout_top);
    points.is_finite().then_some(points)
}

fn content_layout_top_in_points(window: &NSWindow, content: &NSView) -> f64 {
    let content_bounds = content.bounds();
    let layout_in_content = content.convertRect_fromView(window.contentLayoutRect(), None);

    // After conversion both rectangles use content coordinates. The visual top
    // is at minimum Y for a flipped view and maximum Y for an unflipped view.
    if content.isFlipped() {
        return layout_in_content.origin.y - content_bounds.origin.y;
    }

    let content_top = content_bounds.origin.y + content_bounds.size.height;
    content_top - layout_in_content.origin.y - layout_in_content.size.height
}

/// Activation and deferred layout may run before AppKit has merged the windows,
/// so the host height stays pending until the group has at least two windows,
/// a visible tab bar and toolbar. AND an `NSTabBar` attached to the new window.
fn restore_group_height(window: &NSWindow) {
    let Some(host_height) = registry::pending_content_height(window) else { return };
    let Some(group) = window.tabGroup() else { return };
    if group.windows().len() < 2 || !group.isTabBarVisible() {
        return;
    }

    if !window.toolbar().is_some_and(|toolbar| toolbar.isVisible()) {
        return;
    }

    // Toolbar visibility can precede attachment of the actual tab view.
    // Keep the target until AppKit has completed that part of the merge.
    let Some(content) = window.contentView() else { return };
    if layout::tab_bar_bottom_in_content_view(window, &content).is_none() {
        return;
    }

    // AppKit enlarges each group member when it shows the tab toolbar. Once
    // the new window owns the actual tab view, restore the host content height
    // and preserve each frame top edge in AppKit coordinates.
    for member in group.windows().iter() {
        let current_frame = member.frame();
        let mut content_rect = member.contentRectForFrameRect(current_frame);
        if content_rect.size.height == host_height {
            continue;
        }

        content_rect.size.height = host_height;
        let mut frame = member.frameRectForContentRect(content_rect);
        frame.origin = NSPoint::new(
            current_frame.origin.x,
            current_frame.origin.y + current_frame.size.height - frame.size.height,
        );

        frame.size.width = current_frame.size.width;
        member.setFrame_display(frame, false);
    }

    // This target belongs only to the new tab attachment. Later tab switches
    // and user resizing must not restore the old host size.
    registry::clear_pending_content_height(window);
}

fn clipped_tab_inset_in_points(window: &NSWindow, content: &NSView, layout_top: f64) -> f64 {
    let tab_clip_bottom = layout::tab_bar_bottom_in_content_view(window, content);
    let estimated_tab_bottom = layout::constrained_tab_bottom_in_content_view(window, content);
    tab_inset_from_bottoms(layout_top, tab_clip_bottom, estimated_tab_bottom)
}

fn tab_inset_from_bottoms(
    layout_top: f64,
    tab_clip_bottom: Option<f64>,
    estimated_tab_bottom: Option<f64>,
) -> f64 {
    let bottom_limit = layout_top + layout::TOP_INSET;
    let mut inset = layout_top.max(0.0);
    for bottom in [tab_clip_bottom, estimated_tab_bottom].into_iter().flatten() {
        if bottom <= bottom_limit {
            inset = inset.max(bottom);
        }
    }

    inset
}

impl Drop for WindowSystemTabs {
    fn drop(&mut self) { registry::unregister(&self.window); }
}

impl MacosWindowFeature {
    /// Recheck inline tab placement after a window or appearance change.
    pub fn refresh_inline_system_tabs(&self) {
        if let Some(tabs) = &self.system_tabs {
            tabs.refresh();
        }
    }

    /// Install inline layout before native tabbing is enabled after simple fullscreen.
    pub fn ensure_inline_system_tabs(&mut self) {
        if self.simple_fullscreen || self.system_tabs.is_some() {
            return;
        }

        let settings = self.settings.get::<CmdLineSettings>();
        if !settings.system_native_tabs || !supports_system_tabs(settings.frame) {
            return;
        }

        let title_hidden = settings.title_hidden;
        let window_settings = self.settings.get::<WindowSettings>();
        let translucent = window_settings.opacity.min(window_settings.normal_opacity) < 1.0;

        let mtm = MainThreadMarker::new().expect("inline tabs require the main thread");
        let tabs = WindowSystemTabs::install(self.ns_window.clone(), mtm, title_hidden, None);
        tabs.set_translucent_background(translucent);

        self.system_tabs = Some(tabs);
    }
}
