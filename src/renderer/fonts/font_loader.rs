use std::iter;
use std::sync::Arc;

use lru::LruCache;
use skia_safe::{
    font::Edging, Data, Font, FontHinting, FontMgr, FontStyle, TextBlob, TextBlobBuilder, Typeface,
};

use super::swash_font::SwashFont;

#[derive(RustEmbed)]
#[folder = "assets/fonts/"]
pub struct Asset;

const DEFAULT_FONT: &str = "FiraCode-Regular.ttf";

pub struct FontPair {
    pub skia_font: Font,
    pub swash_font: SwashFont,
}

impl FontPair {
    fn new(mut skia_font: Font) -> Option<FontPair> {
        skia_font.set_subpixel(true);
        skia_font.set_hinting(FontHinting::Full);
        skia_font.set_edging(Edging::SubpixelAntiAlias);

        let (font_data, index) = skia_font.typeface().unwrap().to_font_data().unwrap();
        let swash_font = SwashFont::from_data(font_data, index)?;

        Some(Self {
            skia_font,
            swash_font,
        })
    }
}

pub struct FontLoader {
    font_mgr: FontMgr,
    cache: LruCache<Option<String>, Arc<FontPair>>,
    font_size: f32,
}

impl FontLoader {
    pub fn new(font_size: f32) -> FontLoader {
        FontLoader {
            font_mgr: FontMgr::new(),
            cache: LruCache::new(10),
            font_size,
        }
    }

    fn load(&mut self, font_name: &str) -> Option<FontPair> {
        let font_style = FontStyle::normal();
        let typeface = self
            .font_mgr
            .match_family_style(font_name, font_style)
            .unwrap();
        FontPair::new(Font::from_typeface(typeface, self.font_size))
    }

    fn load_default(&mut self) -> Option<FontPair> {
        let default_font_data = Asset::get(DEFAULT_FONT).unwrap();
        let data = Data::new_copy(&default_font_data);
        let typeface = Typeface::from_data(data, 0).unwrap();
        FontPair::new(Font::from_typeface(typeface, self.font_size))
    }

    pub fn get_or_load(&mut self, font_name: Option<String>) -> Option<Arc<FontPair>> {
        if let Some(cached) = self.cache.get(&font_name) {
            return Some(cached.clone());
        }

        let loaded_font = if let Some(font_name) = &font_name {
            self.load(font_name)?
        } else {
            self.load_default()?
        };

        let font_arc = Arc::new(loaded_font);

        self.cache.put(font_name, font_arc.clone());

        Some(font_arc)
    }
}

#[cfg(test)]
mod test {
    use font_kit::{
        font::Font,
        properties::{Properties, Stretch, Style, Weight},
    };
    use skribo::FontRef as SkriboFont;

    use super::*;
    use crate::renderer::fonts::utils::*;

    const PROPERTIES1: Properties = Properties {
        weight: Weight::NORMAL,
        style: Style::Normal,
        stretch: Stretch::NORMAL,
    };

    const PROPERTIES2: Properties = Properties {
        weight: Weight::BOLD,
        style: Style::Normal,
        stretch: Stretch::NORMAL,
    };

    const PROPERTIES3: Properties = Properties {
        weight: Weight::NORMAL,
        style: Style::Italic,
        stretch: Stretch::NORMAL,
    };

    const PROPERTIES4: Properties = Properties {
        weight: Weight::BOLD,
        style: Style::Italic,
        stretch: Stretch::NORMAL,
    };

    fn dummy_font() -> SkriboFont {
        SkriboFont::new(
            Asset::get(EXTRA_SYMBOL_FONT)
                .and_then(|font_data| Font::from_bytes(font_data.to_vec().into(), 0).ok())
                .unwrap(),
        )
    }

    #[test]
    fn test_build_properties() {
        assert_eq!(build_properties(false, false), PROPERTIES1);
        assert_eq!(build_properties(true, false), PROPERTIES2);
        assert_eq!(build_properties(false, true), PROPERTIES3);
        assert_eq!(build_properties(true, true), PROPERTIES4);
    }

    #[test]
    fn test_load_from_asset() {
        let mut loader = FontLoader::new();

        let font_family = loader.load_from_asset("");
        assert!(font_family.is_none());

        let font = dummy_font();
        let mut eft = ExtendedFontFamily::new();
        eft.add_font(font.clone());
        let font_family = loader.load_from_asset(EXTRA_SYMBOL_FONT);
        let result = font_family.unwrap().fonts.first().unwrap().font.full_name();
        assert_eq!(&result, &eft.fonts.first().unwrap().font.full_name());

        assert_eq!(
            &result,
            &loader
                .cache
                .get(&EXTRA_SYMBOL_FONT.to_string())
                .unwrap()
                .fonts
                .first()
                .unwrap()
                .font
                .full_name()
        );
    }

    #[test]
    fn test_load() {
        let mut loader = FontLoader::new();
        let junk_text = "uhasiudhaiudshiaushd";
        let font_family = loader.load(junk_text);
        assert!(font_family.is_none());

        #[cfg(target_os = "linux")]
        const SYSTEM_DEFAULT_FONT: &str = "monospace";

        let font_family = loader.load(SYSTEM_DEFAULT_FONT);
        let result = font_family.unwrap().fonts.first().unwrap().font.full_name();
        assert_eq!(
            &result,
            &loader
                .cache
                .get(&SYSTEM_DEFAULT_FONT.to_string())
                .unwrap()
                .fonts
                .first()
                .unwrap()
                .font
                .full_name()
        );
    }

    #[test]
    fn test_get_random_system_font() {
        let mut loader = FontLoader::new();

        let font_family = loader.get_random_system_font_family();
        let font_name = loader.random_font_name.unwrap();
        let result = font_family.unwrap().fonts.first().unwrap().font.full_name();
        assert_eq!(
            &result,
            &loader
                .cache
                .get(&font_name)
                .unwrap()
                .fonts
                .first()
                .unwrap()
                .font
                .full_name()
        );
    }
}
