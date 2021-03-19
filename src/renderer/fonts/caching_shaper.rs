use cfg_if::cfg_if;
use log::trace;
use lru::LruCache;
use rustybuzz::{shape, Face, UnicodeBuffer};
use skia_safe::{Font, FontMetrics, FontMgr, FontStyle, TextBlob, TextBlobBuilder};

use super::font_options::*;

const STANDARD_CHARACTER_STRING: &str =
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890";

cfg_if! {
    if #[cfg(target_os = "windows")] {
        pub const SYSTEM_DEFAULT_FONT: &str = "Consolas";
    } else if #[cfg(target_os = "linux")] {
        pub const SYSTEM_DEFAULT_FONT: &str = "Ubuntu";
    } else if #[cfg(target_os = "macos")] {
        pub const SYSTEM_DEFAULT_FONT: &str = "Menlo";
    }
}

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
    blob_cache: LruCache<ShapeKey, Vec<TextBlob>>,
}

impl CachingShaper {
    pub fn new() -> CachingShaper {
        CachingShaper {
            options: FontOptions::new(String::from(SYSTEM_DEFAULT_FONT), DEFAULT_FONT_SIZE),
            font_mgr: FontMgr::new(),
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
        let units_per_em = typeface.units_per_em().unwrap() as f32;

        let (data, index) = typeface.to_font_data().expect("Could not get font data");
        let face = Face::from_slice(&data, index as u32).expect("Could not create font face");
        let mut unicode_buffer = UnicodeBuffer::new();
        unicode_buffer.push_str(text);

        let shaped_glyphs = shape(&face, &[], unicode_buffer);
        let shaped_positions = shaped_glyphs.glyph_positions();
        let shaped_infos = shaped_glyphs.glyph_infos();

        let font = Font::from_typeface(typeface, self.options.size);
        let mut blob_builder = TextBlobBuilder::new();
        let (glyphs, positions) =
            blob_builder.alloc_run_pos_h(&font, shaped_glyphs.len(), 0.0, None);
        let mut current_point = 0.0;
        for (i, (shaped_position, shaped_info)) in
            shaped_positions.iter().zip(shaped_infos).enumerate()
        {
            glyphs[i] = shaped_info.codepoint as u16;
            positions[i] = current_point;
            current_point += shaped_position.x_advance as f32 * self.options.size / units_per_em;
        }
        vec![blob_builder.make().expect("Could not create textblob")]
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
        -metrics.ascent
    }
}
