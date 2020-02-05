use std::collections::HashMap;

use lru::LruCache;
use skulpin::skia_safe::{TextBlob, Font as SkiaFont, Typeface, TextBlobBuilder, Data};
use font_kit::{source::SystemSource, metrics::Metrics, properties::{Properties, Weight, Style, Stretch}, family_name::FamilyName, font::Font, };
use skribo::{LayoutSession, FontRef as SkriboFont, FontFamily, FontCollection, TextStyle};

use log::trace;

const STANDARD_CHARACTER_STRING: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890";

const MONOSPACE_FONT: &str = "Fira Code Regular Nerd Font Complete.otf";
const MONOSPACE_BOLD_FONT: &str = "Fira Code Bold Nerd Font Complete.otf";
const SYMBOL_FONT: &str = "DejaVuSansMono.ttf";
const EMOJI_FONT: &str = "NotoColorEmoji.ttf";
const WIDE_FONT: &str = "NotoSansMonoCJKjp-Regular.otf";
const WIDE_BOLD_FONT: &str = "NotoSansMonoCJKjp-Bold.otf";

#[cfg(target_os = "windows")]
const SYSTEM_EMOJI_FONT: &str = "Segoe UI Emoji";

#[cfg(target_os = "macos")]
const SYSTEM_EMOJI_FONT: &str = "Apple Color Emoji";

#[cfg(target_os = "linux")]
const SYSTEM_EMOJI_FONT: &str = "Noto Color Emoji";

#[derive(RustEmbed)]
#[folder = "assets/fonts/"]
struct Asset;

const DEFAULT_FONT_SIZE: f32 = 14.0;

#[derive(new, Clone, Hash, PartialEq, Eq, Debug)]
struct ShapeKey {
    pub text: String,
    pub bold: bool,
    pub italic: bool
}

struct FontSet {
    normal: FontCollection,
    bold: FontCollection,
    italic: FontCollection,
    bold_italic: FontCollection,
}

fn build_collection_by_font_name(font_name: Option<&str>, bold: bool, italic: bool) -> FontCollection {
    let source = SystemSource::new();

    let mut collection = FontCollection::new();

    if let Some(font_name) = font_name {
        let weight = if bold {
            Weight::BOLD
        } else {
            Weight::NORMAL
        };

        let style = if italic {
            Style::Italic
        } else {
            Style::Normal
        };

        let properties = Properties {
            weight, style, stretch: Stretch::NORMAL
        };
        if let Ok(custom) = source.select_best_match(&[FamilyName::Title(font_name.to_string())], &properties) {
            let font = custom.load().unwrap();
            collection.add_family(FontFamily::new_from_font(font));
        }
    }

    let monospace_style = if bold {
        MONOSPACE_BOLD_FONT
    } else {
        MONOSPACE_FONT
    };
    let monospace_data = Asset::get(monospace_style).expect("Failed to read monospace font data");
    let monospace_font = Font::from_bytes(monospace_data.to_vec().into(), 0).expect("Failed to parse monospace font data");
    collection.add_family(FontFamily::new_from_font(monospace_font));

    if let Ok(emoji) = source.select_family_by_name(SYSTEM_EMOJI_FONT) {
        let font = emoji.fonts()[0].load().unwrap();
        collection.add_family(FontFamily::new_from_font(font));
    }

    if cfg!(not(target_os = "macos")) {
        let emoji_data = Asset::get(EMOJI_FONT).expect("Failed to read emoji font data");
        let emoji_font = Font::from_bytes(emoji_data.to_vec().into(), 0).expect("Failed to parse emoji font data");
        collection.add_family(FontFamily::new_from_font(emoji_font));
    }

    let wide_style = if bold {
        WIDE_BOLD_FONT
    } else {
        WIDE_FONT
    };
    let wide_data = Asset::get(wide_style).expect("Failed to read wide font data");
    let wide_font = Font::from_bytes(wide_data.to_vec().into(), 0).expect("Failed to parse wide font data");
    collection.add_family(FontFamily::new_from_font(wide_font));

    let symbol_data = Asset::get(SYMBOL_FONT).expect("Failed to read symbol font data");
    let symbol_font = Font::from_bytes(symbol_data.to_vec().into(), 0).expect("Failed to parse symbol font data");
    collection.add_family(FontFamily::new_from_font(symbol_font));

    collection
}

impl FontSet {
    fn new(font_name: Option<&str>) -> FontSet {
        FontSet {
            normal: build_collection_by_font_name(font_name, false, false),
            bold: build_collection_by_font_name(font_name, true, false),
            italic: build_collection_by_font_name(font_name, false, true),
            bold_italic: build_collection_by_font_name(font_name, true, true),
        }
    }

    fn get(&self, bold: bool, italic: bool) -> &FontCollection {
        match (bold, italic) {
            (false, false) => &self.normal,
            (true, false) => &self.bold,
            (false, true) => &self.italic,
            (true, true) => &self.bold_italic
        }
    }
}

pub struct CachingShaper {
    pub font_name: Option<String>,
    pub base_size: f32,
    font_set: FontSet,
    font_cache: LruCache<String, SkiaFont>,
    blob_cache: LruCache<ShapeKey, Vec<TextBlob>>
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
            font_set: FontSet::new(None),
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
        self.font_set.normal.itemize("a").next().unwrap().1.font.metrics()
    }

    pub fn shape(&mut self, text: &str, bold: bool, italic: bool) -> Vec<TextBlob> {
        let style = TextStyle { size: self.base_size };

        let session = LayoutSession::create(text, &style, &self.font_set.get(bold, italic));

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
        trace!("Font changed {:?} {:?}", &font_name, &base_size);
        self.font_name = font_name.map(|name| name.to_string());
        self.base_size = base_size.unwrap_or(DEFAULT_FONT_SIZE);
        self.font_set = FontSet::new(font_name);
        self.font_cache.clear();
        self.blob_cache.clear();
    }

    pub fn font_base_dimensions(&mut self) -> (f32, f32) {
        let metrics = self.metrics();
        let font_height = (metrics.ascent - metrics.descent) * self.base_size / metrics.units_per_em as f32;

        let style = TextStyle { size: self.base_size };
        let session = LayoutSession::create(STANDARD_CHARACTER_STRING, &style, &self.font_set.normal);

        let layout_run = session.iter_all().next().unwrap();
        let glyph_offsets: Vec<f32> = layout_run.glyphs().map(|glyph| glyph.offset.x).collect();
        let glyph_advances: Vec<f32> = glyph_offsets.windows(2).map(|pair| pair[1] - pair[0]).collect();

        let mut amounts = HashMap::new();
        for advance in glyph_advances.iter() {
            amounts.entry(advance.to_string())
                .and_modify(|e| *e += 1)
                .or_insert(1);
        }
        let (font_width, _) = amounts.into_iter().max_by_key(|(_, count)| *count).unwrap();
        let font_width = font_width.parse::<f32>().unwrap();

        (font_width, font_height)
    }

    pub fn underline_position(&mut self) -> f32 {
        let metrics = self.metrics();
        -metrics.underline_position * self.base_size / metrics.units_per_em as f32
    }
}
