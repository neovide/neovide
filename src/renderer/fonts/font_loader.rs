use std::iter;

use cfg_if::cfg_if;
use font_kit::{properties::Properties, source::SystemSource};
use lru::LruCache;
use rand::Rng;
use skribo::{FontCollection, FontFamily};

#[cfg(any(feature = "embed-fonts", test))]
use super::caching_shaper::Asset;
use super::extended_font_family::*;

cfg_if! {
    if #[cfg(target_os = "windows")] {
        pub const SYSTEM_DEFAULT_FONT: &str = "Consolas";
        pub const SYSTEM_SYMBOL_FONT: &str = "Segoe UI Symbol";
        pub const SYSTEM_EMOJI_FONT: &str = "Segoe UI Emoji";
    } else if #[cfg(target_os = "linux")] {
        pub const SYSTEM_DEFAULT_FONT: &str = "Noto Sans Mono";
        pub const SYSTEM_SYMBOL_FONT: &str = "Noto Sans Mono";
        pub const SYSTEM_EMOJI_FONT: &str = "Noto Color Emoji";
    } else if #[cfg(target_os = "macos")] {
        pub const SYSTEM_DEFAULT_FONT: &str = "Menlo";
        pub const SYSTEM_SYMBOL_FONT: &str = "Apple Symbols";
        pub const SYSTEM_EMOJI_FONT: &str = "Apple Color Emoji";
    }
}

pub const EXTRA_SYMBOL_FONT: &str = "Extra Symbols.otf";
pub const MISSING_GLYPH_FONT: &str = "Missing Glyphs.otf";

pub struct FontLoader {
    cache: LruCache<String, ExtendedFontFamily>,
    source: SystemSource,
    random_font_name: Option<String>,
}

impl FontLoader {
    pub fn new() -> FontLoader {
        FontLoader {
            cache: LruCache::new(10),
            source: SystemSource::new(),
            random_font_name: None,
        }
    }

    fn get(&mut self, font_name: &str) -> Option<ExtendedFontFamily> {
        self.cache.get(&String::from(font_name)).cloned()
    }

    #[cfg(any(feature = "embed-fonts", test))]
    fn load_from_asset(&mut self, font_name: &str) -> Option<ExtendedFontFamily> {
        use font_kit::font::Font;
        use skribo::FontRef as SkriboFont;
        let mut family = ExtendedFontFamily::new();

        if let Some(font) = Asset::get(font_name)
            .and_then(|font_data| Font::from_bytes(font_data.to_vec().into(), 0).ok())
        {
            family.add_font(SkriboFont::new(font));
            self.cache.put(String::from(font_name), family);
            self.get(font_name)
        } else {
            None
        }
    }

    #[cfg(not(any(feature = "embed-fonts", test)))]
    fn load_from_asset(&self, font_name: &str) -> Option<ExtendedFontFamily> {
        log::warn!(
            "Tried to load {} from assets but build didn't include embed-fonts feature",
            font_name
        );
        None
    }

    fn load(&mut self, font_name: &str) -> Option<ExtendedFontFamily> {
        let handle = match self.source.select_family_by_name(font_name) {
            Ok(it) => it,
            _ => return None,
        };

        if !handle.is_empty() {
            let family = ExtendedFontFamily::from(handle);
            self.cache.put(String::from(font_name), family);
            self.get(font_name)
        } else {
            None
        }
    }

    fn get_random_system_font_family(&mut self) -> Option<ExtendedFontFamily> {
        if let Some(font) = self.random_font_name.clone() {
            self.get(&font)
        } else {
            let font_names = self.source.all_families().expect("fonts exist");
            let n = rand::thread_rng().gen::<usize>() % font_names.len();
            let font_name = &font_names[n];
            self.random_font_name = Some(font_name.clone());
            self.load(&font_name)
        }
    }

    pub fn get_or_load(&mut self, font_name: &str) -> Option<ExtendedFontFamily> {
        if let Some(cached) = self.get(font_name) {
            Some(cached)
        } else if let Some(loaded) = self.load(font_name) {
            Some(loaded)
        } else {
            self.load_from_asset(font_name)
        }
    }

    pub fn build_collection_by_font_name(
        &mut self,
        fallback_list: &[String],
        properties: Properties,
    ) -> FontCollection {
        let mut collection = FontCollection::new();

        let gui_fonts = fallback_list
            .iter()
            .map(|fallback_item| fallback_item.as_ref())
            .chain(iter::once(SYSTEM_DEFAULT_FONT));

        for font_name in gui_fonts {
            if let Some(family) = self.get_or_load(font_name) {
                if let Some(font) = family.get(properties) {
                    collection.add_family(FontFamily::new_from_font(font.clone()));
                }
            }
        }

        for font in &[
            SYSTEM_SYMBOL_FONT,
            SYSTEM_EMOJI_FONT,
            EXTRA_SYMBOL_FONT,
            MISSING_GLYPH_FONT,
        ] {
            if let Some(family) = self.get_or_load(font) {
                collection.add_family(FontFamily::from(family));
            }
        }

        if self.cache.is_empty() {
            let font_family = self.get_random_system_font_family();
            collection.add_family(FontFamily::from(font_family.expect("font family loaded")));
        }
        collection
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
    #[ignore]
    fn test_load() {
        let mut loader = FontLoader::new();
        let junk_text = "uhasiudhaiudshiaushd";
        let font_family = loader.load(junk_text);
        assert!(font_family.is_none());

        #[cfg(target_os = "linux")]
        const SYSTEM_DEFAULT_FONT: &str = "DejaVu Serif";

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
