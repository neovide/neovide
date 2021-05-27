use std::sync::Arc;

use lru::LruCache;
use skia_safe::{font::Edging, Data, Font, FontHinting, FontMgr, FontStyle, Typeface};

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
