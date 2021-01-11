use font_kit::{family_handle::FamilyHandle, font::Font, properties::Properties};
use skribo::{FontFamily, FontRef as SkriboFont};

#[derive(Clone)]
pub struct ExtendedFontFamily {
    pub fonts: Vec<SkriboFont>,
}

impl Default for ExtendedFontFamily {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtendedFontFamily {
    pub fn new() -> ExtendedFontFamily {
        ExtendedFontFamily { fonts: Vec::new() }
    }

    pub fn add_font(&mut self, font: SkriboFont) {
        self.fonts.push(font);
    }

    pub fn get(&self, props: Properties) -> Option<&Font> {
        if let Some(first_handle) = &self.fonts.first() {
            for handle in &self.fonts {
                let font = &handle.font;
                let properties = font.properties();

                if properties.weight == props.weight
                    && properties.style == props.style
                    && properties.stretch == props.stretch
                {
                    return Some(&font);
                }
            }

            return Some(&first_handle.font);
        }

        None
    }
}

impl From<FamilyHandle> for ExtendedFontFamily {
    fn from(handle: FamilyHandle) -> Self {
        handle
            .fonts()
            .iter()
            .fold(ExtendedFontFamily::new(), |mut family, font| {
                if let Ok(font) = font.load() {
                    family.add_font(SkriboFont::new(font));
                }
                family
            })
    }
}

impl From<ExtendedFontFamily> for FontFamily {
    fn from(extended_font_family: ExtendedFontFamily) -> Self {
        extended_font_family
            .fonts
            .iter()
            .fold(FontFamily::new(), |mut new_family, font| {
                new_family.add_font(font.clone());
                new_family
            })
    }
}

#[cfg(test)]
mod test {
    use font_kit::properties::{Properties, Stretch, Style, Weight};

    use super::*;
    use crate::renderer::fonts::caching_shaper::Asset;

    const PROPERTIES: Properties = Properties {
        weight: Weight::NORMAL,
        style: Style::Normal,
        stretch: Stretch::NORMAL,
    };
    const EXTRA_SYMBOL_FONT: &str = "Extra Symbols.otf";

    fn dummy_font() -> SkriboFont {
        SkriboFont::new(
            Asset::get(EXTRA_SYMBOL_FONT)
                .and_then(|font_data| Font::from_bytes(font_data.to_vec().into(), 0).ok())
                .unwrap(),
        )
    }

    #[test]
    fn test_add_font() {
        let mut eft = ExtendedFontFamily::new();
        let font = dummy_font();
        eft.add_font(font.clone());
        assert_eq!(
            eft.fonts.first().unwrap().font.full_name(),
            font.font.full_name()
        );
    }

    #[test]
    fn test_get() {
        let mut eft = ExtendedFontFamily::new();
        assert!(eft.get(PROPERTIES).is_none());

        let font = dummy_font();
        eft.fonts.push(font.clone());
        assert_eq!(
            eft.get(font.font.properties()).unwrap().full_name(),
            font.font.full_name()
        );
    }
}
