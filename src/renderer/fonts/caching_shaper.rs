use log::{debug, error};

use parley::style::{FontStack, StyleProperty};
use vide::Shaper;

use crate::{error_msg, profiling::tracy_zone, renderer::fonts::font_options::*, units::PixelSize};

#[derive(new, Clone, Hash, PartialEq, Eq, Debug)]
struct ShapeKey {
    pub text: String,
    pub style: CoarseStyle,
}

pub struct CachingShaper {
    options: FontOptions,
    scale_factor: f32,
    pub shaper: Shaper,
}

impl CachingShaper {
    pub fn new(scale_factor: f32) -> Self {
        let options = FontOptions::default();
        let font_size = options.size * scale_factor;
        let mut shaper = Shaper::new();
        shaper.push_default(StyleProperty::FontStack(FontStack::Source(
            "FiraCode Nerd Font",
        )));
        shaper.push_default(StyleProperty::FontSize(font_size));
        Self {
            options,
            scale_factor,
            shaper,
        }
    }

    /*
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
    */

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

        /*
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
        */
    }

    pub fn update_linespace(&mut self, linespace: f32) {
        debug!("Updating linespace: {}", linespace);

        let font_height = self.font_base_dimensions().height;
        let impossible_linespace = font_height + linespace <= 0.0;

        if !impossible_linespace {
            debug!("Linespace updated to: {linespace}");
            //self.linespace = linespace;
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
        tracy_zone!("reset_font_loader");
        /*
        self.font_info = None;
        let font_size = self.current_size();

        self.font_loader = FontLoader::new(font_size);
        let (_, font_width) = self.info();
        info!(
            "Reset Font Loader: font_size: {:.2}px, font_width: {:.2}px",
            font_size, font_width
        );

        self.blob_cache.clear();
        */
    }

    pub fn font_names(&self) -> Vec<String> {
        //self.font_loader.font_names()
        Vec::new()
    }

    /*
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
    */

    pub fn font_base_dimensions(&mut self) -> PixelSize<f32> {
        /*
        let (metrics, glyph_advance) = self.info();

        let bare_font_height = metrics.ascent + metrics.descent + metrics.leading;
        // assuming that linespace is checked on receive for validity
        let font_height = (bare_font_height + self.linespace).ceil();
        let font_width = glyph_advance + self.options.width;

        (font_width, font_height).into()
        */

        (11.487181, 23.0).into()
    }

    /*
    pub fn underline_position(&mut self) -> f32 {
        self.baseline_offset()
    }

    pub fn stroke_size(&mut self) -> f32 {
        1.0
    }
    */

    pub fn baseline_offset(&mut self) -> f32 {
        /*
        let metrics = self.metrics();
        NOTE: leading is also called linegap and should be equally distributed on the top and
        bottom, so it's centered like our linespace settings. That's how it works on the web,
        but some desktop applications only use the top according to:
        https://googlefonts.github.io/gf-guide/metrics.html#8-linegap-values-must-be-0
        metrics.ascent + (metrics.leading + self.linespace) / 2.0
        */
        0.0
    }
}
