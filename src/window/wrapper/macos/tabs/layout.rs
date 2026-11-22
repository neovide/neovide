use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::NSLayoutConstraint;
use objc2_app_kit::NSView;
use objc2_app_kit::NSWindow;
use objc2_app_kit::NSWindowButton;
use objc2_foundation::ns_string;

// AppKit installs these constraints on the nearest common ancestor of their
// views. The identifier lets layout find them across the titlebar subtree.
const CONSTRAINT_ID: &str = "NeovideInlineTabBar";
const CONSTRAINT_COUNT: usize = 8;
const CONSTRAINTS_PER_VIEW_PAIR: usize = 4;
const WINDOW_BUTTON_GAP: f64 = 78.0;
pub const TOP_INSET: f64 = 2.0;

/// Read the concrete class name used by tab layout and accessory hooks. Failed
/// text conversion yields an empty name, so none of the view checks can match.
pub fn class_name(object: &AnyObject) -> &str {
    AnyObject::class(object).name().to_str().unwrap_or("")
}

/// Find the FIRST descendant with the exact runtime class name `name`. The root
/// is excluded, so the accessory hook also checks the controller view itself.
/// Search visits children before later siblings and retains the matching view.
pub fn find_descendant(view: &NSView, name: &str) -> Option<Retained<NSView>> {
    for child in view.subviews().iter() {
        if class_name(child.as_ref()) == name {
            return Some(child.clone());
        }

        if let Some(found) = find_descendant(&child, name) {
            return Some(found);
        }
    }

    None
}

/// Find the AppKit titlebar from the window content view. The titlebar may lie
/// outside that subtree, so layout and tab inset measurement start their search
/// at the topmost ancestor. Missing views leave layout for a later pass, while
/// inset measurement reads the window content layout rectangle as a fallback.
pub fn titlebar_view(window: &NSWindow) -> Option<Retained<NSView>> {
    let mut root = window.contentView()?;
    while let Some(parent) = unsafe { root.superview() } {
        root = parent;
    }

    find_descendant(&root, "NSTitlebarView")
}

// AppKit has used both clip and container views around NSTabBar. Without one
// of these ancestors, layout leaves the current AppKit arrangement alone.
fn accessory_clip_view(tab_bar: &NSView) -> Option<Retained<NSView>> {
    let mut ancestor = unsafe { tab_bar.superview() };
    while let Some(view) = ancestor {
        if matches!(
            class_name(view.as_ref()),
            "NSTitlebarAccessoryClipView" | "NSTitlebarAccessoryContainerView"
        ) {
            return Some(view);
        }
        ancestor = unsafe { view.superview() };
    }
    None
}

pub fn tab_bar_bottom_in_content_view(window: &NSWindow, content: &NSView) -> Option<f64> {
    let titlebar = titlebar_view(window)?;
    let tab_bar = find_descendant(&titlebar, "NSTabBar")?;
    let clip = accessory_clip_view(&tab_bar)?;
    bottom_in_content_view(&clip, content).map(|bottom| bottom.max(0.0))
}

/// Estimate the tab clip bottom from toolbar geometry during tab attachment.
/// layout offsets the clip top by `TOP_INSET` and matches its height to the
/// toolbar, so this edge remains available while AppKit moves the tab view.
pub fn constrained_tab_bottom_in_content_view(window: &NSWindow, content: &NSView) -> Option<f64> {
    let titlebar = titlebar_view(window)?;
    let toolbar = find_descendant(&titlebar, "NSToolbarView")?;
    bottom_in_content_view(&toolbar, content).map(|bottom| (bottom + TOP_INSET).max(0.0))
}

// The winit content view is flipped, while AppKit titlebar views need not be.
// Convert each view before measuring its bottom in content coordinates.
// https://developer.apple.com/documentation/appkit/nsview/isflipped
fn bottom_in_content_view(view: &NSView, content: &NSView) -> Option<f64> {
    let frame = view.convertRect_toView(view.bounds(), Some(content));
    let bounds = content.bounds();
    let bottom = if content.isFlipped() {
        frame.origin.y + frame.size.height - bounds.origin.y
    } else {
        bounds.origin.y + bounds.size.height - frame.origin.y
    };

    bottom.is_finite().then_some(bottom)
}

/// Align the attached AppKit tab view within the titlebar toolbar.
///
/// The named views are AppKit implementation details. Missing views may mean
/// the bar is not attached yet or that the hierarchy differs on this system.
/// A later refresh can try again. Layout reuses the active constraints only
/// when they connect the current toolbar, clip and accessory. AppKit can
/// replace these views after tab selection or a titlebar rebuild.
pub fn layout(window: &NSWindow) {
    let Some(titlebar) = titlebar_view(window) else { return };
    let Some(toolbar) = find_descendant(&titlebar, "NSToolbarView") else { return };
    let Some(tab_bar) = find_descendant(&titlebar, "NSTabBar") else { return };
    let Some(clip) = accessory_clip_view(&tab_bar) else { return };
    let Some(accessory) = clip.subviews().iter().next() else { return };

    clip.setTranslatesAutoresizingMaskIntoConstraints(false);
    accessory.setTranslatesAutoresizingMaskIntoConstraints(false);

    let existing = active_inline_tab_constraints(&titlebar);
    if constraints_match_current_views(&existing, &toolbar, &clip, &accessory) {
        clip.setNeedsLayout(true);
        accessory.setNeedsLayout(true);
        return;
    }

    // Retire both titlebar and clip constraints before installing a new set.
    for constraint in existing {
        constraint.setActive(false);
    }

    // Reserve space for the standard window buttons when they exist. The
    // fixed gap keeps the first tab clear of those buttons.
    let left = if window.standardWindowButton(NSWindowButton::CloseButton).is_some() {
        WINDOW_BUTTON_GAP
    } else {
        0.0
    };

    let constraints = [
        clip.leftAnchor().constraintEqualToAnchor_constant(&toolbar.leftAnchor(), left),
        clip.rightAnchor().constraintEqualToAnchor(&toolbar.rightAnchor()),
        clip.topAnchor().constraintEqualToAnchor_constant(&toolbar.topAnchor(), TOP_INSET),
        clip.heightAnchor().constraintEqualToAnchor(&toolbar.heightAnchor()),
        accessory.leftAnchor().constraintEqualToAnchor(&clip.leftAnchor()),
        accessory.rightAnchor().constraintEqualToAnchor(&clip.rightAnchor()),
        accessory.topAnchor().constraintEqualToAnchor(&clip.topAnchor()),
        accessory.heightAnchor().constraintEqualToAnchor(&clip.heightAnchor()),
    ];

    for constraint in constraints {
        constraint.setIdentifier(Some(ns_string!(CONSTRAINT_ID)));
        constraint.setActive(true);
    }

    clip.setNeedsLayout(true);
    accessory.setNeedsLayout(true);
}

/// Collect a snapshot of active tagged constraints in the titlebar subtree so
/// layout can decide whether to reuse or retire the whole set after the scan.
fn active_inline_tab_constraints(titlebar: &NSView) -> Vec<Retained<NSLayoutConstraint>> {
    let mut constraints = Vec::new();
    collect_active_inline_tab_constraints(titlebar, &mut constraints);
    constraints
}

/// Search the titlebar subtree because AppKit installs each constraint on the
/// nearest common ancestor of its items. The four accessory to clip constraints
/// can belong to the clip while another view owns clip to toolbar constraints.
fn collect_active_inline_tab_constraints(
    view: &NSView,
    constraints: &mut Vec<Retained<NSLayoutConstraint>>,
) {
    for constraint in view.constraints().iter() {
        if constraint.isActive()
            && constraint.identifier().is_some_and(|id| id.to_string() == CONSTRAINT_ID)
        {
            constraints.push(constraint);
        }
    }

    for child in view.subviews().iter() {
        collect_active_inline_tab_constraints(&child, constraints);
    }
}

/// Reuse an active tagged set only when it still connects the current AppKit
/// views in the expected first and second item order. Four constraints join
/// clip to toolbar and four join accessory to clip, with no extra constraints.
/// A replaced view fails this identity check. The result also assumes layout
/// alone tags these constraints, since the anchor attributes are not compared.
fn constraints_match_current_views(
    existing: &[Retained<NSLayoutConstraint>],
    toolbar: &NSView,
    clip: &NSView,
    accessory: &NSView,
) -> bool {
    if existing.len() != CONSTRAINT_COUNT {
        return false;
    }

    let clip_to_toolbar = existing
        .iter()
        .filter(|constraint| connects_views_in_order(constraint, clip, toolbar))
        .count();

    let accessory_to_clip = existing
        .iter()
        .filter(|constraint| connects_views_in_order(constraint, accessory, clip))
        .count();

    clip_to_toolbar == CONSTRAINTS_PER_VIEW_PAIR && accessory_to_clip == CONSTRAINTS_PER_VIEW_PAIR
}

/// Match the constraint items to the current views in their stored order. A
/// reversed pair cannot count toward either set of four required for reuse.
fn connects_views_in_order(
    constraint: &NSLayoutConstraint,
    first: &NSView,
    second: &NSView,
) -> bool {
    item_is_view(unsafe { constraint.firstItem() }, first)
        && item_is_view(unsafe { constraint.secondItem() }, second)
}

// A constraint item can be a layout guide instead of an NSView. Cast its
// pointer only for address comparison, and never dereference that cast.
fn item_is_view(item: Option<Retained<AnyObject>>, view: &NSView) -> bool {
    item.is_some_and(|item| std::ptr::eq(Retained::as_ptr(&item).cast::<NSView>(), view))
}
