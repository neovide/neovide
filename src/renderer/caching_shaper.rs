use lru::LruCache;
use skulpin::skia_safe::{Shaper, TextBlob, Font, Point, TextBlobBuilder};
use font_kit::source::SystemSource;
use skribo::{
    layout, layout_run, make_layout, FontCollection, FontFamily, FontRef, Layout, LayoutSession,
    TextStyle
};

#[derive(new, Clone, Hash, PartialEq, Eq)]
struct FontKey {
    pub name: String,
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

    fn get_font(&self, font_key: &FontKey) -> &FontRef {
        if !self.font_cache.contains(font_key) {
            let source = SystemSource::new();
            let font_name = font_key.name.clone();
            let font = source
                .select_family_by_name(&font_name)
                .expect("Failed to load by postscript name")
                .fonts()[0]
                .load()
                .unwrap();
            self.font_cache.put(key.clone(), FontRef::new(font));
        }

        self.font_cache.get(&key).unwrap()
    }

    pub fn shape(&self, text: &str, font_name: &str, scale: u16, bold: bool, italic: bool, font: &Font) -> TextBlob {
        let font_key = FontKey::new(font_name.to_string(), scale, bold, italic);
        let font_ref = self.get_font(&font_key);

        let style = TextStyle { size: font_size };
        let layout = layout_run(&style, &font_ref, standard_character_string);

        let blob_builder = TextBlobBuilder::new();

        unsafe {
            let count = layout.glyphs.count();
            let buffer = blob_builder
                .native_mut()
                .allocRunPosH(font.native(), count.try_into().unwrap(), 0, None);
            let mut glyphs = slice::from_raw_parts_mut((*buffer).glyphs, count);
            for (glyph_id, i) in layout.glyphs.iter().map(|glyph| glyph.glyph_id as u16).enumerate() {
                glyphs[i] = glyph_id;
            }
            let mut positions = slice::from_raw_parts_mut((*buffer).pos, count);
            for (offset, i) in layout.glyphs.iter().map(|glyph| glyph.offset.x as f32).enumerate() {
                positions[i] = offset;
            }
        }

        blob_builder.make()
        // TextBlob::from_pos_text_h(text.as_bytes(), layout.glyphs.iter().
        // let (mut glyphs, mut points) = blob_builder.alloc_run_pos(
        // // let glyph_offsets: Vec<f32> = layout.glyphs.iter().map(|glyph| glyph.offset.x).collect();
        // // let glyph_advances: Vec<f32> = glyph_offsets.windows(2).map(|pair| pair[1] - pair[0]).collect();

        // let (blob, _) = self.shaper.shape_text_blob(text, font, true, 1000000.0, Point::default()).unwrap();
        // blob
    }

    pub fn shape_cached(&mut self, text: &str, font_name: &str, scale: u16, bold: bool, italic: bool, font: &Font) -> &TextBlob {
        let font_key = FontKey::new(font_name.to_string(), scale, bold, italic);
        let key = ShapeKey::new(text.to_string(), font_key);
        if !self.blob_cache.contains(&key) {
            self.blob_cache.put(key.clone(), self.shape(text, font_name, scale, bold, italic, &font));
        }

        self.blob_cache.get(&key).unwrap()
    }

    pub fn clear(&mut self) {
        self.font_cache.clear();
        self.blob_cache.clear();
    }
}
