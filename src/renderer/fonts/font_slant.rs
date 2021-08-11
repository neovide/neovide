use skia_safe::font_style::Slant;

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum FontSlant {
    Upright,
    Italic,
    Oblique,
}

impl From<Slant> for FontSlant {
    fn from(slant: Slant) -> Self {
        match slant {
            Slant::Upright => FontSlant::Upright,
            Slant::Italic => FontSlant::Italic,
            Slant::Oblique => FontSlant::Oblique,
        }
    }
}

impl From<FontSlant> for Slant {
    fn from(slant: FontSlant) -> Self {
        match slant {
            FontSlant::Upright => Slant::Upright,
            FontSlant::Italic => Slant::Italic,
            FontSlant::Oblique => Slant::Oblique,
        }
    }
}
