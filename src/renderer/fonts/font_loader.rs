use std::sync::Arc;

use log::{error, trace};
use lru::LruCache;
use nvim_rs::Value;
use skia_safe::{
    font::Edging as SkiaEdging, Data, Font, FontHinting as SkiaHinting, FontMgr, FontStyle,
    Typeface,
};

use crate::{
    renderer::fonts::swash_font::SwashFont,
    settings::{ParseFromValue, SETTINGS},
    WindowSettings,
};

static DEFAULT_FONT: &[u8] = include_bytes!("../../../assets/fonts/FiraCodeNerdFont-Regular.ttf");
static LAST_RESORT_FONT: &[u8] = include_bytes!("../../../assets/fonts/LastResort-Regular.ttf");

pub struct FontPair {
    pub key: FontKey,
    pub skia_font: Font,
    pub swash_font: SwashFont,
}

impl FontPair {
    fn new(key: FontKey, mut skia_font: Font) -> Option<FontPair> {
        skia_font.set_subpixel(true);

        let settings = SETTINGS.get::<WindowSettings>();
        skia_font.set_hinting(settings.font_hinting.0);
        skia_font.set_edging(settings.font_edging.0);

        let typeface = skia_font.typeface().unwrap();
        let (font_data, index) = typeface.to_font_data().unwrap();
        let swash_font = SwashFont::from_data(font_data, index)?;

        Some(Self {
            key,
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

#[derive(Debug, Default, Hash, PartialEq, Eq, Clone)]
pub struct FontKey {
    // TODO(smolck): Could make these private and add constructor method(s)?
    // Would theoretically make things safer I guess, but not sure . . .
    pub bold: bool,
    pub italic: bool,
    pub family_name: Option<String>,
}

pub struct FontLoader {
    font_mgr: FontMgr,
    cache: LruCache<FontKey, Arc<FontPair>>,
    font_size: f32,
    last_resort: Option<Arc<FontPair>>,
}

impl FontLoader {
    pub fn new(font_size: f32) -> FontLoader {
        FontLoader {
            font_mgr: FontMgr::new(),
            cache: LruCache::new(20),
            font_size,
            last_resort: None,
        }
    }

    fn load(&mut self, font_key: FontKey) -> Option<FontPair> {
        let font_style = font_style(font_key.bold, font_key.italic);

        trace!("Loading font {:?}", font_key);
        if let Some(family_name) = &font_key.family_name {
            let typeface = self.font_mgr.match_family_style(family_name, font_style)?;
            FontPair::new(font_key, Font::from_typeface(typeface, self.font_size))
        } else {
            let data = Data::new_copy(DEFAULT_FONT);
            let typeface = Typeface::from_data(data, 0).unwrap();
            FontPair::new(font_key, Font::from_typeface(typeface, self.font_size))
        }
    }

    pub fn get_or_load(&mut self, font_key: &FontKey) -> Option<Arc<FontPair>> {
        if let Some(cached) = self.cache.get(font_key) {
            return Some(cached.clone());
        }

        let loaded_font = self.load(font_key.clone())?;

        let font_arc = Arc::new(loaded_font);

        self.cache.put(font_key.clone(), font_arc.clone());

        Some(font_arc)
    }

    pub fn load_font_for_character(
        &mut self,
        bold: bool,
        italic: bool,
        character: char,
    ) -> Option<Arc<FontPair>> {
        let font_style = font_style(bold, italic);
        let typeface =
            self.font_mgr
                .match_family_style_character("", font_style, &[], character as i32)?;

        let font_key = FontKey {
            bold,
            italic,
            family_name: Some(typeface.family_name()),
        };

        let font_pair = Arc::new(FontPair::new(
            font_key.clone(),
            Font::from_typeface(typeface, self.font_size),
        )?);

        self.cache.put(font_key, font_pair.clone());

        Some(font_pair)
    }

    pub fn get_or_load_last_resort(&mut self) -> Arc<FontPair> {
        if let Some(last_resort) = self.last_resort.clone() {
            last_resort
        } else {
            let font_key = FontKey::default();
            let data = Data::new_copy(LAST_RESORT_FONT);
            let typeface = Typeface::from_data(data, 0).unwrap();

            let font_pair =
                FontPair::new(font_key, Font::from_typeface(typeface, self.font_size)).unwrap();
            let font_pair = Arc::new(font_pair);

            self.last_resort = Some(font_pair.clone());
            font_pair
        }
    }

    pub fn loaded_fonts(&self) -> Vec<Arc<FontPair>> {
        self.cache.iter().map(|(_, v)| v.clone()).collect()
    }

    pub fn refresh(&mut self, font_pair: &FontPair) {
        self.cache.get(&font_pair.key);
    }

    pub fn font_names(&self) -> Vec<String> {
        self.font_mgr.family_names().collect()
    }
}

fn font_style(bold: bool, italic: bool) -> FontStyle {
    match (bold, italic) {
        (true, true) => FontStyle::bold_italic(),
        (false, true) => FontStyle::italic(),
        (true, false) => FontStyle::bold(),
        (false, false) => FontStyle::normal(),
    }
}

#[derive(Clone, Debug)]
pub struct Hinting(SkiaHinting);

impl Default for Hinting {
    fn default() -> Self {
        Self(SkiaHinting::Full)
    }
}

impl ParseFromValue for Hinting {
    fn parse_from_value(&mut self, value: Value) {
        if value.is_str() {
            let hinting = match value.as_str().unwrap() {
                "full" => SkiaHinting::Full,
                "normal" => SkiaHinting::Normal,
                "slight" => SkiaHinting::Slight,
                _ => SkiaHinting::None,
            };
            *self = Self(hinting)
        } else {
            error!("Expected a Font Edging string, but received {:?}", value);
        }
    }
}

impl From<Hinting> for Value {
    fn from(value: Hinting) -> Value {
        let repr = match value.0 {
            SkiaHinting::Full => "full",
            SkiaHinting::Normal => "normal",
            SkiaHinting::Slight => "slight",
            SkiaHinting::None => "none",
        };
        Value::from(repr)
    }
}

#[derive(Clone, Debug)]
pub struct Edging(SkiaEdging);

impl Default for Edging {
    fn default() -> Self {
        Self(SkiaEdging::AntiAlias)
    }
}

impl ParseFromValue for Edging {
    fn parse_from_value(&mut self, value: Value) {
        if value.is_str() {
            let edging = match value.as_str().unwrap() {
                "alias" => SkiaEdging::Alias,
                "subpixelantialias" | "subpixel" => SkiaEdging::SubpixelAntiAlias,
                "antialias" | _ => SkiaEdging::AntiAlias,
            };
            *self = Self(edging);
        } else {
            error!("Expected a Font Edging string, but received {:?}", value);
        }
    }
}

impl From<Edging> for Value {
    fn from(value: Edging) -> Value {
        let repr = match value.0 {
            SkiaEdging::AntiAlias => "antialias",
            SkiaEdging::SubpixelAntiAlias => "subpixel",
            SkiaEdging::Alias => "alias",
        };
        Value::from(repr)
    }
}
