use skia_safe::Color4f;
use winit::window::Theme;

/// Based on formula in https://graphicdesign.stackexchange.com/questions/62368/automatically-select-a-foreground-color-based-on-a-background-color
/// Check if the color is light or dark
fn is_light_color(color: &Color4f) -> bool {
    0.2126 * color.r + 0.7152 * color.g + 0.0722 * color.b > 0.5
}

/// Get the proper dark/light theme for a background_color.
pub fn window_theme_for_background(background_color: Option<Color4f>) -> Option<Theme> {
    background_color?;

    match background_color.unwrap() {
        color if is_light_color(&color) => Some(Theme::Light),
        _ => Some(Theme::Dark),
    }
}
