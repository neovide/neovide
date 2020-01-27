use std::sync::Arc;
use std::collections::HashMap;

use lru::LruCache;
use skulpin::skia_safe::{TextBlob, Font as SkiaFont, FontStyle, Typeface, TextBlobBuilder};
use font_kit::source::SystemSource;
use skribo::{layout_run, LayoutSession, FontRef as SkriboFont, FontFamily, FontCollection, TextStyle};

use crate::error_handling::OptionPanicExplanation;

const STANDARD_CHARACTER_STRING: &'static str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890";

#[cfg(target_os = "windows")]
const DEFAULT_FONT: &str = "Consolas";
#[cfg(target_os = "windows")]
const EMOJI_FONT: &str = "Segoe UI Emoji";

#[cfg(target_os = "macos")]
const DEFAULT_FONT: &str = "Menlo";
#[cfg(target_os = "macos")]
const EMOJI_FONT: &str = "Apple COlor Emoji";

#[cfg(target_os = "linux")]
const DEFAULT_FONT: &str = "Monospace";
#[cfg(target_os = "linux")]
const EMOJI_FONT: &str = "Noto Color Emoji";

const DEFAULT_FONT_SIZE: f32 = 14.0;


#[derive(new, Clone, Hash, PartialEq, Eq, Debug)]
struct FontKey {
    pub scale: u16,
    pub bold: bool,
    pub italic: bool
}

#[derive(new, Clone, Hash, PartialEq, Eq, Debug)]
struct ShapeKey {
    pub text: String,
    pub font_key: FontKey
}

struct FontPair {
    normal: (SkiaFont, SkriboFont),
    emoji: Option<(SkiaFont, SkriboFont)>
}

#[derive(Debug)]
pub struct CachingShaper {
    pub font_name: String,
    pub base_size: f32,
    font_cache: LruCache<FontKey, FontPair>,
    blob_cache: LruCache<ShapeKey, Vec<TextBlob>>
}

fn build_fonts(font_key: &FontKey, font_name: &str, base_size: f32) -> Option<(SkiaFont, SkriboFont)> {
    let source = SystemSource::new();
    let skribo_font = SkriboFont::new(
        source.select_family_by_name(font_name)
              .ok()?
              .fonts()[0]
              .load()
              .ok()?);
     
    let font_style = match (font_key.bold, font_key.italic) {
        (false, false) => FontStyle::normal(),
        (true, false) => FontStyle::bold(),
        (false, true) => FontStyle::italic(),
        (true, true) => FontStyle::bold_italic()
    };
    let skia_font = SkiaFont::from_typeface(
        Typeface::new(font_name.clone(), font_style)?,
        base_size * font_key.scale as f32);

    Some((skia_font, skribo_font))
}

impl CachingShaper {
    pub fn new() -> CachingShaper {
        CachingShaper {
            font_name: DEFAULT_FONT.to_string(),
            base_size: DEFAULT_FONT_SIZE,
            font_cache: LruCache::new(100),
            blob_cache: LruCache::new(10000),
        }
    }

    fn get_font_pair(&mut self, font_key: &FontKey) -> &FontPair {
        if !self.font_cache.contains(font_key) {
            let font_pair = FontPair {
                normal: build_fonts(font_key, &self.font_name, self.base_size)
                    .unwrap_or_explained_panic(
                        "Could not load configured font", 
                        &format!("Could not load {}. This font was either the font configured in init scripts via guifont, or the default font is not available on your machine. Defaults are defined here: https://github.com/Kethku/neovide/blob/master/src/renderer/caching_shaper.rs", &self.font_name)),
                emoji: build_fonts(font_key, EMOJI_FONT, self.base_size)
            };
            self.font_cache.put(font_key.clone(), font_pair);
        }

        self.font_cache.get(font_key).unwrap()
    }

    pub fn shape(&mut self, text: &str, scale: u16, bold: bool, italic: bool) -> Vec<TextBlob> {
        let base_size = self.base_size;
        let font_key = FontKey::new(scale, bold, italic);
        let font_pair = self.get_font_pair(&font_key);

        let style = TextStyle { size: base_size * scale as f32 };

        let mut collection = FontCollection::new();

        let mut normal_family = FontFamily::new();
        normal_family.add_font(font_pair.normal.1.clone());
        collection.add_family(normal_family);

        if let Some(emoji_font_pair) = &font_pair.emoji {
            let mut emoji_family = FontFamily::new();
            emoji_family.add_font(emoji_font_pair.1.clone());
            collection.add_family(emoji_family);
        }

        let session = LayoutSession::create(text, &style, &collection);

        let metrics = font_pair.normal.1.font.metrics();
        let ascent = metrics.ascent * base_size / metrics.units_per_em as f32;

        let mut blobs = Vec::new();

        for layout_run in session.iter_all() {
            let skribo_font = layout_run.font();
            let skia_font = if Arc::ptr_eq(&skribo_font.font, &font_pair.normal.1.font) {
                &font_pair.normal.0
            } else {
                &font_pair.emoji.as_ref().unwrap().0
            };

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

    pub fn shape_cached(&mut self, text: &str, scale: u16, bold: bool, italic: bool) -> &Vec<TextBlob> {
        let font_key = FontKey::new(scale, bold, italic);
        let key = ShapeKey::new(text.to_string(), font_key);
        if !self.blob_cache.contains(&key) {
            let blobs = self.shape(text, scale, bold, italic);
            self.blob_cache.put(key.clone(), blobs);
        }

        self.blob_cache.get(&key).unwrap()
    }

    pub fn change_font(&mut self, font_name: Option<&str>, base_size: Option<f32>) {
        self.font_cache.clear();
        self.blob_cache.clear();
        self.font_name = font_name.unwrap_or(DEFAULT_FONT).to_string();
        self.base_size = base_size.unwrap_or(DEFAULT_FONT_SIZE);
    }

    pub fn font_base_dimensions(&mut self) -> (f32, f32) {
        let base_size = self.base_size;
        let font_key = FontKey::new(1, false, false);
        let (skia_font, skribo_font) = &self.get_font_pair(&font_key).normal;

        let (_, metrics) = skia_font.metrics();
        let font_height = metrics.descent - metrics.ascent;

        let style = TextStyle { size: base_size };
        let layout = layout_run(&style, &skribo_font, STANDARD_CHARACTER_STRING);
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

    pub fn underline_position(&mut self, scale: u16) -> f32 {
        let font_key = FontKey::new(scale, false, false);
        let (skia_font, _) = &self.get_font_pair(&font_key).normal;

        let (_, metrics) = skia_font.metrics();
        metrics.underline_position().unwrap()
    }
}
