use std::sync::Arc;
use std::collections::HashMap;

use lru::LruCache;
use skulpin::skia_safe::{TextBlob, Font as SkiaFont, FontStyle, Typeface, TextBlobBuilder, Data};
use font_kit::{source::SystemSource, metrics::Metrics, properties::Properties, family_name::FamilyName};
use skribo::{layout_run, LayoutSession, FontRef as SkriboFont, FontFamily, FontCollection, TextStyle};

use crate::error_handling::OptionPanicExplanation;

const STANDARD_CHARACTER_STRING: &'static str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890";

#[cfg(target_os = "windows")]
const EMOJI_FONT: &str = "Segoe UI Emoji";

#[cfg(target_os = "macos")]
const EMOJI_FONT: &str = "Apple COlor Emoji";

#[cfg(target_os = "linux")]
const EMOJI_FONT: &str = "Noto Color Emoji";

const DEFAULT_FONT_SIZE: f32 = 14.0;

#[derive(new, Clone, Hash, PartialEq, Eq, Debug)]
struct ShapeKey {
    pub text: String,
    pub bold: bool,
    pub italic: bool
}

pub struct CachingShaper {
    pub font_name: Option<String>,
    pub base_size: f32,
    collection: FontCollection,
    font_cache: LruCache<String, SkiaFont>,
    blob_cache: LruCache<ShapeKey, Vec<TextBlob>>
}

fn build_collection_by_font_name(font_name: Option<&str>) -> FontCollection {
    let source = SystemSource::new();

    let mut collection = FontCollection::new();

    if let Some(font_name) = font_name {
        if let Ok(custom) = source.select_family_by_name(font_name) {
            let font = custom.fonts()[0].load().unwrap();
            collection.add_family(FontFamily::new_from_font(font));
        }
    }

    if let Ok(monospace) = source.select_best_match(&[FamilyName::Monospace], &Properties::new()) {
        let font = monospace.load().unwrap();
        collection.add_family(FontFamily::new_from_font(font));
    }

    if let Ok(emoji) = source.select_family_by_name(EMOJI_FONT) {
        let font = emoji.fonts()[0].load().unwrap();
        collection.add_family(FontFamily::new_from_font(font));
    }

    collection
}

fn build_skia_font_from_skribo_font(skribo_font: &SkriboFont, base_size: f32) -> SkiaFont {
    let font_data = skribo_font.font.copy_font_data().unwrap();
    let skia_data = Data::new_copy(&font_data[..]);
    let typeface = Typeface::from_data(skia_data, None).unwrap();

    SkiaFont::from_typeface(typeface, base_size)
}

impl CachingShaper {
    pub fn new() -> CachingShaper {
        CachingShaper {
            font_name: None,
            base_size: DEFAULT_FONT_SIZE,
            collection: build_collection_by_font_name(None),
            font_cache: LruCache::new(100),
            blob_cache: LruCache::new(10000),
        }
    }


    fn get_skia_font(&mut self, skribo_font: &SkriboFont) -> &SkiaFont {
        let font_name = skribo_font.font.postscript_name().unwrap();
        if !self.font_cache.contains(&font_name) {
            let font = build_skia_font_from_skribo_font(skribo_font, self.base_size);
            self.font_cache.put(font_name.clone(), font);
        }

        self.font_cache.get(&font_name).unwrap()
    }

    fn metrics(&self) -> Metrics {
        self.collection.itemize("a").next().unwrap().1.font.metrics()
    }

    pub fn shape(&mut self, text: &str, bold: bool, italic: bool) -> Vec<TextBlob> {
        let style = TextStyle { size: self.base_size };

        let session = LayoutSession::create(text, &style, &self.collection);

        let metrics = self.metrics();
        let ascent = metrics.ascent * self.base_size / metrics.units_per_em as f32;

        let mut blobs = Vec::new();

        for layout_run in session.iter_all() {
            let skribo_font = layout_run.font();
            let skia_font = self.get_skia_font(&skribo_font);

            let mut blob_builder = TextBlobBuilder::new();

            let count = layout_run.glyphs().count();
            let (glyphs, positions) = blob_builder.alloc_run_pos_h(&skia_font, count, ascent, None);

            for (i, glyph) in layout_run.glyphs().enumerate() {
                glyphs[i] = glyph.glyph_id as u16;
                positions[i] = glyph.offset.x;
            }
            blobs.push(blob_builder.make().unwrap());
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
        self.font_name = font_name.map(|name| name.to_string());
        self.base_size = base_size.unwrap_or(DEFAULT_FONT_SIZE);
        self.collection = build_collection_by_font_name(font_name);
        self.font_cache.clear();
        self.blob_cache.clear();
    }

    pub fn font_base_dimensions(&mut self) -> (f32, f32) {
        let metrics = self.metrics();
        let font_height = (metrics.ascent - metrics.descent) * self.base_size / metrics.units_per_em as f32;

        let style = TextStyle { size: self.base_size };
        let session = LayoutSession::create(STANDARD_CHARACTER_STRING, &style, &self.collection);

        let layout_run = session.iter_all().next().unwrap();
        let glyph_offsets: Vec<f32> = layout_run.glyphs().map(|glyph| glyph.offset.x).collect();
        let glyph_advances: Vec<f32> = glyph_offsets.windows(2).map(|pair| pair[1] - pair[0]).collect();

        let mut amounts = HashMap::new();
        for advance in glyph_advances.iter() {
            amounts.entry(advance.to_string())
                .and_modify(|e| *e += 1)
                .or_insert(1);
        }
        let (font_width, _) = amounts.into_iter().max_by_key(|(_, count)| count.clone()).unwrap();
        let font_width = font_width.parse::<f32>().unwrap();

        (font_width, font_height)
    }

    pub fn underline_position(&mut self) -> f32 {
        let metrics = self.metrics();
        metrics.underline_position
    }
}
