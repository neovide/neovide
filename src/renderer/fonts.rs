use std::collections::HashMap;
use skulpin::skia_safe::{Typeface, Font, FontStyle, Paint};
use crate::editor::Style;

pub struct Fonts {
    pub name: String,
    pub size: f32,
    pub normal: Font,
    pub bold: Font,
    pub italic: Font,
    pub bold_italic: Font
}

impl Fonts {
    fn new(name: &str, size: f32) -> Fonts {
        Fonts {
            name: name.to_string(),
            size,
            normal: Font::from_typeface(
                Typeface::new(name, FontStyle::normal()).expect("Could not load normal font file"),
                size),
            bold: Font::from_typeface(
                Typeface::new(name, FontStyle::bold()).expect("Could not load bold font file"),
                size),
            italic: Font::from_typeface(
                Typeface::new(name, FontStyle::italic()).expect("Could not load italic font file"),
                size),
            bold_italic: Font::from_typeface(
                Typeface::new(name, FontStyle::bold_italic()).expect("Could not load bold italic font file"),
                size)
        }
    }

    pub fn get(&self, style: &Style) -> &Font {
        match (style.bold, style.italic) {
            (false, false) => &self.normal,
            (true, false) => &self.bold,
            (false, true)  => &self.italic,
            (true, true) => &self.bold_italic
        }
    }
}

pub struct FontLookup {
    pub name: String,
    pub base_size: f32,
    pub loaded_fonts: HashMap<u16, Fonts>
}

impl FontLookup {
    pub fn new(name: &str, base_size: f32) -> FontLookup {
        let mut lookup = FontLookup {
            name: name.to_string(),
            base_size,
            loaded_fonts: HashMap::new()
        };

        lookup.size(1);
        lookup.size(2);
        lookup.size(3);

        lookup
    }

    pub fn size(&mut self, size_multiplier: u16) -> &Fonts {
        let name = self.name.clone();
        let base_size = self.base_size;
        self.loaded_fonts.entry(size_multiplier).or_insert_with(|| {
            Fonts::new(&name, base_size * size_multiplier as f32)
        })
    }

    pub fn font_base_dimensions(&mut self, paint: &Paint) -> (f32, f32) {
        let base_fonts = self.size(1);

        let (_, metrics) = base_fonts.normal.metrics();
        let font_width = if metrics.avg_char_width > 0.0 {
            metrics.avg_char_width
        } else {
            let (_, bounds) = base_fonts.normal.measure_str("x", Some(&paint));
            bounds.width()
        };
        let font_height = metrics.descent - metrics.ascent;

        (font_width, font_height)
    }
}
