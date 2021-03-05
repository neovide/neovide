use std::collections::HashMap;

use font_kit::metrics::Metrics;
use log::{trace, warn};
use lru::LruCache;
use skia_safe::{Font as SkiaFont, TextBlob, TextBlobBuilder};
use skribo::{FontCollection, FontRef as SkriboFont, LayoutSession, TextStyle};

use super::font_loader::*;
use super::font_options::*;
use super::utils::*;

const STANDARD_CHARACTER_STRING: &str =
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890";

#[cfg(any(feature = "embed-fonts", test))]
#[derive(RustEmbed)]
#[folder = "assets/fonts/"]
pub struct Asset;

const DEFAULT_FONT_SIZE: f32 = 14.0;

#[derive(new, Clone, Hash, PartialEq, Eq, Debug)]
struct ShapeKey {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
}

struct FontSet {
    normal: FontCollection,
    bold: FontCollection,
    italic: FontCollection,
}

impl FontSet {
    fn new(fallback_list: &[String], loader: &mut FontLoader) -> FontSet {
        FontSet {
            normal: loader
                .build_collection_by_font_name(fallback_list, build_properties(false, false)),
            bold: loader
                .build_collection_by_font_name(fallback_list, build_properties(true, false)),
            italic: loader
                .build_collection_by_font_name(fallback_list, build_properties(false, true)),
        }
    }

    fn get(&self, bold: bool, italic: bool) -> &FontCollection {
        match (bold, italic) {
            (true, _) => &self.bold,
            (false, false) => &self.normal,
            (false, true) => &self.italic,
        }
    }
}

pub struct CachingShaper {
    pub options: FontOptions,
    font_set: FontSet,
    font_loader: FontLoader,
    font_cache: LruCache<String, SkiaFont>,
    blob_cache: LruCache<ShapeKey, Vec<TextBlob>>,
}

impl CachingShaper {
    pub fn new() -> CachingShaper {
        let options = FontOptions::new(String::from(SYSTEM_DEFAULT_FONT), DEFAULT_FONT_SIZE);
        let mut loader = FontLoader::new();
        let font_set = FontSet::new(&options.fallback_list, &mut loader);

        CachingShaper {
            options,
            font_set,
            font_loader: loader,
            font_cache: LruCache::new(10),
            blob_cache: LruCache::new(10000),
        }
    }

    fn get_skia_font(&mut self, skribo_font: &SkriboFont) -> Option<&SkiaFont> {
        let font_name = skribo_font.font.postscript_name()?;
        if !self.font_cache.contains(&font_name) {
            let font = build_skia_font_from_skribo_font(skribo_font, self.options.size)?;
            self.font_cache.put(font_name.clone(), font);
        }

        self.font_cache.get(&font_name)
    }

    fn metrics(&self) -> Metrics {
        self.font_set
            .normal
            .itemize("a")
            .next()
            .expect("Cannot get font metrics")
            .1
            .font
            .metrics()
    }

    pub fn shape(&mut self, text: &str, bold: bool, italic: bool) -> Vec<TextBlob> {
        let style = TextStyle {
            size: self.options.size,
        };
        let session = LayoutSession::create(text, &style, &self.font_set.get(bold, italic));
        let metrics = self.metrics();
        let ascent = metrics.ascent * self.options.size / metrics.units_per_em as f32;
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
                    positions[i] = glyph.offset.x();
                }

                blobs.push(blob_builder.make().unwrap());
            } else {
                warn!("Could not load skribo font");
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

    pub fn update_font(&mut self, guifont_setting: &str) -> bool {
        let updated = self.options.update(guifont_setting);
        if updated {
            trace!("Font changed: {:?}", self.options);
            self.font_set = FontSet::new(&self.options.fallback_list, &mut self.font_loader);
            self.font_cache.clear();
            self.blob_cache.clear();
        }
        updated
    }

    pub fn font_base_dimensions(&mut self) -> (f32, f32) {
        let metrics = self.metrics();
        let font_height =
            (metrics.ascent - metrics.descent) * self.options.size / metrics.units_per_em as f32;
        let style = TextStyle {
            size: self.options.size,
        };
        let session =
            LayoutSession::create(STANDARD_CHARACTER_STRING, &style, &self.font_set.normal);
        let layout_run = session.iter_all().next().unwrap();
        let glyph_offsets: Vec<f32> = layout_run.glyphs().map(|glyph| glyph.offset.x()).collect();
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
        let metrics = self.metrics();
        -metrics.underline_position * self.options.size / metrics.units_per_em as f32
    }
}
