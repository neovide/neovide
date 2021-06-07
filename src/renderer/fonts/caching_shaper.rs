use std::sync::Arc;

use lru::LruCache;
use skia_safe::{FontMetrics, FontMgr, TextBlob, TextBlobBuilder};
use swash::shape::ShapeContext;
use swash::text::cluster::{CharCluster, Parser, Status, Token};
use swash::text::Script;

use super::font_loader::*;
use super::font_options::*;

const STANDARD_CHARACTER_STRING: &str =
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890";
const DEFAULT_FONT_SIZE: f32 = 14.0;

#[derive(new, Clone, Hash, PartialEq, Eq, Debug)]
struct ShapeKey {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
}

pub struct CachingShaper {
    pub options: Option<FontOptions>,
    font_loader: FontLoader,
    blob_cache: LruCache<ShapeKey, Vec<TextBlob>>,
    shape_context: ShapeContext,
}

impl CachingShaper {
    pub fn new() -> CachingShaper {
        CachingShaper {
            options: None,
            font_loader: FontLoader::new(DEFAULT_FONT_SIZE),
            blob_cache: LruCache::new(10000),
            shape_context: ShapeContext::new(),
        }
    }

    fn current_font_pair(&mut self) -> Arc<FontPair> {
        let font_name = self
            .options
            .as_ref()
            .map(|options| options.fallback_list.first().unwrap().clone());
        self.font_loader
            .get_or_load(font_name)
            .unwrap_or_else(|| self.font_loader.get_or_load(None).unwrap())
    }

    pub fn current_size(&self) -> f32 {
        self.options
            .as_ref()
            .map(|options| options.size)
            .unwrap_or(DEFAULT_FONT_SIZE)
    }

    fn metrics(&mut self) -> FontMetrics {
        let font_pair = self.current_font_pair();
        let (_, metrics) = font_pair.skia_font.metrics();
        metrics
    }

    pub fn update_font(&mut self, guifont_setting: &str) -> bool {
        let new_options = FontOptions::parse(guifont_setting, DEFAULT_FONT_SIZE);

        if new_options != self.options && new_options.is_some() {
            self.font_loader = FontLoader::new(new_options.as_ref().unwrap().size);
            self.blob_cache.clear();
            self.options = new_options;

            true
        } else {
            false
        }
    }

    pub fn font_base_dimensions(&mut self) -> (f32, f32) {
        let metrics = self.metrics();
        let font_height = metrics.descent - metrics.ascent;

        let font_pair = self.current_font_pair();

        let (text_width, _) = font_pair
            .skia_font
            .measure_str(STANDARD_CHARACTER_STRING, None);
        let font_width = text_width / STANDARD_CHARACTER_STRING.len() as f32;

        (font_width, font_height)
    }

    pub fn underline_position(&mut self) -> f32 {
        let metrics = self.metrics();
        -metrics.underline_position().unwrap() * self.current_size()
    }

    pub fn y_adjustment(&mut self) -> f32 {
        let metrics = self.metrics();
        -metrics.ascent
    }

    fn build_clusters(&mut self, text: &str) -> Vec<(CharCluster, Arc<FontPair>)> {
        let mut cluster = CharCluster::new();
        let mut parser = Parser::new(
            Script::Latin,
            text.char_indices().map(|(i, ch)| Token {
                ch,
                offset: i as u32,
                len: ch.len_utf8() as u8,
                info: ch.into(),
                data: 0,
            }),
        );

        let mut results = Vec::new();
        while parser.next(&mut cluster) {
            if let Some(options) = &self.options {
                let mut best = None;
                for font_name in options.fallback_list.iter() {
                    if let Some(font_pair) =
                        self.font_loader.get_or_load(Some(font_name.to_owned()))
                    {
                        let charmap = font_pair.swash_font.as_ref().charmap();
                        match cluster.map(|ch| charmap.map(ch)) {
                            Status::Complete => {
                                results.push((cluster.to_owned(), font_pair.clone()));
                                break;
                            }
                            Status::Keep => best = Some(font_pair.clone()),
                            Status::Discard => {}
                        }
                    }
                }

                if let Some(best) = best {
                    results.push((cluster.to_owned(), best.clone()));
                }
            } else {
                let default_font = self
                    .font_loader
                    .get_or_load(None)
                    .expect("Could not load default font");
                results.push((cluster.to_owned(), default_font));
            }
        }
        results
    }

    pub fn shape(&mut self, text: &str) -> Vec<TextBlob> {
        let current_size = self.current_size();

        let mut resulting_blobs = Vec::new();

        let mut current_point: f32 = 0.0;
        for (mut cluster, font_pair) in self.build_clusters(text) {
            let mut shaper = self
                .shape_context
                .builder(font_pair.swash_font.as_ref())
                .size(current_size)
                .build();

            let charmap = font_pair.swash_font.as_ref().charmap();
            cluster.map(|ch| charmap.map(ch));
            shaper.add_cluster(&cluster);

            let mut glyph_data = Vec::new();

            shaper.shape_with(|glyph_cluster| {
                for glyph in glyph_cluster.glyphs {
                    glyph_data.push((glyph.id, glyph.advance));
                }
            });

            if glyph_data.is_empty() {
                return Vec::new();
            }

            let mut blob_builder = TextBlobBuilder::new();
            let (glyphs, positions) =
                blob_builder.alloc_run_pos_h(&font_pair.skia_font, glyph_data.len(), 0.0, None);
            for (i, (glyph_id, glyph_advance)) in glyph_data.iter().enumerate() {
                glyphs[i] = *glyph_id;
                positions[i] = current_point.floor();
                current_point += glyph_advance;
            }

            let blob = blob_builder.make();
            resulting_blobs.push(blob.expect("Could not create textblob"));
        }

        resulting_blobs
    }

    pub fn shape_cached(&mut self, text: &str, bold: bool, italic: bool) -> &Vec<TextBlob> {
        let key = ShapeKey::new(text.to_string(), bold, italic);

        if !self.blob_cache.contains(&key) {
            let blobs = self.shape(text);
            self.blob_cache.put(key.clone(), blobs);
        }

        self.blob_cache.get(&key).unwrap()
    }
}
