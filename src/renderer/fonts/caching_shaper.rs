use log::trace;
use lru::LruCache;
use skia_safe::{Font, FontMetrics, FontMgr, FontStyle, TextBlob};

use super::font_options::*;

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

pub struct CachingShaper {
    pub options: FontOptions,
    font_mgr: FontMgr,
    font_cache: LruCache<String, Font>,
    blob_cache: LruCache<ShapeKey, Vec<TextBlob>>,
}

impl CachingShaper {
    pub fn new() -> CachingShaper {
        CachingShaper {
            options: FontOptions::new(String::from("consolas"), DEFAULT_FONT_SIZE),
            font_mgr: FontMgr::new(),
            font_cache: LruCache::new(10),
            blob_cache: LruCache::new(10000),
        }
    }

    fn metrics(&self) -> FontMetrics {
        let font_name = self.options.fallback_list.first().unwrap();
        let font_style = FontStyle::normal();
        let typeface = self
            .font_mgr
            .match_family_style(font_name, font_style)
            .unwrap();
        let font = Font::from_typeface(typeface, self.options.size);
        let (_, metrics) = font.metrics();
        metrics
    }

    pub fn shape(&mut self, text: &str) -> Vec<TextBlob> {
        let font_name = self.options.fallback_list.first().unwrap();
        let font_style = FontStyle::normal();
        let typeface = self
            .font_mgr
            .match_family_style(font_name, font_style)
            .unwrap();
        let font = Font::from_typeface(typeface, self.options.size);

        let mut blobs = Vec::new();
        let blob = TextBlob::from_str(text, &font).unwrap();
        blobs.push(blob);
        blobs
    }

    pub fn shape_cached(&mut self, text: &str, bold: bool, italic: bool) -> &Vec<TextBlob> {
        let key = ShapeKey::new(text.to_string(), bold, italic);

        if !self.blob_cache.contains(&key) {
            let blobs = self.shape(text);
            self.blob_cache.put(key.clone(), blobs);
        }

        self.blob_cache.get(&key).unwrap()
    }

    pub fn update_font(&mut self, guifont_setting: &str) -> bool {
        let updated = self.options.update(guifont_setting);
        if updated {
            trace!("Font changed: {:?}", self.options);
            self.font_cache.clear();
            self.blob_cache.clear();
        }
        updated
    }

    pub fn font_base_dimensions(&mut self) -> (f32, f32) {
        let metrics = self.metrics();
        let font_height = metrics.descent - metrics.ascent;

        let font_name = self.options.fallback_list.first().unwrap();
        let font_style = FontStyle::normal();
        let typeface = self
            .font_mgr
            .match_family_style(font_name, font_style)
            .unwrap();
        let font = Font::from_typeface(typeface, self.options.size);

        let (text_width, _) = font.measure_str(STANDARD_CHARACTER_STRING, None);
        let font_width = text_width / STANDARD_CHARACTER_STRING.len() as f32;

        (font_width, font_height)
    }

    pub fn underline_position(&self) -> f32 {
        let metrics = self.metrics();
        -metrics.underline_position().unwrap() * self.options.size
    }

    pub fn y_adjustment(&self) -> f32 {
        let metrics = self.metrics();
        metrics.leading - metrics.ascent
    }
}
