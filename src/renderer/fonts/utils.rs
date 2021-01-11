use font_kit::properties::{Properties, Stretch, Style, Weight};
use skribo::FontRef as SkriboFont;
use skulpin::skia_safe::{Data, Font as SkiaFont, Typeface};

pub fn build_skia_font_from_skribo_font(
    skribo_font: &SkriboFont,
    base_size: f32,
) -> Option<SkiaFont> {
    let font_data = skribo_font.font.copy_font_data()?;
    let skia_data = Data::new_copy(&font_data[..]);
    let typeface = Typeface::from_data(skia_data, None)?;

    Some(SkiaFont::from_typeface(typeface, base_size))
}

pub fn build_properties(bold: bool, italic: bool) -> Properties {
    let weight = if bold { Weight::BOLD } else { Weight::NORMAL };
    let style = if italic { Style::Italic } else { Style::Normal };
    Properties {
        weight,
        style,
        stretch: Stretch::NORMAL,
    }
}
