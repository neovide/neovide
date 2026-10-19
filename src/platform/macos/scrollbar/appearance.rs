use objc2_app_kit::{NSAppearanceCustomization, NSBezierPath, NSColor, NSScroller, NSWindow};
use objc2_core_foundation::CGFloat;
use objc2_foundation::NSRect;
use skia_safe::Color4f;

/// Colors used when drawing the legacy appkit scrollbar ourselves.
///
/// NSScrollerStyleLegacy gives us stable hit testing and dragging, but appkit
/// keeps the control visually bright even when the Neovim background is dark.
///
/// These colors let the control keep its appkit behavior while the rendered
/// thumb and track follow neovim resolved default colors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollbarColors {
    pub track: ScrollbarColor,
    pub track_border: ScrollbarColor,
    pub thumb: ScrollbarColor,
    pub active_thumb: ScrollbarColor,
}

impl ScrollbarColors {
    /// Builds a small contrast, preserving scrollbar palette from neovim colors.
    ///
    /// The track stays close to the default background, but is nudged toward
    /// the foreground so the reserved lane reads as scrollbar space. The border
    /// uses a slightly stronger blend to define the curved edge, while the thumb
    /// uses the strongest blend without introducing a separate scrollbar theme
    /// setting.
    pub fn from_default_colors(
        foreground: Option<Color4f>,
        background: Option<Color4f>,
    ) -> Option<Self> {
        let background = background?;
        let foreground = foreground.unwrap_or_else(|| contrasting_color_for(background));

        Some(Self {
            track: ScrollbarColor::from_color(blend(background, foreground, 0.06)),
            track_border: ScrollbarColor::from_color(blend(background, foreground, 0.14)),
            thumb: ScrollbarColor::from_color(blend(background, foreground, 0.34)),
            active_thumb: ScrollbarColor::from_color(blend(background, foreground, 0.48)),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollbarColor {
    red: CGFloat,
    green: CGFloat,
    blue: CGFloat,
    alpha: CGFloat,
}

impl ScrollbarColor {
    fn from_color(color: Color4f) -> Self {
        Self {
            red: CGFloat::from(color.r.clamp(0.0, 1.0)),
            green: CGFloat::from(color.g.clamp(0.0, 1.0)),
            blue: CGFloat::from(color.b.clamp(0.0, 1.0)),
            alpha: CGFloat::from(color.a.clamp(0.0, 1.0)),
        }
    }

    fn fill(self) {
        let color =
            NSColor::colorWithSRGBRed_green_blue_alpha(self.red, self.green, self.blue, self.alpha);
        color.setFill();
    }

    fn stroke(self) {
        let color =
            NSColor::colorWithSRGBRed_green_blue_alpha(self.red, self.green, self.blue, self.alpha);
        color.setStroke();
    }
}

/// Synchronizes the appkit scrollbar with the window appearance that owns it.
///
/// we can force the window to light or dark independent of the system
/// setting. copying the window effective appearance keeps the appkit scroller
/// aligned with that resolved appkit state, including derived variants
/// such as high-contrast appearance.
pub fn sync_scrollbar_appearance(scroller: &NSScroller, window: &NSWindow) {
    let appearance = window.effectiveAppearance();
    scroller.setAppearance(Some(&appearance));
}

pub fn draw_scrollbar_track(rect: NSRect, colors: ScrollbarColors) {
    let rect = extend_rect_vertically(rect, rect.size.width + 1.0);
    fill_rounded_rect(rect, colors.track);
    stroke_rounded_rect(inset_rect(rect, 0.5, 0.5), colors.track_border, 1.0);
}

pub fn draw_scrollbar_thumb(rect: NSRect, colors: ScrollbarColors, active: bool) {
    let rect = thumb_rect(rect);
    if rect.size.width <= 0.0 || rect.size.height <= 0.0 {
        return;
    }

    let color = if active { colors.active_thumb } else { colors.thumb };
    fill_rounded_rect(rect, color);
}

fn thumb_rect(rect: NSRect) -> NSRect {
    // appkit has already inset the knob so we only leave room for the track border.
    inset_rect(rect, 1.0, 3.0)
}

fn fill_rounded_rect(rect: NSRect, color: ScrollbarColor) {
    if rect.size.width <= 0.0 || rect.size.height <= 0.0 {
        return;
    }

    let radius = rect.size.width.min(rect.size.height) / 2.0;
    color.fill();

    let path = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(rect, radius, radius);
    path.fill();
}

fn stroke_rounded_rect(rect: NSRect, color: ScrollbarColor, line_width: CGFloat) {
    if rect.size.width <= 0.0 || rect.size.height <= 0.0 {
        return;
    }

    let radius = rect.size.width.min(rect.size.height) / 2.0;
    color.stroke();
    let path = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(rect, radius, radius);
    path.setLineWidth(line_width);
    path.stroke();
}

fn inset_rect(rect: NSRect, dx: CGFloat, dy: CGFloat) -> NSRect {
    NSRect::new(
        objc2_foundation::NSPoint::new(rect.origin.x + dx, rect.origin.y + dy),
        objc2_foundation::NSSize::new(
            (rect.size.width - dx * 2.0).max(0.0),
            (rect.size.height - dy * 2.0).max(0.0),
        ),
    )
}

fn extend_rect_vertically(rect: NSRect, amount: CGFloat) -> NSRect {
    NSRect::new(
        objc2_foundation::NSPoint::new(rect.origin.x, rect.origin.y - amount),
        objc2_foundation::NSSize::new(rect.size.width, rect.size.height + amount * 2.0),
    )
}

fn blend(background: Color4f, foreground: Color4f, amount: f32) -> Color4f {
    let amount = amount.clamp(0.0, 1.0);
    let inverse = 1.0 - amount;

    Color4f::new(
        background.r * inverse + foreground.r * amount,
        background.g * inverse + foreground.g * amount,
        background.b * inverse + foreground.b * amount,
        1.0,
    )
}

fn contrasting_color_for(background: Color4f) -> Color4f {
    if is_light_color(background) {
        Color4f::new(0.0, 0.0, 0.0, 1.0)
    } else {
        Color4f::new(1.0, 1.0, 1.0, 1.0)
    }
}

fn is_light_color(color: Color4f) -> bool {
    0.2126 * color.r + 0.7152 * color.g + 0.0722 * color.b > 0.5
}
