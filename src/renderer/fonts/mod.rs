use skia_safe::font_style::Slant;

use crate::renderer::FontSettings;
use crate::settings::SETTINGS;

pub mod caching_shaper;
mod font_loader;
mod font_options;
mod font_weight;
mod swash_font;

pub use font_weight::FontWeight;

fn slant(italic: bool) -> Slant {
    if italic {
        let settings = SETTINGS.get::<FontSettings>();
        if settings.use_italic_as_oblique {
            Slant::Oblique
        } else {
            Slant::Italic
        }
    } else {
        Slant::Upright
    }
}
