use std::sync::Arc;

use log::trace;
use lru::LruCache;
use skia_safe::{
    font::Edging as SkiaEdging, Data, Font, FontHinting as SkiaHinting, FontMgr, FontStyle,
    Typeface,
};
use webrender_api::{
    FontInstanceKey, FontInstanceOptions, FontInstancePlatformOptions, FontKey as ApiFontKey,
    FontRenderMode, FontTemplate, IdNamespace,
};
use wr_glyph_rasterizer::{BaseFontInstance, FontInstance};

use super::{
    font_options::{FontEdging, FontHinting},
    glyph_cache::GlyphCache,
    swash_font::SwashFont,
};

static DEFAULT_FONT: &[u8] = include_bytes!("../../../assets/fonts/FiraCodeNerdFont-Regular.ttf");
static LAST_RESORT_FONT: &[u8] = include_bytes!("../../../assets/fonts/LastResort-Regular.ttf");

pub struct FontPair {
    pub key: FontKey,
    pub skia_font: Font,
    pub swash_font: SwashFont,
    pub api_key: ApiFontKey,
    pub template: FontTemplate,
    pub base_instance: Arc<BaseFontInstance>,
}

impl FontPair {
    fn new(key: FontKey, api_key: ApiFontKey, mut skia_font: Font) -> Option<FontPair> {
        skia_font.set_subpixel(true);
        skia_font.set_hinting(font_hinting(&key.hinting));
        skia_font.set_edging(font_edging(&key.edging));

        let typeface = skia_font.typeface().unwrap();
        let (font_data, index) = typeface.to_font_data().unwrap();
        let swash_font = SwashFont::from_data(font_data.clone(), index)?;
        let template = FontTemplate::Raw(Arc::new(font_data), index as u32);

        let instance_key = FontInstanceKey(api_key.0, api_key.1);
        let size = skia_font.size().into();
        let options = FontInstanceOptions {
            render_mode: FontRenderMode::Mono, // TODO: Enable other render modes
            ..Default::default()
        };
        let platform_options = Some(FontInstancePlatformOptions::default()); //TODO: Set the correct
        let variations = Vec::new();

        let base_instance = Arc::new(BaseFontInstance {
            instance_key,
            font_key: api_key,
            size,
            options,
            platform_options,
            variations,
        });

        Some(Self {
            key,
            skia_font,
            swash_font,
            api_key,
            template,
            base_instance,
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
    pub hinting: FontHinting,
    pub edging: FontEdging,
}

pub struct FontLoader {
    font_mgr: FontMgr,
    cache: LruCache<FontKey, Arc<FontPair>>,
    font_size: f32,
    last_resort: Option<Arc<FontPair>>,
    pub glyph_cache: GlyphCache,
    namespace: IdNamespace,
    next_id: u32,
}

impl FontLoader {
    pub fn new(font_size: f32) -> FontLoader {
        FontLoader {
            font_mgr: FontMgr::new(),
            cache: LruCache::new(20),
            font_size,
            last_resort: None,
            glyph_cache: GlyphCache::new(),
            namespace: IdNamespace(0),
            next_id: 0,
        }
    }

    fn new_api_key(&mut self) -> ApiFontKey {
        let id = self.next_id;
        self.next_id += 1;
        ApiFontKey(self.namespace, id)
    }

    fn load(&mut self, font_key: FontKey) -> Option<FontPair> {
        let font_style = font_style(font_key.bold, font_key.italic);

        trace!("Loading font {:?}", font_key);
        if let Some(family_name) = &font_key.family_name {
            let typeface = self.font_mgr.match_family_style(family_name, font_style)?;
            FontPair::new(
                font_key,
                self.new_api_key(),
                Font::from_typeface(typeface, self.font_size),
            )
        } else {
            let data = Data::new_copy(DEFAULT_FONT);
            let typeface = Typeface::from_data(data, 0).unwrap();
            FontPair::new(
                font_key,
                self.new_api_key(),
                Font::from_typeface(typeface, self.font_size),
            )
        }
    }

    fn put_to_cache(&mut self, font_key: FontKey, font_arch: Arc<FontPair>) {
        self.glyph_cache
            .rasterizer
            .add_font(font_arch.api_key, font_arch.template.clone());
        if let Some(removed_font) = self.cache.push(font_key, font_arch) {
            self.glyph_cache
                .rasterizer
                .delete_font(removed_font.1.api_key);
        }
    }

    pub fn get_or_load(&mut self, font_key: &FontKey) -> Option<Arc<FontPair>> {
        if let Some(cached) = self.cache.get(font_key) {
            return Some(cached.clone());
        }

        let loaded_font = self.load(font_key.clone())?;

        let font_arc = Arc::new(loaded_font);

        self.put_to_cache(font_key.clone(), font_arc.clone());

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
            hinting: FontHinting::default(),
            edging: FontEdging::default(),
        };

        let font_pair = Arc::new(FontPair::new(
            font_key.clone(),
            self.new_api_key(),
            Font::from_typeface(typeface, self.font_size),
        )?);

        self.put_to_cache(font_key, font_pair.clone());

        Some(font_pair)
    }

    pub fn get_or_load_last_resort(&mut self) -> Arc<FontPair> {
        if let Some(last_resort) = self.last_resort.clone() {
            last_resort
        } else {
            let font_key = FontKey::default();
            let data = Data::new_copy(LAST_RESORT_FONT);
            let typeface = Typeface::from_data(data, 0).unwrap();

            let font_pair = FontPair::new(
                font_key,
                self.new_api_key(),
                Font::from_typeface(typeface, self.font_size),
            )
            .unwrap();
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
