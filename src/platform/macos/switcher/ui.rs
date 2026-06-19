use objc2::{
    MainThreadOnly,
    rc::Retained,
    runtime::{AnyObject, ProtocolObject, Sel},
    sel,
};
use objc2_app_kit::{
    NSApplication, NSBezelStyle, NSButton, NSButtonType, NSColor, NSControl, NSFloatingWindowLevel,
    NSFocusRingType, NSFont, NSFontWeight, NSImage, NSImageScaling, NSImageView, NSLineBreakMode,
    NSResponder, NSScrollView, NSTextAlignment, NSTextField, NSTextFieldDelegate, NSView, NSWindow,
    NSWindowButton, NSWindowStyleMask, NSWindowTitleVisibility,
};
use objc2_foundation::{MainThreadMarker, NSInteger, NSPoint, NSRect, NSSize, NSString, ns_string};

use super::super::load_neovide_icon;
use super::appkit::{
    EditorSwitcherActionHandler, EditorSwitcherDocumentView, EditorSwitcherSearchDelegate,
    EditorSwitcherSearchField, EditorSwitcherView, EditorSwitcherWindow,
};
use super::hotkey::editor_switcher_toggle_shortcut;
use super::layout::{BottomBar, Panel, Results, Row, Search};
use super::row::EditorSwitcherRow;
use super::{
    EDITOR_SWITCHER_STATE, EditorSwitcherState, EditorSwitcherStateParts, close_editor_switcher,
};

const SEARCH_PLACEHOLDER: &str = "Search...";
const EMPTY_RESULTS_TEXT: &str = "No matching windows";
const FOOTER_CLOSE_TEXT: &str = "Close";
const FOOTER_OPEN_SELECTED_TEXT: &str = "Open Selected";
const FOOTER_CLOSE_SHORTCUTS: &[&str] = &["esc"];
const FOOTER_OPEN_SELECTED_SHORTCUTS: &[&str] = &["↩"];

const SELECTED_ROW_ALPHA: f64 = 0.10;
const BOTTOM_BAR_ALPHA: f64 = 0.10;
const SHORTCUT_CHIP_ALPHA: f64 = 0.05;

struct FooterShortcutChip {
    view: Retained<NSView>,
    width: f64,
    height: f64,
}

struct EditorSwitcherPanelShell {
    container_view: Retained<NSView>,
    root_view: Retained<EditorSwitcherView>,
}

struct EditorSwitcherResultsArea {
    scroll_view: Retained<NSScrollView>,
    results_container: Retained<EditorSwitcherDocumentView>,
}

struct EditorSwitcherSearchViews {
    field: Retained<NSTextField>,
    delegate: Retained<EditorSwitcherSearchDelegate>,
}

struct FooterButtonSpec {
    text: &'static str,
    shortcuts: &'static [&'static str],
    action: Sel,
}

#[derive(Clone, Copy)]
enum EditorSwitcherLabelStyle {
    Primary,
    Secondary,
    Footer,
    Shortcut,
}

pub fn show_editor_switcher_panel(rows: Vec<EditorSwitcherRow>, custom_icon_path: Option<&String>) {
    let Some(mtm) = MainThreadMarker::new() else {
        log::warn!("Editor switcher requested off the main thread");
        return;
    };

    close_editor_switcher();

    let background = NSColor::controlBackgroundColor();
    let window = editor_switcher_window(mtm, background.as_ref());
    let EditorSwitcherPanelShell { container_view, root_view } =
        editor_switcher_panel_shell(mtm, background.as_ref());

    let EditorSwitcherSearchViews { field: search_field, delegate: search_delegate } =
        editor_switcher_search_field(mtm);
    let root_ns_view: &NSView = root_view.as_ref();
    root_ns_view.addSubview(search_field.as_ref());
    root_ns_view.addSubview(editor_switcher_divider(mtm).as_ref());

    let EditorSwitcherResultsArea { scroll_view, results_container } =
        editor_switcher_results_area(mtm);
    root_ns_view.addSubview(scroll_view.as_ref());

    let row_action_handler = EditorSwitcherActionHandler::new(mtm);
    let bottom_bar_background = editor_switcher_bottom_bar_background();
    root_ns_view.addSubview(
        editor_switcher_bottom_bar(
            mtm,
            row_action_handler.as_ref(),
            bottom_bar_background.as_ref(),
        )
        .as_ref(),
    );

    container_view.addSubview(
        editor_switcher_bottom_bar_gap_fill(mtm, bottom_bar_background.as_ref()).as_ref(),
    );
    root_ns_view.setFrame(NSRect::new(
        NSPoint::new(0.0, Panel::TITLE_ADJUSTMENT),
        NSSize::new(Panel::WIDTH, Panel::CONTENT_HEIGHT),
    ));
    container_view.addSubview(root_ns_view);
    window.setContentView(Some(container_view.as_ref()));

    let icon = load_neovide_icon(custom_icon_path);
    let state = EditorSwitcherState::new(EditorSwitcherStateParts {
        window,
        search_field,
        results_container,
        row_action_handler,
        search_delegate,
        rows,
        icon,
        toggle_shortcut: editor_switcher_toggle_shortcut(),
    });

    present_editor_switcher(mtm, state);
}

pub fn rebuild_editor_switcher_rows(state: &EditorSwitcherState) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };

    let container: &NSView = state.results_container.as_ref();
    for subview in container.subviews().iter() {
        subview.removeFromSuperview();
    }

    let visible_width = Panel::WIDTH;
    let scroll_view_height = Results::CONTAINER_HEIGHT;

    if state.filtered_indices.is_empty() {
        container.setFrameSize(NSSize::new(visible_width, scroll_view_height));
        let empty_label = editor_switcher_empty_results_label(mtm, visible_width);
        container.addSubview(empty_label.as_ref());
        return;
    }

    let document_height = (state.filtered_indices.len() as f64 * Row::HEIGHT
        + Results::BOTTOM_PADDING)
        .max(scroll_view_height);
    container.setFrameSize(NSSize::new(visible_width, document_height));

    for (row_index, row_data_index) in state.filtered_indices.iter().copied().enumerate() {
        let row = &state.rows[row_data_index];
        let selected = row_index == state.selected_index;
        let y = row_index as f64 * Row::HEIGHT;
        let row_view = editor_switcher_row_button(
            mtm,
            row,
            row_index,
            selected,
            state.icon.as_deref(),
            state.row_action_handler.as_ref(),
        );
        let row_ns_view: &NSView = row_view.as_ref();
        row_ns_view.setFrame(NSRect::new(
            NSPoint::new(Results::OUTER_PADDING, y),
            NSSize::new(visible_width - Results::OUTER_PADDING * 2.0, Row::BUTTON_HEIGHT),
        ));
        container.addSubview(row_view.as_ref());
    }

    scroll_selected_editor_switcher_row_into_view(container, state.selected_index);
}

fn present_editor_switcher(mtm: MainThreadMarker, state: EditorSwitcherState) {
    state.update_results_view();

    let app = NSApplication::sharedApplication(mtm);
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);
    let switcher_window = state.window.clone();
    let switcher_search_field = state.search_field.clone();
    switcher_window.makeKeyAndOrderFront(None);

    EDITOR_SWITCHER_STATE.with(|cell| {
        *cell.borrow_mut() = Some(state);
    });

    let search_view: &NSView = switcher_search_field.as_ref();
    let responder: &NSResponder = search_view.as_ref();
    switcher_window.makeFirstResponder(Some(responder));
}

fn editor_switcher_window(mtm: MainThreadMarker, background: &NSColor) -> Retained<NSWindow> {
    let style = NSWindowStyleMask::Titled
        | NSWindowStyleMask::Closable
        | NSWindowStyleMask::FullSizeContentView;
    let window: Retained<NSWindow> =
        EditorSwitcherWindow::new(mtm, editor_switcher_panel_frame(), style).into_super();

    window.setTitle(ns_string!("Editors"));
    window.setTitleVisibility(NSWindowTitleVisibility::Hidden);
    window.setTitlebarAppearsTransparent(true);
    window.setLevel(NSFloatingWindowLevel);
    window.setHasShadow(true);
    window.setMovableByWindowBackground(true);
    window.setOpaque(false);
    window.setBackgroundColor(Some(background));
    hide_editor_switcher_window_buttons(&window);

    unsafe {
        window.setReleasedWhenClosed(false);
    }

    window.center();

    window
}

fn editor_switcher_panel_shell(
    mtm: MainThreadMarker,
    background: &NSColor,
) -> EditorSwitcherPanelShell {
    let root_view = EditorSwitcherView::new(mtm, editor_switcher_content_frame());
    let root_ns_view: &NSView = root_view.as_ref();
    set_editor_switcher_view_background(root_ns_view, background, 0.0);

    let container_view = NSView::initWithFrame(NSView::alloc(mtm), editor_switcher_panel_frame());
    set_editor_switcher_view_background(container_view.as_ref(), background, 0.0);

    EditorSwitcherPanelShell { container_view, root_view }
}

fn editor_switcher_search_field(mtm: MainThreadMarker) -> EditorSwitcherSearchViews {
    let search_frame = NSRect::new(
        NSPoint::new(
            Panel::HORIZONTAL_PADDING,
            Search::VERTICAL_PADDING + Search::FIELD_TOP_OFFSET,
        ),
        NSSize::new(
            Panel::WIDTH - Panel::HORIZONTAL_PADDING * 2.0,
            Search::FONT_SIZE + Search::FIELD_HEIGHT_EXTRA,
        ),
    );
    let search_field: Retained<NSTextField> =
        EditorSwitcherSearchField::new(mtm, search_frame).into_super();
    let search_placeholder = NSString::from_str(SEARCH_PLACEHOLDER);
    search_field.setPlaceholderString(Some(&search_placeholder));
    search_field.setBezeled(false);
    search_field.setBordered(false);
    search_field.setDrawsBackground(false);
    search_field.setEditable(true);
    search_field.setSelectable(true);
    search_field.setTextColor(Some(NSColor::labelColor().as_ref()));

    let search_view: &NSView = search_field.as_ref();
    search_view.setFocusRingType(NSFocusRingType::None);

    let search_control: &NSControl = search_field.as_ref();
    search_control.setFont(Some(NSFont::systemFontOfSize(Search::FONT_SIZE).as_ref()));
    search_control.setRefusesFirstResponder(false);

    let search_delegate = EditorSwitcherSearchDelegate::new(mtm);
    unsafe {
        let search_delegate_object: &EditorSwitcherSearchDelegate = search_delegate.as_ref();
        let search_delegate_protocol: &ProtocolObject<dyn NSTextFieldDelegate> =
            ProtocolObject::from_ref(search_delegate_object);
        search_field.setDelegate(Some(search_delegate_protocol));
    }

    EditorSwitcherSearchViews { field: search_field, delegate: search_delegate }
}

fn editor_switcher_divider(mtm: MainThreadMarker) -> Retained<NSView> {
    let divider = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, Search::SECTION_HEIGHT), NSSize::new(Panel::WIDTH, 1.0)),
    );

    set_editor_switcher_view_background(divider.as_ref(), NSColor::separatorColor().as_ref(), 0.0);
    divider
}

fn editor_switcher_results_area(mtm: MainThreadMarker) -> EditorSwitcherResultsArea {
    let scroll_y = Search::SECTION_HEIGHT + Results::OUTER_PADDING;
    let scroll_height = Results::CONTAINER_HEIGHT;
    let scroll_view = NSScrollView::initWithFrame(
        NSScrollView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, scroll_y), NSSize::new(Panel::WIDTH, scroll_height)),
    );
    scroll_view.setDrawsBackground(false);
    scroll_view.setBorderType(objc2_app_kit::NSBorderType::NoBorder);
    scroll_view.setHasVerticalScroller(true);
    scroll_view.setHasHorizontalScroller(false);
    scroll_view.setAutohidesScrollers(true);

    let results_container = EditorSwitcherDocumentView::new(
        mtm,
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(Panel::WIDTH, scroll_height)),
    );

    let results_view: &NSView = results_container.as_ref();
    scroll_view.setDocumentView(Some(results_view));

    EditorSwitcherResultsArea { scroll_view, results_container }
}

fn editor_switcher_bottom_bar(
    mtm: MainThreadMarker,
    row_action_handler: &EditorSwitcherActionHandler,
    background: &NSColor,
) -> Retained<NSView> {
    let bottom_bar_height = editor_switcher_bottom_bar_height();
    let bottom_bar = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(
            NSPoint::new(0.0, editor_switcher_bottom_bar_y()),
            NSSize::new(Panel::WIDTH, bottom_bar_height),
        ),
    );
    set_editor_switcher_view_background(bottom_bar.as_ref(), background, 0.0);

    let mut trailing_edge = Panel::WIDTH - BottomBar::HORIZONTAL_PADDING;
    for spec in editor_switcher_footer_buttons_from_trailing_edge() {
        let button = editor_switcher_bottom_bar_button(mtm, spec.text, spec.shortcuts, spec.action);
        let button_view: &NSView = button.as_ref();
        let button_size = button_view.frame().size;

        trailing_edge -= button_size.width;
        button_view.setFrame(NSRect::new(
            NSPoint::new(trailing_edge, (bottom_bar_height - button_size.height) / 2.0),
            button_size,
        ));
        trailing_edge -= BottomBar::BUTTON_SPACING;

        let button_control: &NSControl = button.as_ref();
        set_editor_switcher_control_target(button_control, row_action_handler);
        bottom_bar.addSubview(button.as_ref());
    }

    bottom_bar
}

fn editor_switcher_bottom_bar_background() -> Retained<NSColor> {
    NSColor::blackColor().colorWithAlphaComponent(BOTTOM_BAR_ALPHA)
}

fn editor_switcher_footer_buttons_from_trailing_edge() -> [FooterButtonSpec; 2] {
    [
        FooterButtonSpec {
            text: FOOTER_CLOSE_TEXT,
            shortcuts: FOOTER_CLOSE_SHORTCUTS,
            action: sel!(editorSwitcherClose:),
        },
        FooterButtonSpec {
            text: FOOTER_OPEN_SELECTED_TEXT,
            shortcuts: FOOTER_OPEN_SELECTED_SHORTCUTS,
            action: sel!(editorSwitcherOpenSelected:),
        },
    ]
}

fn editor_switcher_bottom_bar_gap_fill(
    mtm: MainThreadMarker,
    background: &NSColor,
) -> Retained<NSView> {
    let bottom_bar_gap_fill = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(Panel::WIDTH, Panel::TITLE_ADJUSTMENT)),
    );
    set_editor_switcher_view_background(bottom_bar_gap_fill.as_ref(), background, 0.0);

    bottom_bar_gap_fill
}

fn editor_switcher_panel_frame() -> NSRect {
    NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(Panel::WIDTH, Panel::HEIGHT))
}

fn editor_switcher_content_frame() -> NSRect {
    NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(Panel::WIDTH, Panel::CONTENT_HEIGHT))
}

fn editor_switcher_bottom_bar_y() -> f64 {
    Search::SECTION_HEIGHT + Results::OUTER_PADDING + Results::CONTAINER_HEIGHT
}

fn editor_switcher_bottom_bar_height() -> f64 {
    (Panel::CONTENT_HEIGHT - editor_switcher_bottom_bar_y()).max(BottomBar::HEIGHT)
}

fn editor_switcher_empty_results_label(
    mtm: MainThreadMarker,
    visible_width: f64,
) -> Retained<NSTextField> {
    let empty_label = editor_switcher_label(
        mtm,
        EMPTY_RESULTS_TEXT,
        Results::EMPTY_LABEL_FONT_SIZE,
        EditorSwitcherLabelStyle::Secondary,
    );
    let empty_label_view: &NSView = empty_label.as_ref();
    empty_label_view.setFrame(NSRect::new(
        NSPoint::new(Panel::HORIZONTAL_PADDING, Results::EMPTY_LABEL_Y),
        NSSize::new(visible_width - Panel::HORIZONTAL_PADDING * 2.0, Results::EMPTY_LABEL_HEIGHT),
    ));
    empty_label
}

fn scroll_selected_editor_switcher_row_into_view(container: &NSView, selected_index: usize) {
    let visible_rect = container.visibleRect();
    let visible_min_y = visible_rect.origin.y;
    let visible_max_y = visible_rect.origin.y + visible_rect.size.height;
    let selected_min_y = selected_index as f64 * Row::HEIGHT;
    let selected_max_y = selected_min_y + Row::HEIGHT;

    let scroll_y = if selected_min_y < visible_min_y {
        selected_min_y
    } else if selected_max_y > visible_max_y {
        selected_max_y - visible_rect.size.height
    } else {
        return;
    };

    container.scrollPoint(NSPoint::new(0.0, scroll_y.max(0.0)));
}

fn hide_editor_switcher_window_buttons(window: &NSWindow) {
    for button_kind in
        [NSWindowButton::MiniaturizeButton, NSWindowButton::CloseButton, NSWindowButton::ZoomButton]
    {
        if let Some(button) = window.standardWindowButton(button_kind) {
            let button_view: &NSView = button.as_ref();
            button_view.setHidden(true);
        }
    }
}

fn editor_switcher_row_frame(row_width: f64, row_height: f64) -> NSRect {
    NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(row_width, row_height))
}

fn editor_switcher_row_icon_frame(row_height: f64) -> NSRect {
    NSRect::new(
        NSPoint::new(Results::INNER_PADDING, (row_height - Row::ICON_SIZE) / 2.0),
        NSSize::new(Row::ICON_SIZE, Row::ICON_SIZE),
    )
}

fn editor_switcher_row_title_frame(row_width: f64, row_height: f64) -> NSRect {
    let title_x = Results::INNER_PADDING + Row::ICON_SIZE + Row::ICON_TITLE_SPACING;
    NSRect::new(
        NSPoint::new(title_x, (row_height - Row::LABEL_HEIGHT) / 2.0),
        NSSize::new(row_width - title_x - Results::INNER_PADDING, Row::LABEL_HEIGHT),
    )
}

fn editor_switcher_row_button(
    mtm: MainThreadMarker,
    row: &EditorSwitcherRow,
    row_index: usize,
    selected: bool,
    icon: Option<&NSImage>,
    row_action_handler: &EditorSwitcherActionHandler,
) -> Retained<NSButton> {
    let row_width = Panel::WIDTH - Results::OUTER_PADDING * 2.0;
    let row_height = Row::BUTTON_HEIGHT;
    let button = editor_switcher_button(mtm, editor_switcher_row_frame(row_width, row_height));

    let row_control: &NSControl = button.as_ref();
    row_control.setTag(row_index as NSInteger);
    row_control.setRefusesFirstResponder(true);
    set_editor_switcher_control_target(row_control, row_action_handler);
    unsafe {
        row_control.setAction(Some(sel!(editorSwitcherRowClicked:)));
    }

    let row_view: &NSView = button.as_ref();
    let row_background = if selected {
        NSColor::systemGrayColor().colorWithAlphaComponent(SELECTED_ROW_ALPHA)
    } else {
        NSColor::clearColor()
    };
    set_editor_switcher_view_background(row_view, &row_background, Row::CORNER_RADIUS);

    if let Some(icon) = icon {
        let image_view = NSImageView::initWithFrame(
            NSImageView::alloc(mtm),
            editor_switcher_row_icon_frame(row_height),
        );
        image_view.setImage(Some(icon));
        image_view.setImageScaling(NSImageScaling::ScaleProportionallyDown);
        row_view.addSubview(image_view.as_ref());
    }

    let title_label = editor_switcher_label(
        mtm,
        &row.title,
        Row::TITLE_FONT_SIZE,
        EditorSwitcherLabelStyle::Primary,
    );
    let title_label_view: &NSView = title_label.as_ref();
    title_label_view.setFrame(editor_switcher_row_title_frame(row_width, row_height));
    row_view.addSubview(title_label.as_ref());

    button
}

fn editor_switcher_bottom_bar_button(
    mtm: MainThreadMarker,
    text: &str,
    shortcuts: &[&str],
    action: Sel,
) -> Retained<NSButton> {
    let text_label =
        editor_switcher_label(mtm, text, BottomBar::FONT_SIZE, EditorSwitcherLabelStyle::Footer);
    let text_size = editor_switcher_fit_label(text_label.as_ref());
    let text_width = text_size.width.ceil();
    let text_height = text_size.height.ceil();

    let shortcut_chips: Vec<FooterShortcutChip> = shortcuts
        .iter()
        .map(|shortcut| {
            editor_switcher_footer_shortcut_chip(mtm, shortcut, BottomBar::SHORTCUT_FONT_SIZE)
        })
        .collect();
    let shortcuts_width = shortcut_chips.iter().map(|chip| chip.width).sum::<f64>()
        + BottomBar::SHORTCUT_SPACING * shortcut_chips.len().saturating_sub(1) as f64;
    let shortcuts_height = shortcut_chips.iter().map(|chip| chip.height).fold(0.0, f64::max);
    let label_shortcut_spacing =
        if shortcut_chips.is_empty() { 0.0 } else { BottomBar::LABEL_SHORTCUT_SPACING };
    let content_height = text_height.max(shortcuts_height);
    let button_height =
        BottomBar::BUTTON_HEIGHT.max(content_height + BottomBar::BUTTON_VERTICAL_PADDING * 2.0);
    let width = BottomBar::BUTTON_LEADING_PADDING
        + text_width
        + label_shortcut_spacing
        + shortcuts_width
        + BottomBar::BUTTON_TRAILING_PADDING;

    let button = editor_switcher_button(
        mtm,
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, button_height)),
    );
    let button_control: &NSControl = button.as_ref();
    button_control.setRefusesFirstResponder(true);
    unsafe {
        button_control.setAction(Some(action));
    }

    let button_view: &NSView = button.as_ref();
    set_editor_switcher_view_background(
        button_view,
        &NSColor::clearColor(),
        BottomBar::BUTTON_CORNER_RADIUS,
    );

    let text_label_view: &NSView = text_label.as_ref();
    text_label_view.setFrame(NSRect::new(
        NSPoint::new(BottomBar::BUTTON_LEADING_PADDING, (button_height - text_height) / 2.0),
        NSSize::new(text_width, text_height),
    ));
    button_view.addSubview(text_label.as_ref());

    let mut x = BottomBar::BUTTON_LEADING_PADDING + text_width + label_shortcut_spacing;
    for shortcut_chip in shortcut_chips {
        let shortcut_chip_view: &NSView = shortcut_chip.view.as_ref();
        shortcut_chip_view
            .setFrameOrigin(NSPoint::new(x, (button_height - shortcut_chip.height) / 2.0));
        button_view.addSubview(shortcut_chip.view.as_ref());
        x += shortcut_chip.width + BottomBar::SHORTCUT_SPACING;
    }

    button
}

fn editor_switcher_footer_shortcut_chip(
    mtm: MainThreadMarker,
    text: &str,
    size: f64,
) -> FooterShortcutChip {
    let label = editor_switcher_label(mtm, text, size, EditorSwitcherLabelStyle::Shortcut);
    let label_size = editor_switcher_fit_label(label.as_ref());
    let label_width = label_size.width.ceil();
    let label_height = label_size.height.ceil();
    let width = label_width + BottomBar::SHORTCUT_HORIZONTAL_PADDING * 2.0;
    let height = label_height + BottomBar::SHORTCUT_VERTICAL_PADDING * 2.0;

    let chip = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, height)),
    );
    set_editor_switcher_view_background(
        chip.as_ref(),
        &NSColor::whiteColor().colorWithAlphaComponent(SHORTCUT_CHIP_ALPHA),
        BottomBar::SHORTCUT_CHIP_CORNER_RADIUS,
    );

    let label_view: &NSView = label.as_ref();
    label_view.setFrame(NSRect::new(
        NSPoint::new(BottomBar::SHORTCUT_HORIZONTAL_PADDING, BottomBar::SHORTCUT_VERTICAL_PADDING),
        NSSize::new(label_width, label_height),
    ));
    chip.addSubview(label.as_ref());

    FooterShortcutChip { view: chip, width, height }
}

fn editor_switcher_button(mtm: MainThreadMarker, frame: NSRect) -> Retained<NSButton> {
    let button = NSButton::initWithFrame(NSButton::alloc(mtm), frame);
    button.setTitle(ns_string!(""));
    button.setButtonType(NSButtonType::MomentaryChange);
    button.setBezelStyle(NSBezelStyle::Automatic);
    button.setBordered(false);
    button.setTransparent(true);
    button
}

fn set_editor_switcher_control_target(control: &NSControl, target: &EditorSwitcherActionHandler) {
    unsafe {
        let target: &AnyObject = target.as_ref();
        control.setTarget(Some(target));
    }
}

fn editor_switcher_fit_label(label: &NSTextField) -> NSSize {
    let control: &NSControl = label.as_ref();
    control.sizeToFit();
    let label_view: &NSView = label.as_ref();
    label_view.frame().size
}

fn editor_switcher_label(
    mtm: MainThreadMarker,
    text: &str,
    size: f64,
    style: EditorSwitcherLabelStyle,
) -> Retained<NSTextField> {
    let label = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    let control: &NSControl = label.as_ref();
    let font = match style {
        EditorSwitcherLabelStyle::Shortcut => {
            NSFont::monospacedSystemFontOfSize_weight(size, NSFontWeight::from(0))
        }
        _ => NSFont::systemFontOfSize(size),
    };
    control.setFont(Some(font.as_ref()));
    if matches!(style, EditorSwitcherLabelStyle::Shortcut) {
        control.setAlignment(NSTextAlignment(2));
    }
    control.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    control.setUsesSingleLineMode(true);
    let text_color = match style {
        EditorSwitcherLabelStyle::Primary => NSColor::labelColor(),
        EditorSwitcherLabelStyle::Secondary => NSColor::secondaryLabelColor(),
        EditorSwitcherLabelStyle::Footer | EditorSwitcherLabelStyle::Shortcut => {
            NSColor::systemGrayColor()
        }
    };
    label.setTextColor(Some(text_color.as_ref()));
    label.setMaximumNumberOfLines(1);
    label
}

fn set_editor_switcher_view_background(view: &NSView, color: &NSColor, corner_radius: f64) {
    view.setWantsLayer(true);
    if let Some(layer) = view.layer() {
        let cg_color = color.CGColor();
        layer.setBackgroundColor(Some(cg_color.as_ref()));
        layer.setCornerRadius(corner_radius);
        layer.setMasksToBounds(true);
    }
}
