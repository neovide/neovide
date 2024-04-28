use std::{
    fmt::{Display, Formatter},
    num::NonZeroUsize,
    sync::Arc,
};

use log::trace;
use lru::LruCache;
use skia_safe::{font::Edging as SkiaEdging, Data, Font, FontHinting as SkiaHinting, FontMgr};

use crate::renderer::fonts::font_options::{FontEdging, FontHinting};
use crate::renderer::fonts::swash_font::SwashFont;

use super::font_options::{CoarseStyle, FontDescription};

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
        skia_font.set_baseline_snap(true);
        skia_font.set_hinting(font_hinting(&key.hinting));
        skia_font.set_edging(font_edging(&key.edging));

        let typeface = skia_font.typeface();
        let (font_data, index) = typeface.to_font_data()?;
        // Only the lower 16 bits are part of the index, the rest indicates named instances. But we
        // don't care about those here, since we are just loading the font, so ignore them
        let index = index & 0xFFFF;
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
    pub font_desc: Option<FontDescription>,
    pub hinting: FontHinting,
    pub edging: FontEdging,
}

pub struct FontLoader {
    font_mgr: FontMgr,
    cache: LruCache<FontKey, Arc<FontPair>>,
    font_size: f32,
    last_resort: Option<Arc<FontPair>>,
}

impl Display for FontKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "FontKey {{ font_desc: {:?}, hinting: {:?}, edging: {:?} }}",
            self.font_desc, self.hinting, self.edging
        )
    }
}

impl FontLoader {
    pub fn new(font_size: f32) -> FontLoader {
        FontLoader {
            font_mgr: FontMgr::new(),
            cache: LruCache::new(NonZeroUsize::new(20).unwrap()),
            font_size,
            last_resort: None,
        }
    }

    fn load(&mut self, font_key: FontKey) -> Option<FontPair> {
        trace!("Loading font {:?}", font_key);
        if let Some(desc) = &font_key.font_desc {
            let (family, style) = desc.as_family_and_font_style();
            let typeface = self.font_mgr.match_family_style(family, style)?;
            FontPair::new(font_key, Font::from_typeface(typeface, self.font_size))
        } else {
            let data = Data::new_copy(DEFAULT_FONT);
            let typeface = self.font_mgr.new_from_data(&data, 0)?;
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
        coarse_style: CoarseStyle,
        character: char,
    ) -> Option<Arc<FontPair>> {
        let font_style = coarse_style.into();
        let typeface =
            self.font_mgr
                .match_family_style_character("", font_style, &[], character as i32)?;

        let font_key = FontKey {
            font_desc: Some(FontDescription {
                family: typeface.family_name(),
                style: coarse_style.name().map(str::to_string),
            }),
            hinting: FontHinting::default(),
            edging: FontEdging::default(),
        };

        let font_pair = Arc::new(FontPair::new(
            font_key.clone(),
            Font::from_typeface(typeface, self.font_size),
        )?);

        self.cache.put(font_key, font_pair.clone());

        Some(font_pair)
    }

    pub fn get_or_load_last_resort(&mut self) -> Option<Arc<FontPair>> {
        if self.last_resort.is_some() {
            self.last_resort.clone()
        } else {
            let font_key = FontKey::default();
            let data = Data::new_copy(LAST_RESORT_FONT);

            let typeface = self.font_mgr.new_from_data(&data, 0)?;
            let font_pair = Arc::new(FontPair::new(
                font_key,
                Font::from_typeface(typeface, self.font_size),
            )?);

            self.last_resort = Some(font_pair.clone());
            Some(font_pair)
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

fn font_hinting(hinting: &FontHinting) -> SkiaHinting {
    match hinting {
        FontHinting::Full => SkiaHinting::Full,
        FontHinting::Slight => SkiaHinting::Slight,
        FontHinting::Normal => SkiaHinting::Normal,
        FontHinting::None => SkiaHinting::None,
    }
}

fn font_edging(edging: &FontEdging) -> SkiaEdging {
    match edging {
        FontEdging::AntiAlias => SkiaEdging::AntiAlias,
        FontEdging::Alias => SkiaEdging::Alias,
        FontEdging::SubpixelAntiAlias => SkiaEdging::SubpixelAntiAlias,
    }
}
