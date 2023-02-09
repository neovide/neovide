use std::sync::Arc;

use log::{debug, error, trace, warn};
use lru::LruCache;
use skia_safe::{
    graphics::{font_cache_limit, font_cache_used, set_font_cache_limit},
    TextBlob, TextBlobBuilder,
};
use swash::{
    shape::ShapeContext,
    text::{
        cluster::{CharCluster, Parser, Status, Token},
        Script,
    },
    Metrics,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::renderer::fonts::{font_loader::*, font_options::*};

#[derive(new, Clone, Hash, PartialEq, Eq, Debug)]
struct ShapeKey {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
}

pub struct CachingShaper {
    options: FontOptions,
    font_loader: FontLoader,
    blob_cache: LruCache<ShapeKey, Vec<TextBlob>>,
    shape_context: ShapeContext,
    scale_factor: f32,
    fudge_factor: f32,
    linespace: i64,
}

impl CachingShaper {
    pub fn new(scale_factor: f32) -> CachingShaper {
        let options = FontOptions::default();
        let font_size = options.size * scale_factor;
        let mut shaper = CachingShaper {
            options,
            font_loader: FontLoader::new(font_size),
            blob_cache: LruCache::new(10000),
            shape_context: ShapeContext::new(),
            scale_factor,
            fudge_factor: 1.0,
            linespace: 0,
        };
        shaper.reset_font_loader();
        shaper
    }

    fn current_font_pair(&mut self) -> Arc<FontPair> {
        self.font_loader
            .get_or_load(&FontKey {
                italic: false,
                bold: false,
                family_name: self.options.primary_font(),
                hinting: self.options.hinting.clone(),
                edging: self.options.edging.clone(),
            })
            .unwrap_or_else(|| {
                self.font_loader
                    .get_or_load(&FontKey::default())
                    .expect("Could not load default font")
            })
    }

    pub fn current_size(&self) -> f32 {
        self.options.size * self.scale_factor * self.fudge_factor
    }

    pub fn update_scale_factor(&mut self, scale_factor: f32) {
        debug!("scale_factor changed: {:.2}", scale_factor);
        self.scale_factor = scale_factor;
        self.reset_font_loader();
    }

    pub fn update_font(&mut self, guifont_setting: &str) {
        debug!("Updating font: {}", guifont_setting);

        let options = FontOptions::parse(guifont_setting);
        let font_key = FontKey {
            italic: false,
            bold: false,
            family_name: options.primary_font(),
            hinting: options.hinting.clone(),
            edging: options.edging.clone(),
        };

        if self.font_loader.get_or_load(&font_key).is_some() {
            debug!("Font updated to: {}", guifont_setting);
            self.options = options;
            self.reset_font_loader();
        } else {
            error!("Font can't be updated to: {}", guifont_setting);
        }
    }

    pub fn update_linespace(&mut self, linespace: i64) {
        debug!("Updating linespace: {}", linespace);

        let font_height = self.font_base_dimensions().1;
        let impossible_linespace = font_height as i64 + linespace <= 0;

        if !impossible_linespace {
            debug!("Linespace updated to: {linespace}");
            self.linespace = linespace;
            self.reset_font_loader();
        } else {
            let reason = if impossible_linespace {
                "Linespace too negative, would make font invisible"
            } else {
                "Font not found"
            };
            error!("Linespace can't be updated to {linespace} due to: {reason}");
        }
    }

    fn reset_font_loader(&mut self) {
        self.fudge_factor = 1.0;
        let mut font_size = self.current_size();
        debug!("Original font_size: {:.2}px", font_size);

        self.font_loader = FontLoader::new(font_size);
        let (metrics, font_width) = self.info();

        debug!("Original font_width: {:.2}px", font_width);

        if !self.options.allow_float_size {
            // Calculate the new fudge factor required to scale the font width to the nearest exact pixel
            debug!(
                "Font width: {:.2}px (avg: {:.2}px)",
                font_width, metrics.average_width
            );
            self.fudge_factor = font_width.round() / font_width;
            debug!("Fudge factor: {:.2}", self.fudge_factor);
            font_size = self.current_size();
            debug!("Fudged font size: {:.2}px", font_size);
            debug!("Fudged font width: {:.2}px", self.info().1);
            self.font_loader = FontLoader::new(font_size);
        }
        self.blob_cache.clear();
    }

    pub fn font_names(&self) -> Vec<String> {
        self.font_loader.font_names()
    }

    fn info(&mut self) -> (Metrics, f32) {
        let font_pair = self.current_font_pair();
        let size = self.current_size();
        let mut shaper = self
            .shape_context
            .builder(font_pair.swash_font.as_ref())
            .size(size)
            .build();
        shaper.add_str("M");
        let metrics = shaper.metrics();
        let mut advance = metrics.average_width;
        shaper.shape_with(|cluster| {
            advance = cluster
                .glyphs
                .first()
                .map_or(metrics.average_width, |g| g.advance);
        });
        (metrics, advance)
    }

    fn metrics(&mut self) -> Metrics {
        self.info().0
    }

    pub fn font_base_dimensions(&mut self) -> (u64, u64) {
        let (metrics, glyph_advance) = self.info();
        let font_width = (glyph_advance + 0.5).floor() as u64;

        let bare_font_height = (metrics.ascent + metrics.descent + metrics.leading).ceil();
        let font_height = bare_font_height as i64 + self.linespace;

        (
            font_width,
            font_height as u64, // assuming that linespace is checked on receive for
                                // validity
        )
    }

    pub fn underline_position(&mut self) -> u64 {
        self.metrics().underline_offset as u64
    }

    pub fn y_adjustment(&mut self) -> u64 {
        let metrics = self.metrics();
        (metrics.ascent + metrics.leading).ceil() as u64
    }

    fn build_clusters(
        &mut self,
        text: &str,
        bold: bool,
        italic: bool,
    ) -> Vec<(Vec<CharCluster>, Arc<FontPair>)> {
        let mut cluster = CharCluster::new();

        // Enumerate the characters storing the glyph index in the user data so that we can position
        // glyphs according to Neovim's grid rules
        let mut character_index = 0;
        let mut parser = Parser::new(
            Script::Latin,
            text.graphemes(true)
                .enumerate()
                .flat_map(|(glyph_index, unicode_segment)| {
                    unicode_segment.chars().map(move |character| {
                        let token = Token {
                            ch: character,
                            offset: character_index as u32,
                            len: character.len_utf8() as u8,
                            info: character.into(),
                            data: glyph_index as u32,
                        };
                        character_index += 1;
                        token
                    })
                }),
        );

        let mut results = Vec::new();
        'cluster: while parser.next(&mut cluster) {
            // TODO: Don't redo this work for every cluster. Save it some how
            // Create font fallback list
            let mut font_fallback_keys = Vec::new();

            // Add parsed fonts from guifont
            font_fallback_keys.extend(self.options.font_list.iter().map(|font_name| FontKey {
                italic: self.options.italic || italic,
                bold: self.options.bold || bold,
                family_name: Some(font_name.clone()),
                hinting: self.options.hinting.clone(),
                edging: self.options.edging.clone(),
            }));

            // Add default font
            font_fallback_keys.push(FontKey {
                italic: self.options.italic || italic,
                bold: self.options.bold || bold,
                family_name: None,
                hinting: self.options.hinting.clone(),
                edging: self.options.edging.clone(),
            });

            // Use the cluster.map function to select a viable font from the fallback list and loaded fonts

            let mut best = None;
            // Search through the configured and default fonts for a match
            for fallback_key in font_fallback_keys.iter() {
                if let Some(font_pair) = self.font_loader.get_or_load(fallback_key) {
                    let charmap = font_pair.swash_font.as_ref().charmap();
                    match cluster.map(|ch| charmap.map(ch)) {
                        Status::Complete => {
                            results.push((cluster.to_owned(), font_pair.clone()));
                            continue 'cluster;
                        }
                        Status::Keep => best = Some(font_pair),
                        Status::Discard => {}
                    }
                }
            }

            // Configured font/default didn't work. Search through currently loaded ones
            for loaded_font in self.font_loader.loaded_fonts() {
                let charmap = loaded_font.swash_font.as_ref().charmap();
                match cluster.map(|ch| charmap.map(ch)) {
                    Status::Complete => {
                        results.push((cluster.to_owned(), loaded_font.clone()));
                        self.font_loader.refresh(loaded_font.as_ref());
                        continue 'cluster;
                    }
                    Status::Keep => best = Some(loaded_font),
                    Status::Discard => {}
                }
            }

            if let Some(best) = best {
                results.push((cluster.to_owned(), best.clone()));
            } else {
                let fallback_character = cluster.chars()[0].ch;
                if let Some(fallback_font) =
                    self.font_loader
                        .load_font_for_character(bold, italic, fallback_character)
                {
                    results.push((cluster.to_owned(), fallback_font));
                } else {
                    // Last Resort covers all of the unicode space so we will always have a fallback
                    results.push((
                        cluster.to_owned(),
                        self.font_loader.get_or_load_last_resort(),
                    ));
                }
            }
        }

        // Now we have to group clusters by the font used so that the shaper can actually form
        // ligatures across clusters
        let mut grouped_results = Vec::new();
        let mut current_group = Vec::new();
        let mut current_font_option = None;
        for (cluster, font) in results {
            if let Some(current_font) = current_font_option.clone() {
                if current_font == font {
                    current_group.push(cluster);
                } else {
                    grouped_results.push((current_group, current_font));
                    current_group = vec![cluster];
                    current_font_option = Some(font);
                }
            } else {
                current_group = vec![cluster];
                current_font_option = Some(font);
            }
        }

        if !current_group.is_empty() {
            grouped_results.push((current_group, current_font_option.unwrap()));
        }

        grouped_results
    }

    pub fn adjust_font_cache_size(&self) {
        let current_font_cache_size = font_cache_limit() as f32;
        let percent_font_cache_used = font_cache_used() as f32 / current_font_cache_size;
        if percent_font_cache_used > 0.9 {
            warn!(
                "Font cache is {}% full, increasing cache size",
                percent_font_cache_used * 100.0
            );
            set_font_cache_limit((percent_font_cache_used * 1.5) as usize);
        }
    }

    pub fn shape(&mut self, text: String, bold: bool, italic: bool) -> Vec<TextBlob> {
        let current_size = self.current_size();
        let (glyph_width, ..) = self.font_base_dimensions();

        let mut resulting_blobs = Vec::new();

        trace!("Shaping text: {}", text);

        for (cluster_group, font_pair) in self.build_clusters(&text, bold, italic) {
            let mut shaper = self
                .shape_context
                .builder(font_pair.swash_font.as_ref())
                .size(current_size)
                .build();

            let charmap = font_pair.swash_font.as_ref().charmap();
            for mut cluster in cluster_group {
                cluster.map(|ch| charmap.map(ch));
                shaper.add_cluster(&cluster);
            }

            let mut glyph_data = Vec::new();

            shaper.shape_with(|glyph_cluster| {
                for glyph in glyph_cluster.glyphs {
                    let position = ((glyph.data as u64 * glyph_width) as f32, glyph.y);
                    glyph_data.push((glyph.id, position));
                }
            });

            if glyph_data.is_empty() {
                continue;
            }

            let mut blob_builder = TextBlobBuilder::new();
            let (glyphs, positions) =
                blob_builder.alloc_run_pos(&font_pair.skia_font, glyph_data.len(), None);
            for (i, (glyph_id, glyph_position)) in glyph_data.iter().enumerate() {
                glyphs[i] = *glyph_id;
                positions[i] = (*glyph_position).into();
            }

            let blob = blob_builder.make();
            resulting_blobs.push(blob.expect("Could not create textblob"));
        }

        self.adjust_font_cache_size();

        resulting_blobs
    }

    pub fn shape_cached(&mut self, text: String, bold: bool, italic: bool) -> &Vec<TextBlob> {
        let key = ShapeKey::new(text.clone(), bold, italic);

        if !self.blob_cache.contains(&key) {
            let blobs = self.shape(text, bold, italic);
            self.blob_cache.put(key.clone(), blobs);
        }

        self.blob_cache.get(&key).unwrap()
    }
}
