use std::fmt::{Display, Formatter};

use itertools::Itertools;
use log::{debug, info};

use palette::Srgba;
use vide::{
    parley::{
        context::RangedBuilder,
        style::{FontFamily, FontStack, FontStyle, FontWeight, StyleProperty},
        Layout,
    },
    Shaper,
};

use crate::{error_msg, profiling::tracy_zone, renderer::fonts::font_options::*, units::PixelSize};

#[derive(Debug, Clone)]
struct FontInfo {
    width: f32,
    height: f32,
}

pub struct CachingShaper {
    options: FontOptions,
    scale_factor: f32,
    linespace: f32,
    pub shaper: Shaper,
    font_info: Option<FontInfo>,
}

#[derive(Debug, Default, Hash, PartialEq, Eq, Clone)]
pub struct FontKey {
    // TODO(smolck): Could make these private and add constructor method(s)?
    // Would theoretically make things safer I guess, but not sure . . .
    pub font_desc: Option<FontDescription>,
    pub hinting: FontHinting,
    pub edging: FontEdging,
}

impl Display for FontKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "FontKey {{ font_desc: {:?}, hinting: {:?}, edging: {:?} }}",
            self.font_desc, self.hinting, self.edging
        )
    }
}

impl CachingShaper {
    pub fn new(scale_factor: f32) -> Self {
        let options = FontOptions::default();
        let shaper = Shaper::new();
        let mut ret = Self {
            options: options.clone(),
            scale_factor,
            linespace: 0.0,
            shaper,
            font_info: None,
        };
        ret.update_font_options(options);
        ret
    }

    pub fn current_size(&self) -> f32 {
        let min_font_size = 1.0;
        (self.options.size * self.scale_factor).max(min_font_size)
    }

    pub fn update_scale_factor(&mut self, scale_factor: f32) {
        debug!("scale_factor changed: {:.2}", scale_factor);
        self.scale_factor = scale_factor;
        self.reset();
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
            .into_iter()
            .unique()
            .collect::<Vec<_>>();

        let failed_fonts = keys
            .iter()
            .filter(|font_desc| {
                self.shaper
                    .font_collection()
                    .family_by_name(&font_desc.family)
                    .is_none()
            })
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
            self.reset();
        }
    }

    pub fn update_linespace(&mut self, linespace: f32) {
        debug!("Updating linespace: {}", linespace);
        self.linespace = linespace;
    }

    fn reset(&mut self) {
        self.font_info = None;

        info!("Reset Font Loader -> font_info {:#?}", self.info());
    }

    pub fn font_names(&mut self) -> Vec<String> {
        self.shaper
            .font_collection()
            .family_names()
            .map(|name| name.to_string())
            .sorted()
            .collect_vec()
    }

    fn info(&mut self) -> FontInfo {
        if let Some(info) = &self.font_info {
            return info.clone();
        }
        tracy_zone!("Caculate font info");

        self.shaper.clear_defaults();

        let font_size = self.current_size();
        self.shaper.push_default(StyleProperty::FontSize(font_size));

        let layout = self.shaper.layout_with("M", |builder| {
            if let Some(font_desc) = self.options.primary_font() {
                builder.push_default(&StyleProperty::FontStack(FontStack::Source(
                    &font_desc.family,
                )));
            }
        });
        FontInfo {
            width: layout.width(),
            height: layout.height(),
        }
    }

    fn clamped_linespace(&mut self) -> f32 {
        let info = self.info();
        // Only allow half the font height of negative linespace
        (self.linespace * self.scale_factor).max(-info.height / 2.0)
    }

    pub fn font_base_dimensions(&mut self) -> PixelSize<f32> {
        let info = self.info();

        // The height is always divisible by pixels for better hinting and font clarity
        let font_height = (info.height + self.clamped_linespace()).ceil();
        let font_width = info.width;

        (font_width, font_height).into()
    }

    pub fn baseline_offset(&mut self) -> f32 {
        // The baseline_offset is always rounded to pixels for better hinting and clarity
        (self.clamped_linespace() / 2.0).round()
    }

    #[allow(unused)]
    pub fn underline_position(&mut self) -> f32 {
        // TODO: Fix this
        0.0
    }

    #[allow(unused)]
    pub fn stroke_size(&mut self) -> f32 {
        // TODO: Fix this
        1.0
    }

    pub fn layout_with<'a>(
        &'a mut self,
        text: &'a str,
        style: &CoarseStyle,
        build: impl FnOnce(&mut RangedBuilder<'a, Srgba, &'a str>),
    ) -> Layout<Srgba> {
        self.shaper.layout_with(text, |builder| {
            let font_list = self.options.font_list(*style);
            let fonts = font_list
                .iter()
                .map(|font_desc| FontFamily::Named(&font_desc.family))
                .collect_vec();

            if style.bold {
                builder.push_default(&StyleProperty::FontWeight(FontWeight::BOLD));
            }
            if style.italic {
                builder.push_default(&StyleProperty::FontStyle(FontStyle::Italic));
            }
            builder.push_default(&StyleProperty::FontStack(FontStack::List(&fonts)));
            build(builder);
        })
    }
}
