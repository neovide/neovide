use std::collections::HashMap;

use font_kit::{
    family_name::FamilyName,
    font::Font,
    metrics::Metrics,
    properties::{Properties, Stretch, Style, Weight},
    source::SystemSource,
};
use lru::LruCache;
use skribo::{FontCollection, FontFamily, FontRef as SkriboFont, LayoutSession, TextStyle};
use skulpin::skia_safe::{Data, Font as SkiaFont, TextBlob, TextBlobBuilder, Typeface};

use log::{info, trace, warn};

const STANDARD_CHARACTER_STRING: &str =
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890";

#[cfg(target_os = "windows")]
const SYSTEM_DEFAULT_FONT: &str = "Consolas";

#[cfg(target_os = "linux")]
const SYSTEM_DEFAULT_FONT: &str = "Droid Sans Mono";

#[cfg(target_os = "macos")]
const SYSTEM_DEFAULT_FONT: &str = "Menlo";

#[cfg(target_os = "windows")]
const SYSTEM_SYMBOL_FONT: &str = "Segoe UI Symbol";

#[cfg(target_os = "linux")]
const SYSTEM_SYMBOL_FONT: &str = "Unifont";

#[cfg(target_os = "macos")]
const SYSTEM_SYMBOL_FONT: &str = "Apple Symbols";

#[cfg(target_os = "windows")]
const SYSTEM_EMOJI_FONT: &str = "Segoe UI Emoji";

#[cfg(target_os = "macos")]
const SYSTEM_EMOJI_FONT: &str = "Apple Color Emoji";

#[cfg(target_os = "linux")]
const SYSTEM_EMOJI_FONT: &str = "Noto Color Emoji";

const SYMBOL_FONT: &str = "Extra Symbols.otf";
const MISSING_GLYPHS_FONT: &str = "Missing Glyphs.otf";

#[cfg(feature = "embed-fonts")]
#[derive(RustEmbed)]
#[folder = "assets/fonts/"]
struct Asset;

const DEFAULT_FONT_SIZE: f32 = 14.0;

#[derive(new, Clone, Hash, PartialEq, Eq, Debug)]
struct ShapeKey {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
}

pub struct FontLoader {
    cache: LruCache<String, FontCollection>,
}

impl FontLoader {
    pub fn new() -> FontLoader {
        FontLoader {
            cache: LruCache::new(10),
        }
    }

    pub fn get(&mut self, font_name: &str) -> Option<&FontCollection> {
        let nameref = &font_name.to_string();

        if self.cache.contains(nameref) {
            return self.cache.get(nameref);
        }

        None
    }

    pub fn load_all_variants(&mut self, font_name: &str) -> Option<&FontCollection> {
        let nameref = &font_name.to_string();

        if self.cache.contains(nameref) {
            return self.cache.get(nameref);
        }

        let source = SystemSource::new();
        let mut collection = FontCollection::new();

        source
            .select_family_by_name(font_name)
            .ok()
            .map(|matching_fonts| {
                let fonts = matching_fonts.fonts();

                for font in fonts.into_iter() {
                    if let Some(font) = font.load().ok() {
                        // do something with this
                        let props: Properties = font.properties();

                        collection.add_family(FontFamily::new_from_font(font))
                    }
                }
            });

        self.cache.put(font_name.to_string(), collection);
        self.cache.get(nameref)
    }

    pub fn load(&mut self, font_name: &str, bold: bool, italic: bool) -> Option<&FontCollection> {
        let nameref = &font_name.to_string();

        if self.cache.contains(nameref) {
            return self.cache.get(nameref);
        }

        let source = SystemSource::new();
        let mut collection = FontCollection::new();
        let weight = if bold { Weight::BOLD } else { Weight::NORMAL };
        let style = if italic { Style::Italic } else { Style::Normal };
        let properties = Properties {
            weight,
            style,
            stretch: Stretch::NORMAL,
        };

        if let Ok(custom) =
            source.select_best_match(&[FamilyName::Title(font_name.to_string())], &properties)
        {
            custom
                .load()
                .ok()
                .map(|matching_font| {
                    collection.add_family(FontFamily::new_from_font(matching_font))
                })
                .unwrap_or_else(|| warn!("Could not load gui font"));

            self.cache.put(font_name.to_string(), collection);
        }

        self.cache.get(nameref)
    }

    pub fn load_from_asset(&mut self, font_name: &str) -> Option<&FontCollection> {
        let mut collection = FontCollection::new();
        let nameref = &font_name.to_string();

        if self.cache.contains(nameref) {
            return self.cache.get(nameref);
        }

        Asset::get(font_name)
            .and_then(|font_data| Font::from_bytes(font_data.to_vec().into(), 0).ok())
            .map(|font| collection.add_family(FontFamily::new_from_font(font)));

        self.cache.put(font_name.to_string(), collection);
        self.cache.get(nameref)
    }
}

pub struct CachingShaper {
    pub font_name: Option<String>,
    pub base_size: f32,
    // font_set: FontSet,
    font_loader: FontLoader,
    font_cache: LruCache<String, SkiaFont>,
    blob_cache: LruCache<ShapeKey, Vec<TextBlob>>,
}

fn build_skia_font_from_skribo_font(skribo_font: &SkriboFont, base_size: f32) -> Option<SkiaFont> {
    let font_data = skribo_font.font.copy_font_data()?;
    let skia_data = Data::new_copy(&font_data[..]);
    let typeface = Typeface::from_data(skia_data, None)?;

    Some(SkiaFont::from_typeface(typeface, base_size))
}

impl CachingShaper {
    pub fn new() -> CachingShaper {
        let mut loader = FontLoader::new();

        loader.load_all_variants(SYSTEM_DEFAULT_FONT);

        CachingShaper {
            font_name: Some(String::from(SYSTEM_DEFAULT_FONT)),
            base_size: DEFAULT_FONT_SIZE,
            // font_set: FontSet::new(Some(SYSTEM_DEFAULT_FONT), &mut loader),
            font_loader: loader,
            font_cache: LruCache::new(10),
            blob_cache: LruCache::new(10000),
        }
    }

    fn get_skia_font(&mut self, skribo_font: &SkriboFont) -> Option<&SkiaFont> {
        let font_name = skribo_font.font.postscript_name()?;

        if !self.font_cache.contains(&font_name) {
            let font = build_skia_font_from_skribo_font(skribo_font, self.base_size)?;
            self.font_cache.put(font_name.clone(), font);
        }

        self.font_cache.get(&font_name)
    }

    fn metrics(&mut self) -> Option<Metrics> {
        let var_name = self.font_name.clone().unwrap_or_default();

        if let Some(font) = self.font_loader.get(&var_name) {
            return Some(font.itemize("a").next().unwrap().1.font.metrics());
        }

        None
    }

    pub fn shape(&mut self, text: &str, bold: bool, italic: bool) -> Vec<TextBlob> {
        let style = TextStyle {
            size: self.base_size,
        };

        let var_name = self.font_name.clone().unwrap_or_default();
        let font_collection = self.font_loader.get(&var_name).unwrap();
        let session = LayoutSession::create(text, &style, font_collection);
        let metrics = self.metrics().unwrap();
        let ascent = metrics.ascent * self.base_size / metrics.units_per_em as f32;
        let mut blobs = Vec::new();

        for layout_run in session.iter_all() {
            let skribo_font = layout_run.font();

            if let Some(skia_font) = self.get_skia_font(&skribo_font) {
                let mut blob_builder = TextBlobBuilder::new();
                let count = layout_run.glyphs().count();
                let (glyphs, positions) =
                    blob_builder.alloc_run_pos_h(&skia_font, count, ascent, None);

                for (i, glyph) in layout_run.glyphs().enumerate() {
                    glyphs[i] = glyph.glyph_id as u16;
                    positions[i] = glyph.offset.x;
                }

                blobs.push(blob_builder.make().unwrap());
            } else {
                warn!("Could not load scribo font");
            }
        }

        blobs
    }

    pub fn shape_cached(&mut self, text: &str, bold: bool, italic: bool) -> &Vec<TextBlob> {
        let key = ShapeKey::new(text.to_string(), bold, italic);

        if !self.blob_cache.contains(&key) {
            let blobs = self.shape(text, bold, italic);
            self.blob_cache.put(key.clone(), blobs);
        }

        self.blob_cache.get(&key).unwrap()
    }

    pub fn change_font(&mut self, font_name: Option<&str>, base_size: Option<f32>) {
        trace!("Font changed {:?} {:?}", &font_name, &base_size);
        self.font_name = font_name.map(|name| name.to_string());
        self.base_size = base_size.unwrap_or(DEFAULT_FONT_SIZE);
        self.font_loader
            .load(font_name.unwrap_or_default(), false, false);
        self.font_cache.clear();
        self.blob_cache.clear();
    }

    pub fn font_base_dimensions(&mut self) -> (f32, f32) {
        let var_name = self.font_name.clone().unwrap_or_default();
        let metrics = self.metrics().unwrap();
        let font_collection = self.font_loader.get(&var_name).unwrap();
        let font_height =
            (metrics.ascent - metrics.descent) * self.base_size / metrics.units_per_em as f32;

        let style = TextStyle {
            size: self.base_size,
        };
        let session = LayoutSession::create(STANDARD_CHARACTER_STRING, &style, font_collection);

        let layout_run = session.iter_all().next().unwrap();
        let glyph_offsets: Vec<f32> = layout_run.glyphs().map(|glyph| glyph.offset.x).collect();
        let glyph_advances: Vec<f32> = glyph_offsets
            .windows(2)
            .map(|pair| pair[1] - pair[0])
            .collect();

        let mut amounts = HashMap::new();
        for advance in glyph_advances.iter() {
            amounts
                .entry(advance.to_string())
                .and_modify(|e| *e += 1)
                .or_insert(1);
        }
        let (font_width, _) = amounts.into_iter().max_by_key(|(_, count)| *count).unwrap();
        let font_width = font_width.parse::<f32>().unwrap();

        (font_width, font_height)
    }

    pub fn underline_position(&mut self) -> f32 {
        let metrics = self.metrics().unwrap();
        -metrics.underline_position * self.base_size / metrics.units_per_em as f32
    }
}
