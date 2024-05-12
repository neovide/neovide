use std::{num::NonZeroUsize, sync::Arc};

use itertools::Itertools;
use log::{debug, error, info, trace};
use lru::LruCache;
use skia_safe::{graphics::set_font_cache_limit, TextBlob, TextBlobBuilder};
use swash::{
    shape::ShapeContext,
    text::{
        cluster::{CharCluster, Parser, Status, Token},
        Script,
    },
    Metrics,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::{
    error_msg,
    profiling::tracy_zone,
    renderer::fonts::{font_loader::*, font_options::*},
    units::PixelSize,
};

#[derive(new, Clone, Hash, PartialEq, Eq, Debug)]
struct ShapeKey {
    pub text: String,
    pub style: CoarseStyle,
}

const FONT_CACHE_SIZE: usize = 8 * 1024 * 1024;

pub struct CachingShaper {
    options: FontOptions,
    font_loader: FontLoader,
    blob_cache: LruCache<ShapeKey, Vec<TextBlob>>,
    shape_context: ShapeContext,
    scale_factor: f32,
    linespace: f32,
    font_info: Option<(Metrics, f32)>,
}

impl CachingShaper {
    pub fn new(scale_factor: f32) -> CachingShaper {
        let options = FontOptions::default();
        let font_size = options.size * scale_factor;
        let mut shaper = CachingShaper {
            options,
            font_loader: FontLoader::new(font_size),
            blob_cache: LruCache::new(NonZeroUsize::new(10000).unwrap()),
            shape_context: ShapeContext::new(),
            scale_factor,
            linespace: 0.0,
            font_info: None,
        };
        shaper.reset_font_loader();
        shaper
    }

    fn current_font_pair(&mut self) -> Arc<FontPair> {
        self.font_loader
            .get_or_load(&FontKey {
                font_desc: self.options.primary_font(),
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
        let min_font_size = 1.0;
        (self.options.size * self.scale_factor).max(min_font_size)
    }

    pub fn update_scale_factor(&mut self, scale_factor: f32) {
        debug!("scale_factor changed: {:.2}", scale_factor);
        self.scale_factor = scale_factor;
        self.reset_font_loader();
    }

    pub fn update_font(&mut self, guifont_setting: &str) {
        debug!("Updating font: {}", guifont_setting);

        let options = match FontOptions::parse(guifont_setting) {
            Ok(opt) => opt,
            Err(msg) => {
                error_msg!("Failed to parse guifont: {}", msg);
                return;
            }
        };

        self.update_font_options(options);
    }

    pub fn update_font_options(&mut self, options: FontOptions) {
        debug!("Updating font options: {:?}", options);

        let keys = options
            .possible_fonts()
            .iter()
            .map(|desc| FontKey {
                font_desc: Some(desc.clone()),
                hinting: options.hinting.clone(),
                edging: options.edging.clone(),
            })
            .unique()
            .collect::<Vec<_>>();

        let failed_fonts = keys
            .iter()
            .filter(|key| self.font_loader.get_or_load(key).is_none())
            .collect_vec();

        if !failed_fonts.is_empty() {
            error_msg!(
                "Font can't be updated to: {:#?}\n\
                Following fonts couldn't be loaded: {}",
                options,
                failed_fonts.iter().join(",\n"),
            );
        }

        if failed_fonts.len() != keys.len() {
            debug!("Font updated to: {:?}", options);
            self.options = options;
            self.reset_font_loader();
        }
    }

    pub fn update_linespace(&mut self, linespace: f32) {
        debug!("Updating linespace: {}", linespace);

        let font_height = self.font_base_dimensions().height;
        let impossible_linespace = font_height + linespace <= 0.0;

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
        self.font_info = None;
        let font_size = self.current_size();

        self.font_loader = FontLoader::new(font_size);
        let (_, font_width) = self.info();
        info!(
            "Reset Font Loader: font_size: {:.2}px, font_width: {:.2}px",
            font_size, font_width
        );

        self.blob_cache.clear();
    }

    pub fn font_names(&self) -> Vec<String> {
        self.font_loader.font_names()
    }

    fn info(&mut self) -> (Metrics, f32) {
        if let Some(info) = self.font_info {
            return info;
        }

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
        self.font_info = Some((metrics, advance));
        (metrics, advance)
    }

    fn metrics(&mut self) -> Metrics {
        tracy_zone!("font_metrics");
        self.info().0
    }

    pub fn font_base_dimensions(&mut self) -> PixelSize<f32> {
        let (metrics, glyph_advance) = self.info();

        let bare_font_height = metrics.ascent + metrics.descent + metrics.leading;
        // assuming that linespace is checked on receive for validity
        let font_height = (bare_font_height + self.linespace).ceil();
        let font_width = glyph_advance + self.options.width;

        (font_width, font_height).into()
    }

    pub fn underline_position(&mut self) -> f32 {
        self.metrics().underline_offset
    }

    pub fn y_adjustment(&mut self) -> f32 {
        let metrics = self.metrics();
        metrics.ascent + metrics.leading + self.linespace / 2.
    }

    fn build_clusters(
        &mut self,
        text: &str,
        style: CoarseStyle,
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

            // Add parsed fonts from guifont or config file
            font_fallback_keys.extend(
                self.options
                    .font_list(style)
                    .iter()
                    .map(|font_desc| FontKey {
                        font_desc: Some(font_desc.clone()),
                        hinting: self.options.hinting.clone(),
                        edging: self.options.edging.clone(),
                    })
                    .unique(),
            );

            // Add default font
            font_fallback_keys.push(FontKey {
                font_desc: None,
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
                if let Some(fallback_font) = self
                    .font_loader
                    .load_font_for_character(style, fallback_character)
                {
                    results.push((cluster.to_owned(), fallback_font));
                } else {
                    // Last Resort covers all of the unicode space so we will always have a fallback
                    results.push((
                        cluster.to_owned(),
                        self.font_loader.get_or_load_last_resort().unwrap(),
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

    pub fn cleanup_font_cache(&self) {
        tracy_zone!("purge_font_cache");
        set_font_cache_limit(FONT_CACHE_SIZE / 2);
        set_font_cache_limit(FONT_CACHE_SIZE);
    }

    pub fn shape(&mut self, text: String, style: CoarseStyle) -> Vec<TextBlob> {
        let current_size = self.current_size();
        let glyph_width = self.font_base_dimensions().width;

        let mut resulting_blobs = Vec::new();

        trace!("Shaping text: {:?}", text);

        for (cluster_group, font_pair) in self.build_clusters(&text, style) {
            let features = self.get_font_features(
                font_pair
                    .as_ref()
                    .key
                    .font_desc
                    .as_ref()
                    .map(|desc| desc.family.as_str()),
            );

            let mut shaper = self
                .shape_context
                .builder(font_pair.swash_font.as_ref())
                .features(features.iter().map(|(name, value)| (name.as_ref(), *value)))
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
                    let position = (glyph.data as f32 * glyph_width, glyph.y);
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

        resulting_blobs
    }

    pub fn shape_cached(&mut self, text: String, style: CoarseStyle) -> &Vec<TextBlob> {
        tracy_zone!("shape_cached");
        let key = ShapeKey::new(text.clone(), style);

        if !self.blob_cache.contains(&key) {
            let blobs = self.shape(text, style);
            self.blob_cache.put(key.clone(), blobs);
        }

        self.blob_cache.get(&key).unwrap()
    }

    fn get_font_features(&self, name: Option<&str>) -> Vec<(String, u16)> {
        if let Some(name) = name {
            self.options
                .features
                .get(name)
                .map(|features| {
                    features
                        .iter()
                        .map(|feature| (feature.0.clone(), feature.1))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        } else {
            vec![]
        }
    }
}
