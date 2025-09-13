use std::{
    fmt::{Display, Formatter},
    num::NonZeroUsize,
    rc::Rc,
};

use log::info;
use lru::LruCache;
use skia_safe::{
    font::Edging as SkiaEdging, Data, Font, FontHinting as SkiaHinting, FontMgr, Typeface,
};
use swash::{shape::ShapeContext, Metrics};

use crate::{
    profiling::tracy_zone,
    renderer::fonts::{
        font_options::{CoarseStyle, FontDescription, FontEdging, FontHinting, DEFAULT_FONT},
        swash_font::SwashFont,
    },
};

static LAST_RESORT_FONT: &[u8] = include_bytes!("../../../assets/fonts/LastResort-Regular.ttf");

pub struct FontPair {
    pub key: FontKey,
    pub skia_font: Font,
    pub swash_font: SwashFont,
    pub font_info: Option<(Metrics, f32)>,
}

fn info_for_font(shape_context: &mut ShapeContext, font: &SwashFont) -> (Metrics, f32) {
    let mut shaper = shape_context.builder(font.as_ref()).build();
    shaper.add_str("M");
    let metrics = shaper.metrics();
    let mut advance = metrics.average_width;
    shaper.shape_with(|cluster| {
        advance = cluster.glyphs.first().map_or(metrics.average_width, |g| {
            g.advance / metrics.units_per_em as f32
        });
    });
    (metrics, advance)
}

impl FontPair {
    fn new(
        key: FontKey,
        typeface: Typeface,
        shaper: Option<&mut ShapeContext>,
    ) -> Option<FontPair> {
        let (font_data, index) = typeface.to_font_data()?;
        // Only the lower 16 bits are part of the index, the rest indicates named instances. But we
        // don't care about those here, since we are just loading the font, so ignore them
        let index = index & 0xFFFF;
        let swash_font = SwashFont::from_data(font_data, index)?;
        let font_info = shaper.map(|shaper| info_for_font(shaper, &swash_font));
        let mut skia_font = Font::from_typeface(typeface, None);
        skia_font.set_subpixel(true);
        skia_font.set_baseline_snap(true);
        skia_font.set_hinting(font_hinting(&key.hinting));
        skia_font.set_edging(font_edging(&key.edging));

        Some(Self {
            key,
            skia_font,
            swash_font,
            font_info,
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
    pub font_desc: FontDescription,
    pub hinting: FontHinting,
    pub edging: FontEdging,
}

pub struct FontLoader {
    font_mgr: FontMgr,
    cache: LruCache<FontKey, Rc<FontPair>>,
    last_resort: Option<Rc<FontPair>>,
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
    pub fn new() -> FontLoader {
        FontLoader {
            font_mgr: FontMgr::new(),
            cache: LruCache::new(NonZeroUsize::new(20).unwrap()),
            last_resort: None,
        }
    }

    fn load(&mut self, font_key: FontKey, shaper: Option<&mut ShapeContext>) -> Option<FontPair> {
        tracy_zone!("load_font");
        info!("Loading font {font_key:?}");
        let desc = &font_key.font_desc;
        let (family, style) = desc.as_family_and_font_style();
        let typeface = self.font_mgr.match_family_style(family, style)?;
        info!("Actually loaded font {:?}", typeface.family_name());
        FontPair::new(font_key, typeface, shaper)
    }

    pub fn get_or_load(
        &mut self,
        font_key: &FontKey,
        shaper: Option<&mut ShapeContext>,
    ) -> Option<Rc<FontPair>> {
        if let Some(cached) = self.cache.get(font_key) {
            return Some(cached.clone());
        }

        let loaded_font = self.load(font_key.clone(), shaper)?;
        let font_rc = Rc::new(loaded_font);
        self.cache.put(font_key.clone(), font_rc.clone());

        Some(font_rc)
    }

    pub fn load_font_for_character(
        &mut self,
        coarse_style: CoarseStyle,
        character: char,
        shaper: Option<&mut ShapeContext>,
    ) -> Option<Rc<FontPair>> {
        let font_style = coarse_style.into();
        let typeface = self.font_mgr.match_family_style_character(
            DEFAULT_FONT,
            font_style,
            &[],
            character as i32,
        )?;

        let font_key = FontKey {
            font_desc: FontDescription {
                family: typeface.family_name(),
                style: coarse_style.name().map(str::to_string),
            },
            hinting: FontHinting::default(),
            edging: FontEdging::default(),
        };

        info!(
            "Load font for character {} {}",
            character,
            typeface.family_name()
        );

        let font_pair = Rc::new(FontPair::new(font_key.clone(), typeface, shaper)?);

        self.cache.put(font_key, font_pair.clone());

        Some(font_pair)
    }

    pub fn get_or_load_last_resort(
        &mut self,
        shaper: Option<&mut ShapeContext>,
    ) -> Option<Rc<FontPair>> {
        log::warn!("Last resort font used");
        if self.last_resort.is_some() {
            self.last_resort.clone()
        } else {
            let font_key = FontKey::default();
            let data = Data::new_copy(LAST_RESORT_FONT);

            let typeface = self.font_mgr.new_from_data(&data, 0)?;
            let font_pair = Rc::new(FontPair::new(font_key, typeface, shaper)?);

            self.last_resort = Some(font_pair.clone());
            Some(font_pair)
        }
    }

    pub fn loaded_fonts(&self) -> Vec<Rc<FontPair>> {
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
