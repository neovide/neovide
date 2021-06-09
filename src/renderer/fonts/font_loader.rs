use std::sync::Arc;

use lru::LruCache;
use skia_safe::{font::Edging, Data, Font, FontHinting, FontMgr, FontStyle, Typeface, Unichar};

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

impl PartialEq for FontPair {
    fn eq(&self, other: &Self) -> bool {
        self.swash_font.key == other.swash_font.key
    }
}

pub struct FontLoader {
    font_mgr: FontMgr,
    cache: LruCache<FontKey, Arc<FontPair>>,
    font_size: f32,
}

#[derive(Hash, PartialEq, Eq, Clone)]
pub enum FontKey {
    Default,
    Name(String),
    Character(char),
}

impl From<&str> for FontKey {
    fn from(string: &str) -> FontKey {
        let string = string.to_string();
        FontKey::Name(string)
    }
}

impl From<&String> for FontKey {
    fn from(string: &String) -> FontKey {
        let string = string.to_owned();
        FontKey::Name(string)
    }
}

impl From<String> for FontKey {
    fn from(string: String) -> FontKey {
        FontKey::Name(string)
    }
}

impl From<char> for FontKey {
    fn from(character: char) -> FontKey {
        FontKey::Character(character)
    }
}

impl FontLoader {
    pub fn new(font_size: f32) -> FontLoader {
        FontLoader {
            font_mgr: FontMgr::new(),
            cache: LruCache::new(10),
            font_size,
        }
    }

    fn load(&mut self, font_key: FontKey) -> Option<FontPair> {
        match font_key {
            FontKey::Default => {
                let default_font_data = Asset::get(DEFAULT_FONT).unwrap();
                let data = Data::new_copy(&default_font_data);
                let typeface = Typeface::from_data(data, 0).unwrap();
                FontPair::new(Font::from_typeface(typeface, self.font_size))
            },
            FontKey::Name(name) => {
                let font_style = FontStyle::normal();
                let typeface = self
                    .font_mgr
                    .match_family_style(name, font_style)?;
                FontPair::new(Font::from_typeface(typeface, self.font_size))
            },
            FontKey::Character(character) => {
                let font_style = FontStyle::normal();
                let typeface = self
                    .font_mgr
                    .match_family_style_character("", font_style, &[], character as i32)?;
                FontPair::new(Font::from_typeface(typeface, self.font_size))
            }
        }
    }

    pub fn get_or_load(&mut self, font_key: FontKey) -> Option<Arc<FontPair>> {
        if let Some(cached) = self.cache.get(&font_key) {
            return Some(cached.clone());
        }

        let loaded_font = self.load(font_key.clone())?;

        let font_arc = Arc::new(loaded_font);

        self.cache.put(font_key, font_arc.clone());

        Some(font_arc)
    }
}
