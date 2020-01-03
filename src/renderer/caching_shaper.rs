use std::collections::HashMap;
use std::rc::Rc;

use lru::LruCache;
use skulpin::skia_safe::{Shaper, TextBlob, Font, Point, TextBlobBuilder};
use font_kit::source::SystemSource;
use skribo::{
    layout, layout_run, make_layout, FontCollection, FontFamily, FontRef, Layout, LayoutSession,
    TextStyle
};

use super::fonts::FontLookup;

const standard_character_string: &'static str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890";

#[derive(new, Clone, Hash, PartialEq, Eq)]
struct FontKey {
    pub name: String,
    pub base_size: String, // hack because comparison of floats doesn't work
    pub scale: u16,
    pub bold: bool,
    pub italic: bool
}

#[derive(new, Clone, Hash, PartialEq, Eq)]
struct ShapeKey {
    pub text: String,
    pub font_key: FontKey
}

pub struct CachingShaper {
    font_cache: LruCache<FontKey, FontRef>,
    blob_cache: LruCache<ShapeKey, TextBlob>
}

impl CachingShaper {
    pub fn new() -> CachingShaper {
        CachingShaper {
            font_cache: LruCache::new(100),
            blob_cache: LruCache::new(10000)
        }
    }

    fn get_font(&mut self, font_key: &FontKey) -> &FontRef {
        if !self.font_cache.contains(font_key) {
            let source = SystemSource::new();
            let font_name = font_key.name.clone();
            let font = source
                .select_family_by_name(&font_name)
                .expect("Failed to load by postscript name")
                .fonts()[0]
                .load()
                .unwrap();
            self.font_cache.put(font_key.clone(), FontRef::new(font));
        }

        self.font_cache.get(font_key).unwrap()
    }

    pub fn shape(&mut self, text: &str, font_name: &str, base_size: f32, scale: u16, bold: bool, italic: bool, font: &Font) -> TextBlob {
        let font_key = FontKey::new(font_name.to_string(), base_size.to_string(), scale, bold, italic);
        let font_ref = self.get_font(&font_key);

        let style = TextStyle { size: base_size * scale as f32 };
        let layout = layout_run(&style, &font_ref, text);

        let mut blob_builder = TextBlobBuilder::new();

        
        let count = layout.glyphs.len();
        let metrics = font_ref.font.metrics();
        let ascent = metrics.ascent * base_size / metrics.units_per_em as f32;
        let (glyphs, positions) = blob_builder.alloc_run_pos_h(font, count, ascent, None);

        for (i, glyph_id) in layout.glyphs.iter().map(|glyph| glyph.glyph_id as u16).enumerate() {
            glyphs[i] = glyph_id;
        }
        for (i, offset) in layout.glyphs.iter().map(|glyph| glyph.offset.x as f32).enumerate() {
            positions[i] = offset;
        }

        blob_builder.make().unwrap()
    }

    pub fn shape_cached(&mut self, text: &str, font_name: &str, base_size: f32, scale: u16, bold: bool, italic: bool, font: &Font) -> &TextBlob {
        let font_key = FontKey::new(font_name.to_string(), base_size.to_string(), scale, bold, italic);
        let key = ShapeKey::new(text.to_string(), font_key);
        if !self.blob_cache.contains(&key) {
            let blob = self.shape(text, font_name, base_size, scale, bold, italic, &font);
            self.blob_cache.put(key.clone(), blob);
        }

        self.blob_cache.get(&key).unwrap()
    }

    pub fn clear(&mut self) {
        self.font_cache.clear();
        self.blob_cache.clear();
    }

    pub fn font_base_dimensions(&mut self, font_lookup: &mut FontLookup) -> (f32, f32) {
        let base_fonts = font_lookup.size(1);
        let normal_font = &base_fonts.normal;
        let (_, metrics) = normal_font.metrics();
        let font_height = metrics.descent - metrics.ascent;

        let font_key = FontKey::new(font_lookup.name.to_string(), font_lookup.base_size.to_string(), 1, false, false);
        let font_ref = self.get_font(&font_key);
        let style = TextStyle { size: font_lookup.base_size };
        let layout = layout_run(&style, font_ref, standard_character_string);
        let glyph_offsets: Vec<f32> = layout.glyphs.iter().map(|glyph| glyph.offset.x).collect();
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
}
